use std::fs;

use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use tree_sitter::Node;

use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};

mod config;
pub use config::Config;

pub struct MethodLength(Vec<Assertion>);

impl Rule for MethodLength {
    const ID: &'static str = "rust/method-length";
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
    if node.kind() == "function_item" && method(node) {
        let test = tests[node.start_byte()];
        let selected = match assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        if selected {
            let lines = lines(node, &source.text, tests, assertion.scope);
            if lines > assertion.max_lines {
                let name = node
                    .child_by_field_name("name")
                    .map(|name| &source.text[name.byte_range()])
                    .unwrap_or("<anonymous>");
                findings.push(Finding { span: Some(linter::Span::new(&source.text, node.byte_range())), related: Vec::new(),
                    rule: MethodLength::ID,
                    path: source.path.clone(),
                    configuration: format!("{}.max_lines", assertion.setting),
                    message: format!("Rust method `{name}` at line {} has {lines} lines; maximum is {}", node.start_position().row + 1, assertion.max_lines),
                    instruction: "Split the function into cohesive operations owned by the appropriate domain.".into(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}

fn method(node: Node<'_>) -> bool {
    node.parent()
        .filter(|parent| parent.kind() == "declaration_list")
        .and_then(|parent| parent.parent())
        .is_some_and(|parent| matches!(parent.kind(), "impl_item" | "trait_item"))
}

fn lines(node: Node<'_>, text: &str, tests: &[bool], scope: Scope) -> usize {
    let mut offset = node.start_byte();
    text[node.byte_range()]
        .split_inclusive('\n')
        .filter(|line| {
            let mask = &tests[offset..offset + line.len()];
            offset += line.len();
            !matches!(scope, Scope::Production)
                || line
                    .bytes()
                    .zip(mask)
                    .any(|(byte, test)| !test && !byte.is_ascii_whitespace())
                || !mask.iter().any(|test| *test)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str, settings: &str) -> Vec<Finding> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), text).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/method-length\"]]\ntarget='**/*.rs'\n{settings}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<MethodLength>()
            .unwrap()
            .check(root.path())
            .unwrap()
            .findings
    }

    #[test]
    fn checks_associated_functions_methods_and_trait_default_bodies() {
        let text = "struct Item; impl Item { fn new() -> Self {\nSelf\n} fn value(&self) {\n}\n} trait Trait { fn default(&self) {\n} fn declaration(&self); } impl Trait for Item { fn default(&self) {\n} } fn free() {\n}";
        let findings = check(text, "max_lines=1");
        assert_eq!(findings.len(), 4);
        for name in ["new", "value", "default"] {
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.message.contains(&format!("`{name}`")))
            );
        }
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("`free`"))
        );
    }

    #[test]
    fn counts_complete_signatures_comments_blanks_and_closures() {
        let text = "struct Item<T>(T); impl<T> Item<T> {\nfn value(\n&self,\n) where T: Clone {\n// comment\n\nlet closure = || {\n};\n}\n}";
        assert!(check(text, "max_lines=8").is_empty());
        let findings = check(text, "max_lines=7");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("has 8 lines"));
    }

    #[test]
    fn nested_methods_count_but_nested_free_functions_do_not() {
        let text = "fn enclosing() { struct Inner; impl Inner { fn nested() {\n}\n}} struct Outer; impl Outer { fn method() {\nfn nested_free() {\n}\n}\n}";
        let findings = check(text, "max_lines=1");
        assert_eq!(findings.len(), 2);
        assert!(
            !findings
                .iter()
                .any(|finding| finding.message.contains("`nested_free`"))
        );
    }

    #[test]
    fn scopes_exclude_test_items_in_production_methods() {
        let text = "struct Item; impl Item { fn production() {\n#[cfg(test)] fn nested() {\n\n}\n}\n#[cfg(test)] fn test_method() {\n\n}\n}";
        assert!(check(text, "max_lines=2").is_empty());
        assert_eq!(check(text, "max_lines=2\nscope='tests'").len(), 1);
        assert_eq!(check(text, "max_lines=2\nscope='all'").len(), 2);
        assert!(check(text, "max_lines=1\nexclude='lib.rs'").is_empty());
    }

    #[test]
    fn fifty_lines_is_default_and_configuration_is_strict() {
        assert!(
            check(
                &format!(
                    "struct Item; impl Item {{ fn value() {{\n{}}} }}",
                    "\n".repeat(48)
                ),
                ""
            )
            .is_empty()
        );
        assert_eq!(
            check(
                &format!(
                    "struct Item; impl Item {{ fn value() {{\n{}}} }}",
                    "\n".repeat(49)
                ),
                ""
            )
            .len(),
            1
        );
        for fields in [
            "target=[]",
            "target='../*'",
            "target='*'\nmax_lines=0",
            "target='*'\nscope='maybe'",
            "glob='*'",
            "target='*'\nmaximum=5",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/method-length\"]]\n{fields}"),
            )
            .unwrap();
            let registry = linter::Registry::default()
                .register::<MethodLength>()
                .unwrap();
            assert!(
                matches!(registry.check(root.path()), Err(Error::Configuration(_))),
                "{fields}"
            );
        }
    }

    #[test]
    fn own_implementation_obeys_default_method_budget() {
        assert!(check(include_str!("mod.rs"), "").is_empty());
        assert!(check(include_str!("config.rs"), "").is_empty());
    }

    #[test]
    fn integration_methods_are_tests_without_attributes() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("tests")).unwrap();
        fs::write(
            root.path().join("tests/payment.rs"),
            "struct Item; impl Item { fn value() {\n\n} }\n",
        )
        .unwrap();
        let registry = linter::Registry::default()
            .register::<MethodLength>()
            .unwrap();
        for (scope, expected) in [("production", 0), ("tests", 1), ("all", 1)] {
            fs::write(root.path().join("linter.toml"), format!("[[rules.\"rust/method-length\"]]\ntarget='**/*.rs'\nmax_lines=2\nscope='{scope}'")).unwrap();
            assert_eq!(
                registry.check(root.path()).unwrap().findings.len(),
                expected
            );
        }
    }
}
