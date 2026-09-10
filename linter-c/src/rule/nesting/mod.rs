use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use tree_sitter::Node;

mod config;
use config::Assertion;
pub use config::Config;

pub struct Nesting {
    assertions: Vec<Assertion>,
}

impl Rule for Nesting {
    const ID: &'static str = "c/nesting";
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
        for source in &analysis.sources {
            let assertions: Vec<_> = self
                .assertions
                .iter()
                .filter(|assertion| {
                    assertion.target.matches(&source.path)
                        && !assertion
                            .exclude
                            .as_ref()
                            .is_some_and(|exclude| exclude.matches(&source.path))
                })
                .collect();
            if !assertions.is_empty() {
                visit(
                    source.syntax.root_node(),
                    source,
                    &assertions,
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

fn visit(node: Node<'_>, source: &Source, assertions: &[&Assertion], findings: &mut Vec<Finding>) {
    if node.kind() == "function_definition" {
        let depth = maximum(node, 0);
        for assertion in assertions
            .iter()
            .filter(|assertion| depth > assertion.max_depth)
        {
            findings.push(Finding { span: Some(linter::Span::new(&source.text, node.byte_range())), related: Vec::new(),
                rule: Nesting::ID,
                path: source.path.clone(),
                configuration: assertion.setting.clone(),
                message: format!("C function at line {} has control-flow depth {depth}; maximum is {}", node.start_position().row + 1, assertion.max_depth),
                instruction: "Reduce nested decisions with early exits or extract cohesive behavior into a named function.".into(),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, source, assertions, findings);
    }
}

fn maximum(node: Node<'_>, depth: usize) -> usize {
    let else_if = node.kind() == "if_statement"
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == "else_clause");
    let depth = depth
        + usize::from(
            !else_if
                && matches!(
                    node.kind(),
                    "if_statement"
                        | "switch_statement"
                        | "for_statement"
                        | "while_statement"
                        | "do_statement"
                ),
        );
    let mut result = depth;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        result = result.max(maximum(child, depth));
    }
    result
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
            .register::<super::Nesting>()?
            .check(root)
    }

    fn config(root: &Path, fields: &str) {
        write(
            root,
            "linter.toml",
            &format!("[[rules.\"c/nesting\"]]\n{fields}"),
        );
    }

    #[test]
    fn default_depth_passes_six_and_reports_seven_once_per_function() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.{c,h}'");
        for depth in [6, 7] {
            write(
                root.path(),
                "depth.c",
                &format!(
                    "int deep(int value) {{\n{}return value;{}\nreturn 0;\n}}",
                    "if (value) {".repeat(depth),
                    "}".repeat(depth)
                ),
            );
            let report = check(root.path()).unwrap();
            assert_eq!(report.findings.len(), usize::from(depth == 7));
            if depth == 7 {
                assert_eq!(
                    report.findings[0].message,
                    "C function at line 1 has control-flow depth 7; maximum is 6"
                );
            }
        }
    }

    #[test]
    fn counts_control_flow_not_braces_preprocessor_comments_or_strings() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.c'\nmax_depth = 5");
        write(
            root.path(),
            "portable.c",
            r#"
#define WRAP(value) do { value; } while (0)
/* if (fake) { while (fake) { */
const char *text = "if (fake) { while (fake) {";
int sample(int value) {
    struct Local { int nested[8]; } local = {0};
    {{{ value += 1; }}}
#if ENABLED
    if (value) { while (value) { for (;;) { switch (value) { default: do { value--; } while (value); } } } }
#endif
    return local.nested[0];
}
"#,
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        config(root.path(), "target = '**/*.c'\nmax_depth = 4");
        assert!(
            check(root.path()).unwrap().findings[0]
                .message
                .contains("depth 5")
        );
    }

    #[test]
    fn else_if_chain_stays_at_one_level_but_nested_else_branch_counts() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.c'\nmax_depth = 1");
        write(
            root.path(),
            "classification.c",
            "int classify(int value) {\nif (value == 1) return 1;\nelse if (value == 2) return 2;\nelse if (value == 3) return 3;\nelse return 0;\n}",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "classification.c",
            "int classify(int value) {\nif (value == 1) return 1;\nelse { if (value == 2) return 2; }\nreturn 0;\n}",
        );
        assert!(
            check(root.path()).unwrap().findings[0]
                .message
                .contains("depth 2")
        );
    }

    #[test]
    fn selectors_exclude_files_and_functions_get_independent_budgets() {
        let root = tempfile::tempdir().unwrap();
        config(
            root.path(),
            "target = ['src/*.c']\nexclude = 'src/skip.c'\nmax_depth = 1",
        );
        let text = "int a(int x) { if(x) { if(x) { return 0; } } return 1; }\nint b(int x) { while(x) { while(x) { x--; } } return x; }";
        for path in ["src/run.c", "src/skip.c", "other/run.c"] {
            write(root.path(), path, text);
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.path == Path::new("src/run.c"))
        );
        assert_eq!(report, check(root.path()).unwrap());
    }

    #[test]
    fn rejects_invalid_settings() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '../*'",
            "target = '*'\nexclude = []",
            "target = '*'\nmax_depth = 0",
            "target = '*'\nmax_depth = -1",
            "target = '*'\nmax_depth = 'deep'",
            "target = '*'\nunknown = true",
        ] {
            config(root.path(), fields);
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
}
