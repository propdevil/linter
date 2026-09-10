use crate::{Analysis, Source};
use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use tree_sitter::Node;

mod config;
use config::Assertion;
pub use config::Config;

pub struct FunctionLength {
    assertions: Vec<Assertion>,
}

impl Rule for FunctionLength {
    const ID: &'static str = "c/function-length";
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
            if assertions.is_empty() {
                continue;
            }
            let clean = crate::lines::without_comments(source)?;
            visit(
                source.syntax.root_node(),
                source,
                &clean,
                &assertions,
                &mut findings,
            );
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn visit(
    node: Node<'_>,
    source: &Source,
    clean: &str,
    assertions: &[&Assertion],
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "function_definition" {
        let lines = crate::lines::effective(clean, node.byte_range());
        for assertion in assertions
            .iter()
            .filter(|assertion| lines > assertion.max_lines)
        {
            findings.push(Finding { span: Some(linter::Span::new(&source.text, node.byte_range())), related: Vec::new(),
                rule: FunctionLength::ID,
                path: source.path.clone(),
                configuration: assertion.setting.clone(),
                message: format!("C function at line {} has {lines} effective code lines; maximum is {}", node.start_position().row + 1, assertion.max_lines),
                instruction: "Extract cohesive behavior behind a named C function or module boundary; comments and blank lines already do not count.".into(),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, source, clean, assertions, findings);
    }
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
            .register::<super::FunctionLength>()?
            .check(root)
    }

    fn config(root: &Path, fields: &str) {
        write(
            root,
            "linter.toml",
            &format!("[[rules.\"c/function-length\"]]\n{fields}"),
        );
    }

    #[test]
    fn counts_signature_braces_and_code_but_not_blank_or_comment_lines() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.c'\nmax_lines = 5");
        write(
            root.path(),
            "sample.c",
            "int\nexample(void)\n{\n\n/* multiline\n ü comment */\nreturn 0; // trailing comment\n}\n",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        config(root.path(), "target = '**/*.c'\nmax_lines = 4");
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].message,
            "C function at line 1 has 5 effective code lines; maximum is 4"
        );
    }

    #[test]
    fn transfers_default_function_budget_without_other_structure_budgets() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.{c,h}'");
        write(
            root.path(),
            "large.h",
            &format!(
                "int oversized(void) {{\n{}return 0;\n}}",
                "int value;\n".repeat(197)
            ),
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "large.h",
            &format!(
                "int oversized(void) {{\n{}return 0;\n}}",
                "int value;\n".repeat(198)
            ),
        );
        assert_eq!(check(root.path()).unwrap().findings.len(), 1);
    }

    #[test]
    fn comments_strings_and_prototypes_do_not_forge_functions() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.c'\nmax_lines = 1");
        write(
            root.path(),
            "small.c",
            "/* int fake(void) { {{{{{{{ */\nconst char *text = \"int fake(void) { {{{{{{{\";\nint prototype(void);\nint small(void) { return 0; }\n",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "small.c",
            "int small(void) {\nconst char *text = \"/* real string */\";\nreturn 0;\n}\n",
        );
        assert!(
            check(root.path()).unwrap().findings[0]
                .message
                .contains("4 effective")
        );
    }

    #[test]
    fn honors_targets_and_exclusions_and_reports_each_function() {
        let root = tempfile::tempdir().unwrap();
        config(
            root.path(),
            "target = ['src/*.c']\nexclude = 'src/skip.c'\nmax_lines = 1",
        );
        let text = "int first(void) {\nreturn 0;\n}\nint second(void) {\nreturn 1;\n}";
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
    fn rejects_bad_settings_and_malformed_c() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '../*'",
            "target = '*'\nmax_lines = 0",
            "target = '*'\nmax_lines = -1",
            "target = '*'\nextra = true",
            "target = '*'\nexclude = []",
        ] {
            config(root.path(), fields);
            assert!(
                matches!(check(root.path()), Err(linter::Error::Configuration(_))),
                "{fields}"
            );
        }
        config(root.path(), "target = '**/*.c'");
        write(root.path(), "broken.c", "int broken(void) { return ;");
        assert!(matches!(
            check(root.path()),
            Err(linter::Error::Analysis(_))
        ));
    }
}
