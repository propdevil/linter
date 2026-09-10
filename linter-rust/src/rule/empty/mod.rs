use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use linter::{Error, Finding, Project, Rule, RuleResult, Span, Status};
use std::fs;
use tree_sitter::Node;
mod config;
pub use config::Config;
use config::{Assertion, Scope};

pub struct EmptyStruct(Vec<Assertion>);
impl Rule for EmptyStruct {
    const ID: &'static str = "rust/empty-struct";
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
    if node.kind() == "struct_item" && empty(node) {
        let test = tests[node.start_byte()];
        let selected = match assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        if selected && let Some(name) = node.child_by_field_name("name") {
            findings.push(Finding { rule: EmptyStruct::ID, path: source.path.clone(), configuration: assertion.setting.clone(),
                span: Some(Span::new(&source.text, node.byte_range())), related: Vec::new(),
                message: format!("struct '{}' carries no fields", &source.text[name.byte_range()]),
                instruction: "Use a module, function, trait, enum or meaningful state; justify intentional marker types with a narrow reasoned directive.".into(),
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}
fn empty(node: Node<'_>) -> bool {
    let Some(body) = node.child_by_field_name("body") else {
        return true;
    };
    let mut cursor = body.walk();
    !body.named_children(&mut cursor).any(|child| {
        !matches!(
            child.kind(),
            "line_comment" | "block_comment" | "attribute_item"
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn check(root: &Path) -> Result<linter::Report, linter::Error> {
        linter::Registry::default()
            .register::<super::EmptyStruct>()?
            .check(root)
    }
    fn fixture(source: &str, settings: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Cargo.toml",
            "[package]\nname='example'\nversion='0.0.0'\nedition='2024'",
        );
        write(root.path(), "src/lib.rs", source);
        write(
            root.path(),
            "linter.toml",
            &format!("[[rules.\"rust/empty-struct\"]]\ntarget = '**/*.rs'\n{settings}"),
        );
        root
    }
    #[test]
    fn rejects_all_three_empty_forms_but_preserves_named_and_tuple_fields() {
        let root = fixture(
            "struct Unit; struct Named {} struct Tuple(); struct Comment { /* state? */ } struct Value { field: u8 } struct Wrapper(String); struct Marker(std::marker::PhantomData<()>);",
            "",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 4);
        assert!(report.findings.iter().all(|finding| finding.span.is_some()));
        assert_eq!(report, check(root.path()).unwrap());
    }
    #[test]
    fn production_tests_and_all_scopes_share_test_classification() {
        let source = "struct Production; #[cfg(test)] struct Test; #[cfg(test)] mod tests { struct Nested; } #[cfg(any(test, feature = \"other\"))] struct MaybeProduction;";
        let root = fixture(source, "");
        write(root.path(), "tests/integration.rs", "struct Integration;");
        assert_eq!(check(root.path()).unwrap().findings.len(), 2);
        for (scope, expected) in [("tests", 3), ("all", 5)] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"rust/empty-struct\"]]\ntarget = '**/*.rs'\nscope = '{scope}'"),
            );
            assert_eq!(check(root.path()).unwrap().findings.len(), expected);
        }
    }
    #[test]
    fn reasoned_markers_are_explicit_and_stale_directives_fail() {
        let root = fixture(
            "// linter:disable rust/empty-struct -- Marker distinguishes authorized capability in the type system.\nstruct Marker;\nstruct Empty;",
            "",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.suppressed.len(), 1);
        write(
            root.path(),
            "src/lib.rs",
            "// linter:disable rust/empty-struct -- Marker distinguishes authorized capability in the type system.\nstruct Marker(u8);",
        );
        assert_eq!(check(root.path()).unwrap().findings[0].rule, "directive");
    }
    #[test]
    fn selections_and_strict_configuration_are_enforced() {
        let root = fixture("struct Empty;", "exclude = 'src/lib.rs'");
        assert!(check(root.path()).unwrap().findings.is_empty());
        for fields in [
            "target = []",
            "target = '../*'",
            "target = '*'\nscope = 'unknown'",
            "target = '*'\nexclude = []",
            "target = '*'\nallow_markers = true",
            "scope = 'all'",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"rust/empty-struct\"]]\n{fields}"),
            );
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
    #[test]
    fn directive_text_in_raw_strings_does_not_hide_empty_structs() {
        let root = fixture(
            r##"const NOTE: &str = r#"
// linter:disable rust/empty-struct -- forged in raw string
"#;
struct Empty;
"##,
            "",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule, "rust/empty-struct");
    }
}
