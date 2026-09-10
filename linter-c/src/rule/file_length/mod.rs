use crate::Analysis;
use linter::{Error, Finding, Project, Rule, RuleResult, Status};

mod config;
use config::Assertion;
pub use config::Config;

pub struct FileLength {
    assertions: Vec<Assertion>,
}

impl Rule for FileLength {
    const ID: &'static str = "c/file-length";
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
            let lines = crate::lines::effective(&clean, 0..clean.len());
            for assertion in assertions
                .into_iter()
                .filter(|assertion| lines > assertion.max_lines)
            {
                findings.push(Finding {
                    rule: Self::ID,
                    path: source.path.clone(),
                    configuration: assertion.setting.clone(),
                    message: format!("C file has {lines} effective code lines; maximum is {}", assertion.max_lines),
                    instruction: "Split cohesive state and behavior behind a named C module boundary; comments and blank lines already do not count.".into(),
                });
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
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
            .register::<super::FileLength>()?
            .check(root)
    }

    fn config(root: &Path, fields: &str) {
        write(
            root,
            "linter.toml",
            &format!("[[rules.\"c/file-length\"]]\n{fields}"),
        );
    }

    #[test]
    fn default_budget_passes_at_limit_and_reports_one_above() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.{c,h}'");
        write(root.path(), "large.h", &"int declaration;\n".repeat(1500));
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "large.h", &"int declaration;\n".repeat(1501));
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].message,
            "C file has 1501 effective code lines; maximum is 1500"
        );
        assert_eq!(report, check(root.path()).unwrap());
    }

    #[test]
    fn counts_braces_literals_and_directives_not_comments_or_blank_lines() {
        let root = tempfile::tempdir().unwrap();
        config(root.path(), "target = '**/*.c'\nmax_lines = 5");
        write(
            root.path(),
            "sample.c",
            "/* multiline\r\nü comment */\r\n\r\n#define NUMBER 1\r\nint example(void)\r\n{\r\nconst char *text = \"/* string */\"; // inline\r\n}\r\n// trailing comment\r\n",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        config(root.path(), "target = '**/*.c'\nmax_lines = 4");
        assert!(
            check(root.path()).unwrap().findings[0]
                .message
                .contains("5 effective")
        );
        write(
            root.path(),
            "sample.c",
            "/* comments only */\n\n// more comments",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }

    #[test]
    fn target_exclude_and_project_exclusion_limit_reported_files() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[files]
exclude = ["src/generated.c"]
[[rules."c/file-length"]]
target = ["src/*.c"]
exclude = "src/skipped.c"
max_lines = 1
"#,
        );
        for path in [
            "src/run.c",
            "src/generated.c",
            "src/skipped.c",
            "elsewhere/run.c",
        ] {
            write(root.path(), path, "int first;\nint second;");
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("src/run.c"));
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '../*'",
            "target = '*'\nexclude = []",
            "target = '*'\nmax_lines = 0",
            "target = '*'\nmax_lines = -1",
            "target = '*'\nmax_lines = 'large'",
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
