use crate::{Error, Finding, Project, Rule, RuleResult, Status};
mod config;
use config::Assertion;
pub use config::Config;

pub struct LineWidth {
    assertions: Vec<Assertion>,
}

impl Rule for LineWidth {
    const ID: &'static str = "line-width";
    type Analysis = ();
    type Config = Config;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }

    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }

    fn check(&self, project: &Project, _: &()) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for entry in project.entries().filter(|entry| entry.kind.is_file()) {
            let assertions: Vec<_> = self
                .assertions
                .iter()
                .filter(|assertion| {
                    assertion.target.matches(&entry.path)
                        && !assertion
                            .exclude
                            .as_ref()
                            .is_some_and(|exclude| exclude.matches(&entry.path))
                })
                .collect();
            if assertions.is_empty() {
                continue;
            }
            let path = project.root().join(&entry.path);
            let text = std::fs::read_to_string(&path).map_err(|error| Error::io(&path, error))?;
            for assertion in assertions {
                for (index, line) in text.lines().enumerate() {
                    let count = width(line, assertion.tab_width)?;
                    if count > assertion.max_columns {
                        findings.push(Finding {
                            span: None,
                            related: Vec::new(),
                            rule: Self::ID,
                            path: entry.path.clone(),
                            configuration: assertion.setting.clone(),
                            message: format!(
                                "line {} has {count} columns; maximum is {}",
                                index + 1,
                                assertion.max_columns
                            ),
                            instruction:
                                "Wrap the line or shorten its contents to fit the configured width."
                                    .into(),
                        });
                    }
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn width(line: &str, tab_width: usize) -> Result<usize, Error> {
    line.chars().try_fold(0usize, |column, character| {
        let advance = if character == '\t' {
            tab_width - column % tab_width
        } else {
            1
        };
        column
            .checked_add(advance)
            .ok_or_else(|| Error::Analysis("line width exceeds the supported column range".into()))
    })
}

#[cfg(test)]
mod tests {
    use crate::{Error, Registry, Report};
    use std::{fs, path::Path};

    fn check(root: &Path) -> Result<Report, Error> {
        Registry::default()
            .register::<super::LineWidth>()?
            .check(root)
    }

    fn setup(config: &str, text: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("linter.toml"), config).unwrap();
        fs::write(root.path().join("input.rs"), text).unwrap();
        root
    }

    #[test]
    fn counts_physical_lines_unicode_tabs_and_trailing_whitespace() {
        let root = setup(
            "[[rules.\"line-width\"]]\ntarget = '*.rs'\nmax_columns = 4",
            "abcd\r\nabcde\n界界界界\n\t\na\t\na\tb\nab   \n\n",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 3);
        let messages: Vec<_> = report
            .findings
            .iter()
            .map(|finding| finding.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "line 2 has 5 columns; maximum is 4",
                "line 6 has 5 columns; maximum is 4",
                "line 7 has 5 columns; maximum is 4",
            ]
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.path == Path::new("input.rs"))
        );
    }

    #[test]
    fn checks_comments_strings_and_tests_with_default_limit() {
        let text = format!(
            "{}\n{}\n//{}\n\"{}\"\n#[test]\n{}",
            "x".repeat(100),
            "x".repeat(101),
            "x".repeat(99),
            "x".repeat(99),
            "x".repeat(101)
        );
        let root = setup("[[rules.\"line-width\"]]\ntarget = '*.rs'", &text);
        assert_eq!(check(root.path()).unwrap().findings.len(), 4);
    }

    #[test]
    fn target_and_exclusions_skip_unselected_files_including_invalid_utf8() {
        let root = setup(
            "[files]\nexclude = ['global.rs']\n[[rules.\"line-width\"]]\ntarget = ['*.rs\
                ']\nexclude = ['skip.rs']\nmax_columns = 1\ntab_width = 2",
            "\t",
        );
        for path in ["skip.rs", "global.rs", "other.txt"] {
            fs::write(root.path().join(path), [0xff]).unwrap();
        }
        assert_eq!(
            check(root.path()).unwrap().findings[0].message,
            "line 1 has 2 columns; maximum is 1"
        );
        fs::write(root.path().join("input.rs"), [0xff]).unwrap();
        assert!(matches!(check(root.path()), Err(Error::Io { .. })));
    }

    #[test]
    fn validates_configuration() {
        for fields in [
            "max_columns = 0",
            "tab_width = 0",
            "max_columns = -1",
            "tab_width = 'four'",
            "exclude = []",
            "exclude = '../*'",
            "unknown = true",
        ] {
            let root = setup(
                &format!("[[rules.\"line-width\"]]\ntarget = '*.rs'\n{fields}"),
                "",
            );
            assert!(
                matches!(check(root.path()), Err(Error::Configuration(_))),
                "{fields}"
            );
        }
        let root = setup("[[rules.\"line-width\"]]\nmax_columns = 10", "");
        assert!(matches!(check(root.path()), Err(Error::Configuration(_))));
    }

    #[test]
    fn handles_empty_files_and_reports_overflow_without_panicking() {
        let root = setup("[[rules.\"line-width\"]]\ntarget = '*.rs'", "");
        assert!(check(root.path()).unwrap().findings.is_empty());
        assert!(super::width("\t\t", usize::MAX).is_err());
        assert_eq!(super::width("e\u{301}", 4).unwrap(), 2);
    }
}
