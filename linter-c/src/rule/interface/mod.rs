use crate::{Analysis, Source};
use linter::{Error, Evidence, Finding, Project, Rule, RuleResult, Span, Status};
use tree_sitter::Node;
mod config;
use config::Assertion;
pub use config::Config;

pub struct Interface {
    assertions: Vec<Assertion>,
}
impl Rule for Interface {
    const ID: &'static str = "c/interface-breadth";
    type Analysis = Analysis;
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }
    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }
    fn check(&self, _: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for source in analysis.sources.iter().filter(|source| {
            source
                .path
                .extension()
                .is_some_and(|extension| extension == "h")
        }) {
            let mut declarations = Vec::new();
            collect(source.syntax.root_node(), source, &mut declarations);
            for assertion in self.assertions.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
                    && declarations.len() > assertion.max_functions
            }) {
                findings.push(finding(source, assertion, &declarations));
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn collect<'tree>(
    node: Node<'tree>,
    source: &Source,
    declarations: &mut Vec<(Node<'tree>, Node<'tree>)>,
) {
    if node.kind() == "declaration" {
        let mut cursor = node.walk();
        if node.named_children(&mut cursor).any(|child| {
            child.kind() == "storage_class_specifier"
                && &source.text[child.byte_range()] == "static"
        }) {
            return;
        }
        for declarator in node.children_by_field_name("declarator", &mut cursor) {
            if let Some(name) = function(declarator) {
                declarations.push((node, name));
            }
        }
        return;
    }
    if node.kind() == "translation_unit"
        || matches!(
            node.kind(),
            "preproc_if" | "preproc_ifdef" | "preproc_else" | "preproc_elif" | "preproc_elifdef"
        )
    {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(child, source, declarations);
        }
    }
}

fn function(mut node: Node<'_>) -> Option<Node<'_>> {
    let mut binding = None;
    loop {
        if node.kind() == "identifier" {
            return (binding == Some("function_declarator")).then_some(node);
        }
        if matches!(
            node.kind(),
            "pointer_declarator" | "array_declarator" | "function_declarator"
        ) {
            binding = Some(node.kind());
        }
        node = node.child_by_field_name("declarator").or_else(|| {
            (node.kind() == "parenthesized_declarator")
                .then(|| node.named_child(0))
                .flatten()
        })?;
    }
}

fn finding(
    source: &Source,
    assertion: &Assertion,
    declarations: &[(Node<'_>, Node<'_>)],
) -> Finding {
    Finding {
        rule: Interface::ID,
        path: source.path.clone(),
        configuration: assertion.setting.clone(),
        span: declarations
            .first()
            .map(|(node, _)| Span::new(&source.text, node.byte_range())),
        related: declarations
            .iter()
            .map(|(_, name)| Evidence {
                path: source.path.clone(),
                span: Some(Span::new(&source.text, name.byte_range())),
                message: format!(
                    "External function declaration '{}'.",
                    &source.text[name.byte_range()]
                ),
            })
            .collect(),
        message: format!(
            "C header exposes {} function declarations; maximum is {}",
            declarations.len(),
            assertion.max_functions
        ),
        instruction: "Split unrelated operations into cohesive headers with narrow ownership."
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::Interface>()?
            .check(root)
    }
    fn run(source: &str, limit: usize) -> linter::Report {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!(
                "[[rules.\"c/interface-breadth\"]]\ntarget = '**/*.h'\nmax_functions = {limit}"
            ),
        )
        .unwrap();
        fs::write(root.path().join("api.h"), source).unwrap();
        check(root.path()).unwrap()
    }
    #[test]
    fn exact_limit_passes_and_excess_has_declaration_evidence() {
        let source = "int open_store(void);\nint read_store(void);\nint close_store(void);\n";
        assert!(run(source, 3).findings.is_empty());
        let report = run(source, 2);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].related.len(), 3);
        assert!(
            report.findings[0]
                .message
                .contains("3 function declarations")
        );
    }
    #[test]
    fn skips_static_pointers_typedefs_variables_and_local_declarations() {
        let source = "static int helper(void);\ninline static int reordered(void);\nint \
            (*callback)(void);\ntypedef int Function(void);\ntypedef int (*Callback)(voi\
            d);\nint numbers[4];\nstruct Table { int (*callback)(void); };\nstatic int i\
            mplementation(void) { int local(void); return 0; }\nint *public_api(void);\n";
        assert!(run(source, 1).findings.is_empty());
        let source = format!("{source}int (*factory(void))(int);\n");
        let report = run(&source, 1);
        assert_eq!(report.findings[0].related.len(), 2);
        assert!(report.findings[0].related[1].message.contains("factory"));
    }
    #[test]
    fn counts_each_declarator_and_preprocessor_branch_not_macro_text() {
        let source = "#define DECLARE() int fake(void);\n/* int fake(void); */\nconst ch\
            ar *text = \"int fake(void);\";\n#ifndef API_H\n#define API_H\nint a(void), \
            b(void);\n#if FLAG\nint c(void);\n#else\nint d(void);\n#endif\n#endif\n";
        let report = run(source, 3);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].related.len(), 4);
    }
    #[test]
    fn directives_attach_to_first_declaration_and_stale_directives_fail() {
        let source = "// linter:disable c/interface-breadth -- generated protocol surfac\
            e\nint a(void);\nint b(void);\n";
        let report = run(source, 1);
        assert!(report.findings.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        let report = run(source, 2);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "directive");
    }
    #[test]
    fn validates_configuration_default_budget_and_selectors() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '*'\nmax_functions = 0",
            "target = '*'\nmax_functions = -1",
            "target = '*'\nexclude = []",
            "target = '*'\nunknown = true",
        ] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"c/interface-breadth\"]]\n{fields}"),
            )
            .unwrap();
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        fs::write(
            root.path().join("linter.toml"),
            "[[rules.\"c/interface-breadth\"]]\ntarget = ['*']\nexclude = 'skip.h'",
        )
        .unwrap();
        let source = (0..25)
            .map(|index| format!("int function_{index}(void);\n"))
            .collect::<String>();
        for path in ["run.c", "skip.h"] {
            fs::write(root.path().join(path), &source).unwrap();
        }
        assert!(check(root.path()).unwrap().findings.is_empty());
        fs::write(root.path().join("api.h"), &source).unwrap();
        assert!(
            check(root.path()).unwrap().findings[0]
                .message
                .contains("maximum is 24")
        );
    }
}
