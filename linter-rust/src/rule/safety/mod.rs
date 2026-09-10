use crate::{
    Analysis, Source,
    declaration::Index,
    scope::{integration, mark_tests},
};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use std::fs;
use syn::{Meta, Token, punctuated::Punctuated};
use tree_sitter::Node;
mod config;
mod rationale;
pub use config::Config;
use config::{Assertion, Scope};

pub struct UnsafeBoundary(Vec<Assertion>);
impl Rule for UnsafeBoundary {
    const ID: &'static str = "rust/unsafe-boundary";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }
    fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let index = Index::new(analysis, &root);
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            let mut comments = Vec::new();
            rationale::comments(source.syntax.root_node(), &mut comments);
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                Scan {
                    source,
                    index: &index,
                    tests: &tests,
                    comments: &comments,
                    assertion,
                    findings: &mut findings,
                }
                .visit(source.syntax.root_node(), false);
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
struct Scan<'a, 'b> {
    source: &'a Source,
    index: &'b Index<'a>,
    tests: &'b [bool],
    comments: &'b [Node<'a>],
    assertion: &'b Assertion,
    findings: &'b mut Vec<Finding>,
}
impl Scan<'_, '_> {
    fn allowed(&self, node: Node<'_>) -> bool {
        if self
            .assertion
            .allowed_targets
            .as_ref()
            .is_some_and(|selector| selector.matches(&self.source.path))
        {
            return true;
        }
        let owner = attribute_owner(node);
        let mut identity = self.index.identity(self.source, owner);
        if node.kind() == "attribute_item"
            && owner.kind() == "mod_item"
            && let Some(name) = owner.child_by_field_name("name")
        {
            identity.module.push(
                self.source.text[name.byte_range()]
                    .trim_start_matches("r#")
                    .into(),
            );
        }
        self.assertion
            .allowed_modules
            .iter()
            .any(|module| identity.module.starts_with(module))
    }
    fn visit(&mut self, node: Node<'_>, macro_context: bool) {
        if matches!(
            node.kind(),
            "line_comment"
                | "block_comment"
                | "string_literal"
                | "raw_string_literal"
                | "char_literal"
        ) {
            return;
        }
        let selected = match self.assertion.scope {
            Scope::Production => self.tests.get(node.start_byte()) != Some(&true),
            Scope::Tests => self.tests.get(node.start_byte()) == Some(&true),
            Scope::All => true,
        };
        if selected {
            self.inspect(node, macro_context);
        }
        if selected && matches!(node.kind(), "attribute_item" | "inner_attribute_item") {
            return;
        }
        let macro_context =
            macro_context || matches!(node.kind(), "macro_invocation" | "macro_definition");
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, macro_context);
        }
    }
    fn inspect(&mut self, node: Node<'_>, macro_context: bool) {
        if matches!(node.kind(), "attribute_item" | "inner_attribute_item") {
            if weak_attribute(node, &self.source.text) && !self.allowed(node) {
                self.report(
                    node,
                    "attribute weakens unsafe_code outside an approved boundary",
                );
            }
            return;
        }
        if let Some((subject, block)) = construct(node, macro_context) {
            if !self.allowed(node) {
                self.report(
                    node,
                    &format!("{subject} is outside an approved unsafe boundary"),
                );
            } else if block && !rationale::present(node, self.source, self.comments) {
                self.report(
                    node,
                    "approved unsafe block has no attached nonempty SAFETY: rationale",
                );
            }
        }
    }
    fn report(&mut self, node: Node<'_>, message: &str) {
        self.findings.push(Finding {
            rule: UnsafeBoundary::ID,
            path: self.source.path.clone(),
            configuration: self.assertion.setting.clone(),
            span: Some(Span::new(&self.source.text, node.byte_range())),
            related: Vec::new(),
            message: message.into(),
            instruction: "Keep unsafe operations inside an explicitly configured boundar\
                y and explain each unsafe block's validity with an attached SAFETY: comm\
                ent."
                .into(),
        });
    }
}
fn attribute_owner(mut node: Node<'_>) -> Node<'_> {
    if node.kind() != "attribute_item" {
        return node;
    }
    while let Some(next) = node.next_named_sibling() {
        node = next;
        if !matches!(
            node.kind(),
            "attribute_item" | "line_comment" | "block_comment"
        ) {
            break;
        }
    }
    node
}
fn unsafe_modifier(node: Node<'_>) -> bool {
    if node.kind() == "unsafe" {
        return true;
    }
    let mut cursor = node.walk();
    node.kind() == "function_modifiers"
        && node
            .children(&mut cursor)
            .any(|modifier| modifier.kind() == "unsafe")
}
fn construct(node: Node<'_>, macro_context: bool) -> Option<(&'static str, bool)> {
    if node.kind() == "unsafe_block" {
        return Some(("unsafe block", true));
    }
    if macro_context && node.kind() == "unsafe" {
        let block = node
            .next_named_sibling()
            .is_some_and(|next| matches!(next.kind(), "token_tree" | "block"));
        return Some((
            if block {
                "unsafe block in macro"
            } else {
                "unsafe item in macro"
            },
            block,
        ));
    }
    if matches!(
        node.kind(),
        "function_item"
            | "function_signature_item"
            | "impl_item"
            | "trait_item"
            | "foreign_mod_item"
    ) {
        let mut cursor = node.walk();
        let unsafe_item = node.children(&mut cursor).any(unsafe_modifier);
        if unsafe_item {
            return Some((
                match node.kind() {
                    "impl_item" => "unsafe impl",
                    "trait_item" => "unsafe trait",
                    "foreign_mod_item" => "unsafe foreign block",
                    _ => "unsafe function",
                },
                false,
            ));
        }
    }
    None
}
fn weak_attribute(node: Node<'_>, text: &str) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|node| node.kind() == "attribute")
        .and_then(|attribute| syn::parse_str::<Meta>(&text[attribute.byte_range()]).ok())
        .is_some_and(|meta| weak(&meta))
}
fn weak(meta: &Meta) -> bool {
    let Meta::List(list) = meta else { return false };
    let Ok(arguments) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
    else {
        return false;
    };
    if ["allow", "warn", "expect"]
        .iter()
        .any(|name| list.path.is_ident(name))
    {
        return arguments
            .iter()
            .any(|argument| argument.path().is_ident("unsafe_code"));
    }
    list.path.is_ident("cfg_attr") && arguments.iter().skip(1).any(weak)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::UnsafeBoundary>()?
            .check(root)
    }
    fn report(source: &str, settings: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/unsafe-boundary\"]]\ntarget = '**/*.rs'\n{settings}"),
        )
        .unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn detects_each_unsafe_construct_outside_boundary() {
        let source = "unsafe fn free() {} struct Value; unsafe trait Contract { unsafe f\
            n invoke(); } unsafe impl Contract for Value { unsafe fn invoke() {} } fn ca\
            ll() { unsafe {} }";
        assert_eq!(report(source, "").findings.len(), 6);
        assert!(
            report("unsafe extern \"C\" { fn call(); }", "")
                .findings
                .iter()
                .any(|finding| finding.message.contains("foreign"))
        );
    }
    #[test]
    fn approved_files_and_exact_modules_preserve_item_contracts() {
        let source = "unsafe fn entry() {} fn call() {\n// SAFETY: Pointer contract was \
            validated.\nunsafe {}\n}";
        assert!(
            report(source, "allowed_targets = ['lib.rs']")
                .findings
                .is_empty()
        );
        let source = format!(
            "mod ffi {{ {source} mod nested {{ unsafe fn export() {{}} }} }} mod not_ffi\
                \u{20}{{ unsafe fn export() {{}} }}"
        );
        let findings = report(&source, "allowed_modules = ['ffi']").findings;
        assert_eq!(findings.len(), 1);
        assert!(
            report(
                "mod other { mod ffi { unsafe fn export() {} } }",
                "allowed_modules = ['ffi']"
            )
            .findings
            .len()
                == 1
        );
        assert!(
            report(
                "mod other { mod ffi { unsafe fn export() {} } }",
                "allowed_modules = ['other::ffi']"
            )
            .findings
            .is_empty()
        );
    }
    #[test]
    fn rationale_is_real_attached_nonempty_and_can_start_the_block() {
        for body in [
            "// SAFETY: Allocation remains live.\n// Long explanation.\n// More detail.\
                \n// Fourth line.\nunsafe {}",
            "/* SAFETY: Allocation remains live. */ unsafe {}",
            "unsafe {\n// SAFETY: Allocation remains live.\n}",
            "// SAFETY:\n// Allocation remains live.\nlet value = unsafe { 1 };",
        ] {
            assert!(
                report(
                    &format!("fn run() {{\n{body}\n}}"),
                    "allowed_targets = 'lib.rs'"
                )
                .findings
                .is_empty(),
                "{body}"
            );
        }
        for body in [
            "unsafe {}",
            "// SAFETY:\nunsafe {}",
            "/* SAFETY: */ unsafe {}",
            "// SAFETY: Allocation live.\n\nunsafe {}",
            "// SAFETY: Allocation live.\nlet x = 1;\nunsafe {}",
            "let text = \"// SAFETY: forged\";\nunsafe {}",
        ] {
            assert_eq!(
                report(
                    &format!("fn run() {{\n{body}\n}}"),
                    "allowed_targets = 'lib.rs'"
                )
                .findings
                .len(),
                1,
                "{body}"
            );
        }
    }
    #[test]
    fn weakening_attributes_are_not_boundary_authorization() {
        for attribute in [
            "#![allow(unsafe_code)]",
            "#![warn(unsafe_code)]",
            "#[expect(unsafe_code)]",
            "#[allow(dead_code, unsafe_code)]",
            "#[cfg_attr(feature = \"native\", allow(unsafe_code))]",
        ] {
            let source = format!("{attribute}\nunsafe fn call() {{}}");
            assert_eq!(report(&source, "").findings.len(), 2, "{attribute}");
            assert!(
                report(&source, "allowed_targets = 'lib.rs'")
                    .findings
                    .is_empty()
            );
        }
        for attribute in [
            "#![deny(unsafe_code)]",
            "#![forbid(unsafe_code)]",
            "#![allow(unsafe_op_in_unsafe_fn)]",
        ] {
            assert!(
                report(&format!("{attribute}\nfn safe() {{}}"), "")
                    .findings
                    .is_empty()
            );
        }
    }
    #[test]
    fn macro_arguments_and_definitions_cannot_hide_unsafe() {
        for source in [
            "fn run() { assert_eq!(unsafe { 1 }, 1); }",
            "macro_rules! trampoline { ($name:ident) => { fn $name() { unsafe {} } }; }",
            "macro_rules! export { ($name:ident) => { unsafe extern \"C\" fn $name() {} }; }",
        ] {
            assert_eq!(report(source, "").findings.len(), 1, "{source}");
        }
        let source = "macro_rules! trampoline { ($name:ident) => { fn $name() {\n// SAFE\
            TY: Export table validated.\nunsafe {}\n} }; }";
        assert!(
            report(source, "allowed_targets = 'lib.rs'")
                .findings
                .is_empty()
        );
        assert_eq!(
            report(
                "fn run() { assert_eq!(unsafe { 1 }, 1); }",
                "allowed_targets = 'lib.rs'"
            )
            .findings
            .len(),
            1
        );
    }
    #[test]
    fn scopes_and_directives_remain_explicit() {
        let source = "#[cfg(test)] unsafe fn test() {} unsafe fn production() {}";
        assert_eq!(report(source, "").findings.len(), 1);
        assert_eq!(report(source, "scope = 'tests'").findings.len(), 1);
        assert_eq!(report(source, "scope = 'all'").findings.len(), 2);
        assert!(report(source, "exclude = 'lib.rs'").findings.is_empty());
        let source = "// linter:disable rust/unsafe-boundary -- Compiler integration req\
            uires this exact boundary here.\nunsafe fn entry() {}";
        let report = report(source, "");
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
    }
    #[test]
    fn invalid_selectors_modules_and_unknown_settings_fail() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '*'\nallowed_targets = []",
            "target = '*'\nallowed_modules = ['']",
            "target = '*'\nallowed_modules = ['ffi/*']",
            "target = '*'\nscope = 'invalid'",
            "target = '*'\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/unsafe-boundary\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
    #[test]
    fn outer_module_attributes_use_the_boundary_they_apply_to() {
        assert!(
            report(
                "#[allow(unsafe_code)] mod ffi { unsafe fn entry() {} }",
                "allowed_modules = ['ffi']"
            )
            .findings
            .is_empty()
        );
        assert_eq!(
            report(
                "#[allow(unsafe_code)] mod not_ffi { unsafe fn entry() {} }",
                "allowed_modules = ['ffi']"
            )
            .findings
            .len(),
            2
        );
    }
}
