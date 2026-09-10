use crate::{Error, Finding, Project, Rule, RuleResult, Status};
use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
mod config;
pub use config::Config;
use config::{Assertion, Case, Kind};

pub struct Filename {
    assertions: Vec<Assertion>,
}
impl Rule for Filename {
    const ID: &'static str = "filename";
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
        for assertion in &self.assertions {
            assertion.inspect(project, &mut findings);
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

impl Assertion {
    fn inspect(&self, project: &Project, findings: &mut Vec<Finding>) {
        for entry in project.entries().filter(|entry| {
            (match self.kind {
                Kind::Any => entry.kind.is_dir() || entry.kind.is_file(),
                Kind::File => entry.kind.is_file(),
                Kind::Directory => entry.kind.is_dir(),
            }) && self.selector.matches(&entry.path)
        }) {
            if entry.path == std::path::Path::new(".") {
                continue;
            }
            let directory = entry.kind.is_dir();
            let limit = self.max_words.unwrap_or(if directory { 1 } else { 2 });
            let label = if directory { "directory" } else { "file" };
            let stem = if directory {
                entry.path.file_name()
            } else {
                entry.path.file_stem()
            };
            let mut messages = Vec::new();
            if let Some(name) = stem.and_then(|value| value.to_str()) {
                let normalized = name.to_snake_case();
                let words: Vec<_> = normalized
                    .split('_')
                    .filter(|word| !word.is_empty())
                    .collect();
                if !name.chars().any(char::is_alphabetic) {
                    messages.push(format!("{label} name must contain a word"));
                }
                if words.len() > limit {
                    messages.push(format!(
                        "{label} name has {} words; maximum is {}",
                        words.len(),
                        limit
                    ));
                }
                if let Some(maximum) = self.max_characters {
                    let count = name.chars().count();
                    if count > maximum {
                        messages.push(format!(
                            "{label} name has {count} characters; maximum is {maximum}"
                        ));
                    }
                }
                if self.reject_numbered_fragments && numbered_fragment(name) {
                    messages.push(format!(
                        "{label} name uses a numbered implementation fragment"
                    ));
                }
                if let Some(case) = self.case {
                    let expected = match case {
                        Case::Snake => name.to_snake_case(),
                        Case::Camel => name.to_lower_camel_case(),
                        Case::Pascal => name.to_upper_camel_case(),
                        Case::Kebab => name.to_kebab_case(),
                    };
                    if name != expected {
                        messages.push(format!(
                            "{label} name does not use configured case; expected {expected:?}"
                        ));
                    }
                }
            } else {
                messages.push(format!("{label} name is not valid Unicode"));
            }
            for message in messages {
                findings.push(Finding { rule: Filename::ID, path: entry.path.clone(), configuration: self.setting.clone(), message,
                    instruction: format!("Choose a concise name describing this {label}'s responsibility using the configured case and permitted words.") });
            }
        }
    }
}

fn numbered_fragment(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["part", "section", "segment", "fragment", "chunk"]
        .iter()
        .any(|noun| {
            name.strip_prefix(noun).is_some_and(|suffix| {
                let suffix = suffix.strip_prefix('_').unwrap_or(suffix);
                !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
            })
        })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<crate::Report, crate::Error> {
        crate::Registry::default()
            .register::<super::Filename>()?
            .check(root)
    }
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    #[test]
    fn filename_word_limit_splits_cases_and_ignores_the_final_extension() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.filename]]\ntarget = 'src/*'\nmax_words = 2",
        );
        for file in [
            "one.rs",
            "two_words.rs",
            "twoWords.rs",
            "two-words.rs",
            "three_long_words.rs",
            "threeLongWords.rs",
            "three-long-words.rs",
        ] {
            write(root.path(), &format!("src/{file}"), "content");
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 3);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.message == "file name has 3 words; maximum is 2")
        );
    }

    #[test]
    fn filename_words_default_to_two_and_reject_invalid_configuration() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "src/long_file_name.rs", "content");
        write(root.path(), "src/long_file_other.rs", "content");
        write(
            root.path(),
            "linter.toml",
            "[[rules.filename]]\ntarget = 'src/*'",
        );
        assert_eq!(check(root.path()).unwrap().findings.len(), 2);
        for setting in ["max_words", "max_characters"] {
            for invalid in [
                format!("{setting} = 0"),
                format!("{setting} = -1"),
                format!("{setting} = 'two'"),
                format!("directories.{setting} = 2"),
            ] {
                write(
                    root.path(),
                    "linter.toml",
                    &format!("[[rules.filename]]\ntarget = 'src/*'\n{invalid}"),
                );
                assert!(
                    matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                    "{invalid}"
                );
            }
        }
    }
    #[test]
    fn targets_files_and_directories_independently() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "src/payment_accounts/data.rs", "content");
        write(root.path(), "src/payments/two_words.rs", "content");
        write(root.path(), "other/three_long_words.rs", "content");
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.filename]]
target = ["src/*"]
kind = "directory"
[[rules.filename]]
target = "src/**/*.rs"
kind = "file"
case = "snake_case"
"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("src/payment_accounts"));
        write(root.path(), "src/payments/BadName.rs", "content");
        assert_eq!(check(root.path()).unwrap().findings.len(), 2);
    }
    #[test]
    fn rejects_retired_selectors_and_unsupported_settings() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "glob = '*'",
            "target = []",
            "target = '../*'",
            "target = '*'\nkind = 'rust'",
            "target = '*'\nforbidden_words = ['helper']",
            "target = '*'\nmax_words = 0",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.filename]]\n{fields}"),
            );
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
    #[test]
    fn character_limit_counts_unicode_stems_and_directory_names() {
        let root = tempfile::tempdir().unwrap();
        for file in ["abcd.rs", "abcde.rs", "éééé.rs", "ééééé.rs"] {
            write(root.path(), &format!("src/{file}"), "content");
        }
        fs::create_dir_all(root.path().join("src/longer")).unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.filename]]\ntarget = 'src/*'\nmax_characters = 4",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 3);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.message.contains("characters; maximum is 4"))
        );
    }

    #[test]
    fn rejects_only_complete_numbered_fragment_names_when_enabled() {
        let root = tempfile::tempdir().unwrap();
        let rejected = [
            "part1",
            "part_2",
            "SECTION3",
            "segment_4",
            "fragment5",
            "chunk_06",
        ];
        let accepted = [
            "part",
            "section",
            "partition2",
            "part_2_extra",
            "part__2",
            "part-2",
            "chunk2d",
            "v2",
            "http2",
            "sha256",
            "version_2",
        ];
        for name in rejected.iter().chain(&accepted) {
            write(root.path(), &format!("src/{name}.rs"), "content");
        }
        write(
            root.path(),
            "linter.toml",
            "[[rules.filename]]\ntarget = 'src/*'\nmax_words = 10",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "linter.toml",
            "[[rules.filename]]\ntarget = 'src/*'\nmax_words = 10\nreject_numbered_fragments = true",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), rejected.len());
        for finding in report.findings {
            assert!(rejected.contains(&finding.path.file_stem().unwrap().to_str().unwrap()));
            assert!(finding.message.contains("numbered implementation fragment"));
        }
    }
}
