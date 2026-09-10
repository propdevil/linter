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

pub struct FunctionLength(Vec<Assertion>);

impl Rule for FunctionLength {
    const ID: &'static str = "rust/function-length";
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
    inspect_node(node, source, tests, assertion, findings);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}

fn inspect_node(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() != "function_item" || method(node) {
        return;
    }
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
            findings.push(Finding {
                span: Some(linter::Span::new(&source.text, node.byte_range())),
                related: Vec::new(),
                rule: FunctionLength::ID,
                path: source.path.clone(),
                configuration: format!("{}.max_lines", assertion.setting),
                message: format!(
                    "Rust function `{name}` at line {} has {lines} line\
                s; maximum is {}",
                    node.start_position().row + 1,
                    assertion.max_lines
                ),
                instruction: "Split the function into cohesive operations owned by t\
                he appropriate domain."
                    .into(),
            });
        }
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
            format!("[[rules.\"rust/function-length\"]]\ntarget='**/*.rs'\n{settings}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<FunctionLength>()
            .unwrap()
            .check(root.path())
            .unwrap()
            .findings
    }

    #[test]
    fn inclusive_budget_counts_signatures_comments_blanks_and_closures() {
        let text = "fn operation(\n) {\n// comment\n\nlet closure = || {\n};\n}\n";
        assert!(check(text, "max_lines=7").is_empty());
        let findings = check(text, "max_lines=6");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("has 7 lines"));
    }

    #[test]
    fn includes_nested_functions_but_excludes_methods_and_trait_bodies() {
        let text = "fn outer() {\nfn nested() {\n}\n}\nstruct Item;\nimpl Item { fn meth\
            od() {\n}\n}\ntrait Trait { fn method() {\n}\nfn absent(); }";
        let findings = check(text, "max_lines=1");
        assert_eq!(findings.len(), 2);
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`outer`"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.message.contains("`nested`"))
        );
    }

    #[test]
    fn scopes_and_nested_test_exclusions_are_explicit() {
        let text =
            "fn production() {\n#[cfg(test)] fn nested() {\n\n}\n}\n#[test] fn check() {\n}\n";
        assert!(check(text, "max_lines=2").is_empty());
        assert_eq!(check(text, "max_lines=2\nscope='tests'").len(), 1);
        assert_eq!(check(text, "max_lines=2\nscope='all'").len(), 2);
        assert!(check(text, "max_lines=1\nexclude='lib.rs'").is_empty());
    }

    #[test]
    fn default_budget_is_fifty_and_invalid_configuration_fails() {
        assert!(check(&format!("fn work() {{\n{}}}", "\n".repeat(48)), "").is_empty());
        assert_eq!(
            check(&format!("fn work() {{\n{}}}", "\n".repeat(49)), "").len(),
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
                format!("[[rules.\"rust/function-length\"]]\n{fields}"),
            )
            .unwrap();
            let registry = linter::Registry::default()
                .register::<FunctionLength>()
                .unwrap();
            assert!(
                matches!(registry.check(root.path()), Err(Error::Configuration(_))),
                "{fields}"
            );
        }
    }
    #[test]
    fn own_implementation_obeys_the_default_function_budget() {
        assert!(check(include_str!("mod.rs"), "").is_empty());
        assert!(check(include_str!("config.rs"), "").is_empty());
    }

    #[test]
    fn integration_sources_are_tests_even_without_attributes() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("tests")).unwrap();
        fs::write(
            root.path().join("tests/payment.rs"),
            "fn integration() {\n\n}\n",
        )
        .unwrap();
        let registry = linter::Registry::default()
            .register::<FunctionLength>()
            .unwrap();
        for (scope, expected) in [("production", 0), ("tests", 1), ("all", 1)] {
            fs::write(
                root.path().join("linter.toml"),
                format!(
                    "[[rules.\"rust/function-\
                length\"]]\ntarget='**/*.rs'\nmax_lines=2\nscope='{scope}'"
                ),
            )
            .unwrap();
            assert_eq!(
                registry.check(root.path()).unwrap().findings.len(),
                expected
            );
        }
    }
}
