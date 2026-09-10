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

impl Assertion {
    fn inspect(&self, project: &Project, findings: &mut Vec<Finding>) {
        use heck::ToSnakeCase;
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<(_, _, _), Vec<_>> = BTreeMap::new();
        for entry in project
            .entries()
            .filter(|entry| entry.kind.is_file() && self.selector.matches(&entry.path))
        {
            let Some(stem) = entry.path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            let normalized = stem.to_snake_case();
            let words: Vec<_> = normalized
                .split('_')
                .filter(|word| !word.is_empty())
                .collect();
            if let [first, .., last] = words.as_slice() {
                let parent = entry
                    .path
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new("."));
                for (label, word, limit) in [
                    ("prefix", first, self.prefix),
                    ("suffix", last, self.suffix),
                ] {
                    if limit.is_some() {
                        groups
                            .entry((parent.to_path_buf(), label, (*word).to_owned()))
                            .or_default()
                            .push(entry.path.clone());
                    }
                }
            }
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
            findings.push(Finding { span: None, related: Vec::new(), rule: SharedAffix::ID, path: parent, configuration: self.setting.clone(),
                message: format!("{} files share {label} word '{word}': {}", paths.len(), paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")),
                instruction: format!("Consider grouping these files under {word}/ and removing the repeated {label}; check ownership and name collisions first.") });
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
