use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use std::fs;
use tree_sitter::Node;
mod config;
pub use config::Config;
use config::{Assertion, Scope};

pub struct SelfConstructor(Vec<Assertion>);
impl Rule for SelfConstructor {
    const ID: &'static str = "rust/self-constructor-static";
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
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &tests,
                    assertion,
                    &mut findings,
                );
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn inspect(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "function_item"
        && inherent(node)
        && let Some(name) = node.child_by_field_name("name")
        && constructor(&source.text[name.byte_range()])
        && let Some(parameters) = node.child_by_field_name("parameters")
        && let Some(receiver) = receiver_use(parameters)
        && node
            .child_by_field_name("return_type")
            .is_some_and(|output| returns_self(output, &source.text))
        && node
            .child_by_field_name("body")
            .is_some_and(|body| receiver_use(body).is_none())
    {
        let selected = match assertion.scope {
            Scope::Production => !tests[node.start_byte()],
            Scope::Tests => tests[node.start_byte()],
            Scope::All => true,
        };
        if selected {
            findings.push(Finding {
                rule: SelfConstructor::ID,
                path: source.path.clone(),
                configuration: assertion.setting.clone(),
                span: Some(Span::new(&source.text, node.byte_range())),
                related: vec![Evidence {
                    path: source.path.clone(),
                    span: Some(Span::new(&source.text, receiver.byte_range())),
                    message: "\
                Receiver is not referenced by the factory body."
                        .into(),
                }],
                message: format!(
                    "constructor '{}' returns Self but takes an unused receiver",
                    &source.text[name.byte_range()]
                ),
                instruction: "Make this receiver-independent constructor an associated f\
                unction and update its callers."
                    .into(),
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}
fn inherent(node: Node<'_>) -> bool {
    node.parent()
        .filter(|parent| parent.kind() == "declaration_list")
        .and_then(|parent| parent.parent())
        .is_some_and(|parent| {
            parent.kind() == "impl_item" && parent.child_by_field_name("trait").is_none()
        })
}
fn constructor(name: &str) -> bool {
    let name = name.trim_start_matches("r#");
    ["new", "parse", "from", "try_from"].iter().any(|prefix| {
        name == *prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('_'))
    })
}
fn receiver_use(node: Node<'_>) -> Option<Node<'_>> {
    if matches!(
        node.kind(),
        "line_comment" | "block_comment" | "string_literal" | "raw_string_literal" | "char_literal"
    ) {
        return None;
    }
    if node.kind() == "self" {
        return (!node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "scoped_identifier" | "scoped_type_identifier"
            )
        }))
        .then_some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).find_map(receiver_use)
}
fn returns_self(node: Node<'_>, text: &str) -> bool {
    if node.kind() == "type_identifier" && &text[node.byte_range()] == "Self" {
        return true;
    }
    if node.kind() != "generic_type" {
        return false;
    }
    let Some(arguments) = node.child_by_field_name("type_arguments") else {
        return false;
    };
    let mut cursor = arguments.walk();
    arguments.named_children(&mut cursor).any(|argument| {
        argument.kind() == "type_identifier" && &text[argument.byte_range()] == "Self"
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::SelfConstructor>()?
            .check(root)
    }
    fn run(source: &str, settings: &str) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), source).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/self-constructor-static\"]]\ntarget = '**/*.rs'\n{settings}"),
        )
        .unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn reports_inherent_receiver_independent_factories_and_self_wrappers() {
        let source = "struct Value(u8); impl Value { fn new(self) -> Self { Self(0) } fn\
            \u{20}parse(&self) -> Result<Self, ()> { Ok(Self(1)) } fn from_parts(&mut se\
            lf) -> Option<Self> { Some(Self(2)) } fn try_from_text(self: Box<Self>) -> B\
            ox<Self> { Box::new(Self(3)) } }";
        let report = run(source, "");
        assert_eq!(report.findings.len(), 4);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.related.len() == 1)
        );
    }
    #[test]
    fn preserves_receiver_dependent_conversions_builders_and_trait_contracts() {
        let source = "struct Value(u8); impl Value { fn parse(self) -> Result<Self, ()> \
            { Ok(self) } fn from_parts(self) -> Self { Self(self.0) } fn new_alias(self)\
            \u{20}-> Self { let previous = self; previous } fn new_mutating(mut self) ->\
            \u{20}Self { self.0 += 1; self } fn from_macro(self) -> Self { convert!(self\
            ) } fn new() -> Self { Self(0) } fn newer(&self) -> Self { Self(1) } } trait\
            \u{20}Build { fn new(&self) -> Self; } impl Build for Value { fn new(&self) \
            -> Self { Self(0) } }";
        assert!(run(source, "").findings.is_empty());
    }
    #[test]
    fn strings_comments_similar_names_and_shadowed_closure_values_do_not_count_as_self() {
        let source = "struct Value(u8); impl Value { fn parse(self) -> Self { let myself\
            \u{20}= \"self\"; /* self */ let text = r#\"self\"#; let f = |myself| myself\
            ; Self(0) } }";
        assert_eq!(run(source, "").findings.len(), 1);
        assert_eq!(
            run(
                "struct Value(u8); impl Value { fn new(self) -> Self { Self(self::constant()) } }",
                ""
            )
            .findings
            .len(),
            1
        );
    }
    #[test]
    fn excludes_non_self_returns_and_distinct_scope_types() {
        let source = "struct Value(u8); impl Value { fn new_ref(&self) -> &Self { unreac\
            hable!() } fn parse(&self) -> u8 { 0 } fn from_nested(&self) -> Result<Optio\
            n<Self>, ()> { Ok(None) } #[cfg(test)] fn new(self) -> Self { Self(0) } fn f\
            rom_outer(self) -> Self { fn parse() {} Self(1) } }";
        assert_eq!(run(source, "").findings.len(), 1);
        assert_eq!(run(source, "scope = 'tests'").findings.len(), 1);
        assert_eq!(run(source, "scope = 'all'").findings.len(), 2);
        assert!(run(source, "exclude = 'lib.rs'").findings.is_empty());
    }
    #[test]
    fn directives_remain_reasoned_and_unused_directives_fail() {
        let source = "struct Value(u8); impl Value {\n// linter:disable rust/self-constr\
            uctor-static -- Fixed compatibility method signature.\nfn parse(self) -> Sel\
            f { Self(0) }\nfn new(self) -> Self { Self(1) }\n}";
        let report = run(source, "");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.suppressed.len(), 1);
        let report = run(&source.replace("Self(0)", "self"), "");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.rule == "directive")
        );
    }
    #[test]
    fn invalid_configuration_fails() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '*'\nexclude = []",
            "target = '*'\nscope = 'unknown'",
            "target = '*'\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/self-constructor-static\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
}
