use crate::{Error, Finding, Project, Rule, RuleResult, Status};
mod config;
use config::Assertion;
pub use config::Config;

pub struct SharedAffix {
    assertions: Vec<Assertion>,
}
impl Rule for SharedAffix {
    const ID: &'static str = "shared-affix";
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

type Groups =
    std::collections::BTreeMap<(std::path::PathBuf, &'static str, String), Vec<std::path::PathBuf>>;

impl Assertion {
    fn inspect(&self, project: &Project, findings: &mut Vec<Finding>) {
        let mut groups = Groups::new();
        for entry in project
            .entries()
            .filter(|entry| entry.kind.is_file() && self.selector.matches(&entry.path))
        {
            self.group(entry, &mut groups);
        }
        for ((parent, label, word), mut paths) in groups {
            let limit = if label == "prefix" {
                self.prefix
            } else {
                self.suffix
            };
            if limit.is_none_or(|limit| paths.len() <= limit) {
                continue;
            }
            paths.sort();
            let names = paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(Finding {
                span: None,
                related: Vec::new(),
                rule: SharedAffix::ID,
                path: parent,
                configuration: self.setting.clone(),
                message: format!("{} files share {label} word '{word}': {names}", paths.len()),
                instruction: format!(
                    concat!(
                        "Consider grouping these files under {}/ and removing the repeated {}; ",
                        "check ownership and name collisions first."
                    ),
                    word, label
                ),
            });
        }
    }

    fn group(&self, entry: &crate::Entry, groups: &mut Groups) {
        use heck::ToSnakeCase;
        let Some(stem) = entry.path.file_stem().and_then(|name| name.to_str()) else {
            return;
        };
        let normalized = stem.to_snake_case();
        let words: Vec<_> = normalized
            .split('_')
            .filter(|word| !word.is_empty())
            .collect();
        let [first, .., last] = words.as_slice() else {
            return;
        };
        let parent = entry
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        for (label, word, limit) in [
            ("prefix", first, self.prefix),
            ("suffix", last, self.suffix),
        ] {
            let selected = label != "suffix"
                || self
                    .suffix_words
                    .as_ref()
                    .is_none_or(|words| words.iter().any(|candidate| candidate == *word));
            if limit.is_none() || !selected {
                continue;
            }
            groups
                .entry((parent.to_path_buf(), label, (*word).to_owned()))
                .or_default()
                .push(entry.path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<crate::Report, crate::Error> {
        crate::Registry::default()
            .register::<super::SharedAffix>()?
            .check(root)
    }
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    #[test]
    fn shared_affixes_report_sibling_groups_at_the_configured_boundary() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"shared-affix\"]]\ntarget = 'src/*'\nmax_prefix = 2\nmax_suffix = 2",
        );
        for file in [
            "func_a.rs",
            "func_b.rs",
            "a_task.rs",
            "b_task.rs",
            "function_c.rs",
        ] {
            write(root.path(), &format!("src/{file}"), "content");
        }
        write(root.path(), "other/func_c.rs", "content");
        fs::create_dir(root.path().join("src/func_directory")).unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "src/funcC.rs", "content");
        write(root.path(), "src/c-task.rs", "content");
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.path == Path::new("src"))
        );
        assert!(report.findings.iter().any(
            |finding| finding.message.contains("prefix word 'func'")
                && finding.instruction.contains("func/")
                && finding.message.contains("funcC.rs")
        ));
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("suffix word 'task'")
                    && finding.instruction.contains("task/"))
        );
        assert_eq!(check(root.path()).unwrap(), report);
    }
    #[test]
    fn configured_suffix_vocabulary_only_counts_selected_roles() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "a_handler",
            "b_handler",
            "c_handler",
            "a_wallet",
            "b_wallet",
            "c_wallet",
        ] {
            write(root.path(), &format!("src/{name}.rs"), "content");
        }
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules."shared-affix"]]
target = "src/*"
max_suffix = 2
suffix_words = ["handler"]
"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].message.contains("suffix word 'handler'"));
        for fields in [
            "max_prefix = 2\nsuffix_words = ['handler']",
            "max_suffix = 2\nsuffix_words = []",
            "max_suffix = 2\nsuffix_words = ['Handler']",
            "max_suffix = 2\nsuffix_words = ['handler', 'handler']",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"shared-affix\"]]\ntarget = 'src/*'\n{fields}"),
            );
            assert!(matches!(
                check(root.path()),
                Err(crate::Error::Configuration(_))
            ));
        }
    }
    #[test]
    fn counts_only_selected_siblings_and_rejects_invalid_limits() {
        let root = tempfile::tempdir().unwrap();
        for path in ["src/func_a.rs", "src/func_b.rs", "src/func_c.txt"] {
            write(root.path(), path, "content");
        }
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"shared-affix\"]]\ntarget = '**/*.rs'\nmax_prefix = 2",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        for fields in [
            "",
            "max_prefix = 0",
            "max_suffix = -1",
            "max_prefix = 'two'",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"shared-affix\"]]\ntarget = '**/*.rs'\n{fields}"),
            );
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{fields}"
            );
        }
    }
}
