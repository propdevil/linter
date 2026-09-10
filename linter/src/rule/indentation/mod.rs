use crate::{Error, Finding, Project, Rule, RuleResult, Status};
mod config;
use config::Assertion;
pub use config::Config;

pub struct MaxIndent {
    assertions: Vec<Assertion>,
}

impl Rule for MaxIndent {
    const ID: &'static str = "max-indent";
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
                    if line.trim().is_empty() {
                        continue;
                    }
                    let count = indentation(line, assertion.tab_width)?;
                    if count > assertion.max_columns {
                        findings.push(Finding {
                            span: None,
                            related: Vec::new(),
                            rule: Self::ID,
                            path: entry.path.clone(),
                            configuration: assertion.setting.clone(),
                            message: format!(
                                "line {} has {count} indentation columns; maximum is {}",
                                index + 1,
                                assertion.max_columns
                            ),
                            instruction: "Reduce nesting or restructure the continuation to fit t\
                he configured indentation."
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

fn indentation(line: &str, tab_width: usize) -> Result<usize, Error> {
    line.chars()
        .take_while(|character| character.is_whitespace())
        .try_fold(0usize, |column, character| {
            let advance = if character == '\t' {
                tab_width - column % tab_width
            } else {
                1
            };
            column.checked_add(advance).ok_or_else(|| {
                Error::Analysis("indentation exceeds the supported column range".into())
            })
        })
}

#[cfg(test)]
mod tests {
    use crate::{Error, Registry, Report};
    use std::{fs, path::Path};

    fn check(root: &Path) -> Result<Report, Error> {
        Registry::default()
            .register::<super::MaxIndent>()?
            .check(root)
    }

    fn setup(config: &str, text: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("linter.toml"), config).unwrap();
        fs::write(root.path().join("input.rs"), text).unwrap();
        root
    }

    #[test]
    fn counts_leading_columns_with_tabs_and_crlf() {
        let root = setup(
            "[[rules.\"max-indent\"]]\ntarget = '*.rs'\nmax_columns = 4",
            "    x\r\n     x\n\tx\n \tx\n\t x\nx\t\t\t\n",
        );
        let report = check(root.path()).unwrap();
        let messages: Vec<_> = report
            .findings
            .iter()
            .map(|finding| finding.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "line 2 has 5 indentation columns; maximum is 4",
                "line 5 has 5 indentation columns; maximum is 4",
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
    fn default_limit_applies_to_continuations_comments_and_tests() {
        let text = format!(
            "{}x\n{}argument,\n{}// comment\n{}#[test]",
            " ".repeat(16),
            " ".repeat(17),
            " ".repeat(17),
            " ".repeat(17)
        );
        let root = setup("[[rules.\"max-indent\"]]\ntarget = '*.rs'", &text);
        assert_eq!(check(root.path()).unwrap().findings.len(), 3);
    }

    #[test]
    fn ignores_whitespace_only_lines_and_counts_unicode_whitespace() {
        let root = setup(
            "[[rules.\"max-indent\"]]\ntarget = '*.rs'\nmax_columns = 1",
            "\n                  \n\t\t\t\r\n\u{2003}\u{2003}\n\u{2003}x\n\u{2003}\u{2003}x",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].message,
            "line 6 has 2 indentation columns; maximum is 1"
        );
    }

    #[test]
    fn selections_exclude_unreadable_text_and_custom_tab_stops_apply() {
        let root = setup(
            "[files]\nexclude = ['global.rs']\n[[rules.\"max-indent\"]]\ntarget = ['*.rs\
                ']\nexclude = 'skip.rs'\nmax_columns = 1\ntab_width = 2",
            "\tx",
        );
        for path in ["skip.rs", "global.rs", "other.txt"] {
            fs::write(root.path().join(path), [0xff]).unwrap();
        }
        assert_eq!(
            check(root.path()).unwrap().findings[0].message,
            "line 1 has 2 indentation columns; maximum is 1"
        );
        fs::write(root.path().join("input.rs"), [0xff]).unwrap();
        assert!(matches!(check(root.path()), Err(Error::Io { .. })));
    }

    #[test]
    fn rejects_bad_configuration_and_reports_overflow() {
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
                &format!("[[rules.\"max-indent\"]]\ntarget = '*.rs'\n{fields}"),
                "",
            );
            assert!(
                matches!(check(root.path()), Err(Error::Configuration(_))),
                "{fields}"
            );
        }
        let root = setup("[[rules.\"max-indent\"]]\nmax_columns = 10", "");
        assert!(matches!(check(root.path()), Err(Error::Configuration(_))));
        assert!(super::indentation("\t\tx", usize::MAX).is_err());
    }
}
