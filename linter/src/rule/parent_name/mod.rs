use std::collections::BTreeSet;

use crate::{Error, Finding, Project, Rule, RuleResult, Status};
use heck::ToSnakeCase;

mod config;
use config::Assertion;
pub use config::Config;

pub struct ParentName {
    assertions: Vec<Assertion>,
}

impl Rule for ParentName {
    const ID: &'static str = "redundant-parent-name";
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
            for entry in project.entries().filter(|entry| {
                entry.kind.is_file()
                    && assertion.selector.matches(&entry.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|selector| selector.matches(&entry.path))
            }) {
                let Some(stem) = entry.path.file_stem().and_then(|name| name.to_str()) else {
                    continue;
                };
                if assertion.ignored_names.iter().any(|name| name == stem) {
                    continue;
                }
                let Some(parent) = entry
                    .path
                    .parent()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                else {
                    continue;
                };
                let parent_words = words(parent);
                let stem_words = words(stem);
                let repeated = parent_words
                    .intersection(&stem_words)
                    .cloned()
                    .collect::<Vec<_>>();
                if repeated.is_empty() {
                    continue;
                }
                let repeated = repeated.join(", ");
                findings.push(Finding {
                    rule: Self::ID,
                    path: entry.path.clone(),
                    configuration: assertion.setting.clone(),
                    message: format!("filename '{stem}' repeats parent '{parent}' words: {repeated}"),
                    instruction: format!("Remove repeated words ({repeated}) from the filename; the immediate parent already supplies that context. Check name collisions before renaming."),
                });
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn words(value: &str) -> BTreeSet<String> {
    value
        .to_snake_case()
        .split('_')
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn write(root: &Path, path: &str, contents: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn check(root: &Path) -> Result<crate::Report, crate::Error> {
        crate::Registry::default()
            .register::<super::ParentName>()?
            .check(root)
    }

    #[test]
    fn transfers_parent_name_cases_and_normalizes_complete_words() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"redundant-parent-name\"]]\ntarget = 'src/**'",
        );
        for path in [
            "memory/shared_memory.c",
            "memory/memory_map.h",
            "net_work/socket_work.rs",
            "HTTPServer/serverHTTP.rs",
            "memory/memorial.c",
            "memory/shared.c",
            "net_work/network.rs",
            "memory/index.c",
            "memory/child/memory.rs",
        ] {
            write(root.path(), &format!("src/{path}"), "");
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 4);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message.contains("words: http, server"))
        );
        assert_eq!(report, check(root.path()).unwrap());
    }

    #[test]
    fn selections_ignores_and_project_exclusions_are_independent() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[files]
exclude = ["generated/**"]
[[rules."redundant-parent-name"]]
target = ["**/*.rs", "**/*.c"]
exclude = "memory/memory_excluded.rs"
ignored_names = ["mod"]
"#,
        );
        for path in [
            "mod/mod.rs",
            "memory/memory_excluded.rs",
            "generated/generated.rs",
            "memory/memory.txt",
            "root.rs",
            "src/src.rs",
        ] {
            write(root.path(), path, "");
        }
        fs::create_dir_all(root.path().join("folder/folder.rs")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("src/src.rs"));
    }

    #[test]
    fn rejects_missing_invalid_or_unknown_configuration() {
        let root = tempfile::tempdir().unwrap();
        for fields in [
            "",
            "target = []",
            "target = '../*'",
            "target = '*'\nexclude = []",
            "target = '*'\nignored_names = ['']",
            "target = '*'\nignored_names = ['src/mod']",
            "target = '*'\nunknown = true",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"redundant-parent-name\"]]\n{fields}"),
            );
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{fields}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"redundant-parent-name\"]]\ntarget = '**/*.rs'",
        );
        write(root.path(), "original.rs", "");
        fs::create_dir(root.path().join("memory")).unwrap();
        std::os::unix::fs::symlink(
            root.path().join("original.rs"),
            root.path().join("memory/memory.rs"),
        )
        .unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
}
