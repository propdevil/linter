use std::{fs, path::Path};

use crate::{Error, Finding, Project, Rule, RuleResult, Status};

mod children;
mod config;
pub use config::Config;
use config::{Assertion, Check};

pub struct Layout {
    assertions: Vec<Check>,
}

impl Rule for Layout {
    const ID: &'static str = "layout";
    type Config = Config;
    type Analysis = ();

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }

    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }

    fn check(&self, project: &Project, _: &()) -> Result<RuleResult, Error> {
        let status = if self.assertions.is_empty() {
            Status::Unconfigured
        } else {
            Status::Completed
        };
        let mut findings = Vec::new();
        let mut banned = Vec::<std::path::PathBuf>::new();
        for entry in project.entries() {
            if banned
                .iter()
                .any(|path| entry.path != *path && entry.path.starts_with(path))
            {
                continue;
            }
            let decision = self
                .assertions
                .iter()
                .filter_map(|check| match check {
                    Check::Permission(permission)
                        if permission.kind.matches(entry.kind)
                            && permission.selector.matches(&entry.path)
                            && (!permission.allow
                                || entry.kind.is_file()
                                || entry.kind.is_dir()) =>
                    {
                        Some(permission)
                    }
                    _ => None,
                })
                .next_back();
            if let Some(decision) = decision
                && !decision.allow
            {
                if entry.kind.is_dir() {
                    banned.push(entry.path.clone());
                }
                let label = if entry.kind.is_dir() {
                    "directory"
                } else {
                    "file"
                };
                findings.push(Finding { span: None, related: Vec::new(),
                    rule: Self::ID,
                    path: entry.path.clone(),
                    configuration: decision.setting.clone(),
                    message: format!("forbidden {label} {}", entry.path.display()),
                    instruction: "Remove or move this file, or add a later allow block with a purpose description.".into(),
                });
            }
        }
        for check in &self.assertions {
            let Check::Structure(assertion) = check else {
                continue;
            };
            let mut matches = 0;
            for directory in project
                .entries
                .directories()
                .filter(|path| assertion.selector.matches(path))
            {
                matches += 1;
                assertion.inspect(project, directory, &mut findings)?;
            }
            if matches == 0 {
                findings.push(assertion.finding(
                    Path::new("."),
                    "target matched no directories".into(),
                    "Create a matching directory or correct the configured glob.".into(),
                ));
            }
        }
        Ok(RuleResult { status, findings })
    }
}

impl Assertion {
    fn inspect(
        &self,
        project: &Project,
        directory: &Path,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        for (paths, expected) in [
            (&self.files.required, "file"),
            (&self.directories.required, "directory"),
        ] {
            for required in paths {
                let relative = directory.join(required);
                let actual = entry_kind(&project.root().join(directory), required)?;
                if actual == Some(expected) {
                    continue;
                }
                let (message, instruction) = match actual {
                    None => (
                        format!("missing required {expected} {}", required.display()),
                        format!("Create {expected} {}.", relative.display()),
                    ),
                    Some(actual) => (
                        format!("required {expected} {} is a {actual}", required.display()),
                        format!("Replace {} with a {expected}.", relative.display()),
                    ),
                };
                findings.push(self.finding(directory, message, instruction));
            }
        }
        self.inspect_children(project, directory, findings)
    }

    pub(super) fn finding(
        &self,
        directory: &Path,
        message: String,
        instruction: String,
    ) -> Finding {
        Finding {
            span: None,
            related: Vec::new(),
            rule: "layout",
            path: directory.to_path_buf(),
            configuration: self.setting.clone(),
            message,
            instruction,
        }
    }
}

// Inspect each component so an intermediate symlink cannot escape the selected directory.
fn entry_kind(directory: &Path, required: &Path) -> Result<Option<&'static str>, Error> {
    let mut path = directory.to_path_buf();
    let mut components = required.components().peekable();
    while let Some(component) = components.next() {
        path.push(component);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::io(&path, error)),
        };
        let kind = if metadata.is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "special entry"
        };
        if components.peek().is_none() || kind == "symlink" {
            return Ok(Some(kind));
        }
        if kind != "directory" {
            return Ok(None);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::Status;

    fn check(root: &Path) -> Result<crate::Report, crate::Error> {
        crate::Registry::default()
            .register::<crate::Layout>()?
            .check(root)
    }

    #[test]
    fn checks_empty_entries_with_independent_file_and_directory_settings() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "src"
files.allow_empty = false
directories.allow_empty = false
"#,
        );
        write(root.path(), "src/good_name.rs", "content");
        write(root.path(), "src/BadName.rs", "content");
        write(root.path(), "src/empty.rs", "");
        write(root.path(), "src/goodFolder/data", "content");
        write(root.path(), "src/bad_folder/data", "content");
        fs::create_dir(root.path().join("src/emptyFolder")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert_eq!(
            report
                .findings
                .iter()
                .map(|finding| finding.path.as_path())
                .collect::<Vec<_>>(),
            [Path::new("src/empty.rs"), Path::new("src/emptyFolder")]
        );
    }

    #[test]
    fn file_bans_allow_documented_globs_without_listing_each_document() {
        let root = tempfile::tempdir().unwrap();
        let policy = r#"
[[rules.layout]]
target = ["**/*.md", "**/*.markdown"]
allow = false
case_sensitive = false
[[rules.layout]]
target = ["docs/*.md"]
allow = true
description = "Documents the project and its rules."
"#;
        write(root.path(), "linter.toml", policy);
        write(root.path(), "docs/goal.md", "");
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "docs/new-rule.md", "");
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "README.md", "");
        write(root.path(), "docs/nested/extra.md", "");
        write(root.path(), "NOTES.MARKDOWN", "");
        let report = check(root.path()).unwrap();
        assert_eq!(
            report
                .findings
                .iter()
                .map(|finding| finding.path.as_path())
                .collect::<Vec<_>>(),
            [
                Path::new("NOTES.MARKDOWN"),
                Path::new("README.md"),
                Path::new("docs/nested/extra.md")
            ]
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.rule == "layout")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("linter.toml")).unwrap(),
            policy
        );
    }

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn enforces_rule_structure_and_reports_the_exact_missing_file() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "rules/*"
files.required = ["mod.rs", "config.rs", "readme.md"]
mode = "restrictive"
directories.allowed = [{ target = "fixtures", description = "Test entries." }]
"#,
        );
        for name in ["mod.rs", "config.rs", "readme.md"] {
            write(root.path(), &format!("rules/layout/{name}"), "");
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.rules["layout"], Status::Completed);
        assert!(report.findings.is_empty());

        fs::remove_file(root.path().join("rules/layout/readme.md")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.rule, "layout");
        assert_eq!(finding.path, Path::new("rules/layout"));
        assert_eq!(finding.configuration, "rules.layout[0]");
        assert_eq!(finding.message, "missing required file readme.md");
        assert_eq!(finding.instruction, "Create file rules/layout/readme.md.");
    }

    #[test]
    fn applies_overlapping_layouts_and_reports_types_and_extra_entries() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "apps/*"
files.required = ["Cargo.toml", "src/main.rs"]
directories.required = ["tests"]
mode = "restrictive"
files.allowed = [{ target = "*.txt", description = "Test entries." }]
[[rules.layout]]
target = "apps/cli"
files.required = ["src/extra.rs"]
"#,
        );
        write(root.path(), "apps/cli/src/main.rs", "");
        write(root.path(), "apps/cli/tests", "");
        write(root.path(), "apps/cli/notes.txt", "");
        write(root.path(), "apps/cli/unexpected", "");
        fs::create_dir(root.path().join("apps/cli/Cargo.toml")).unwrap();
        let report = check(root.path()).unwrap();
        let messages: Vec<_> = report
            .findings
            .iter()
            .map(|finding| finding.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "required directory tests is a file",
                "required file Cargo.toml is a directory",
                "unexpected entry unexpected",
                "missing required file src/extra.rs",
            ]
        );
        assert_eq!(check(root.path()).unwrap(), report);
    }

    #[test]
    fn restrictive_mode_separates_optional_files_and_directories() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "rules/*"
mode = "restrictive"
files = { required = ["mod.rs"], allowed = [{ target = "*.snap", description = "Test entries." }] }
directories = { required = ["tests"], allowed = [{ target = "fixtures", description = "Test entries." }] }
"#,
        );
        write(root.path(), "rules/layout/mod.rs", "");
        fs::create_dir(root.path().join("rules/layout/tests")).unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "rules/layout/result.snap", "");
        fs::create_dir(root.path().join("rules/layout/fixtures")).unwrap();
        assert!(check(root.path()).unwrap().findings.is_empty());

        fs::remove_dir(root.path().join("rules/layout/fixtures")).unwrap();
        write(root.path(), "rules/layout/fixtures", "Wrong type");
        fs::create_dir(root.path().join("rules/layout/directory.snap")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert_eq!(
            report.findings[0].message,
            "unexpected entry directory.snap"
        );
        assert_eq!(report.findings[1].message, "unexpected entry fixtures");

        let text = fs::read_to_string(root.path().join("linter.toml"))
            .unwrap()
            .replace("mode = \"restrictive\"", "mode = \"permissive\"")
            .replace(
                ", allowed = [{ target = \"*.snap\", description = \"Test entries.\" }]",
                "",
            )
            .replace(
                ", allowed = [{ target = \"fixtures\", description = \"Test entries.\" }]",
                "",
            );
        write(root.path(), "linter.toml", &text);
        assert!(check(root.path()).unwrap().findings.is_empty());
    }

    #[test]
    fn glob_segments_exclusions_and_unmatched_selectors_are_observable() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[files]
exclude = ["sources/**", "**/.git", "**/target"]
[[rules.layout]]
target = "**/rule"
files.required = ["mod.rs"]
[[rules.layout]]
target = "absent/*"
files.required = ["mod.rs"]
"#,
        );
        for directory in [
            "rule",
            "nested/rule",
            "sources/deeper/rule",
            "target/rule",
            ".git/rule",
            "nested/target/rule",
        ] {
            fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        let report = check(root.path()).unwrap();
        let paths: Vec<_> = report
            .findings
            .iter()
            .map(|finding| finding.path.as_path())
            .collect();
        assert_eq!(
            paths,
            [Path::new("."), Path::new("nested/rule"), Path::new("rule")]
        );
        assert_eq!(report.findings[0].message, "target matched no directories");

        write(
            root.path(),
            "linter.toml",
            r#"
[files]
exclude = ["**/.git", "**/target"]
[[rules.layout]]
target = "*/rule"
files.required = ["mod.rs"]
"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("nested/rule"));
    }

    #[test]
    fn supports_root_layout_empty_allowlist_and_optional_extra_entries() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "."
files.required = ["linter.toml"]
directories.required = ["empty"]
[[rules.layout]]
target = "empty"
mode = "restrictive"
"#,
        );
        fs::create_dir(root.path().join("empty")).unwrap();
        write(root.path(), "other", "allowed");
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "empty/extra", "");
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].message, "unexpected entry extra");
    }

    #[test]
    fn defaults_to_unconfigured_and_honors_disabling() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            check(root.path()).unwrap().rules["layout"],
            Status::Unconfigured
        );
        write(
            root.path(),
            "linter.toml",
            r#"
[rules.layout]
enabled = false
config = [{ target = "missing", files.required = ["missing"] }]
"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.rules["layout"], Status::Disabled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn rejects_invalid_configuration_including_disabled_rules() {
        let root = tempfile::tempdir().unwrap();
        for configuration in [
            "[rules.unknown]",
            "[rules.layout]\nenabld = false",
            "[[rules.layout]]\ntarget = '['",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['/absolute']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['../outside']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['.']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['C:\\outside']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['a']\ndirectories.required = ['a']",
            "[[rules.layout]]\ntarget = 'x'\nmode = 'restrictive'\nfiles.allowed = ['src/main.rs']",
            "[[rules.layout]]\ntarget = 'x'\nmode = 'restrictive'\nfiles.allowed = ['[']",
            "[[rules.layout]]\ntarget = '../outside'",
            "[[rules.layout]]\ntarget = '/absolute'",
            "[[rules.layout]]\ntarget = ''",
            "[[rules.layout]]\nfiles.required = ['a']",
            "[[rules.layout]]\ntarget = 'x'\nfile = ['a']",
            "[rules.layout]\nenabled = false\n[[rules.layout]]\ntarget = '['",
            "[files]\nexclude = ['[']",
            "[rules.layout]\nenabled = 'yes'",
            "[[rules.layout]]\ntarget = 'x'\nmode = 'invalid'",
            "[[rules.layout]]\ntarget = 'x'\nfiles.allowed = ['*.snap']",
            "[[rules.layout]]\ntarget = 'x'\nfiles.required = ['src', 'src/lib.rs']",
        ] {
            write(root.path(), "linter.toml", configuration);
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{configuration}"
            );
        }
    }

    #[test]
    fn missing_or_unreadable_inputs_are_execution_errors() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(
            check(&root.path().join("absent")),
            Err(crate::Error::Io { .. })
        ));
        write(root.path(), "file", "");
        assert!(matches!(
            check(&root.path().join("file")),
            Err(crate::Error::Io { .. })
        ));
        fs::create_dir(root.path().join("linter.toml")).unwrap();
        assert!(matches!(check(root.path()), Err(crate::Error::Io { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_never_satisfy_requirements_or_select_outside_directories() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "present.rs", "");
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "rules/*"
files.required = ["direct.rs", "src/present.rs"]
directories.required = ["linked"]
"#,
        );
        let directory = root.path().join("rules/layout");
        fs::create_dir_all(&directory).unwrap();
        symlink(outside.path(), root.path().join("rules/outside")).unwrap();
        symlink(
            outside.path().join("present.rs"),
            directory.join("direct.rs"),
        )
        .unwrap();
        symlink(outside.path(), directory.join("src")).unwrap();
        symlink(outside.path(), directory.join("linked")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 3);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.path == Path::new("rules/layout")
                    && finding.message.ends_with("is a symlink"))
        );
    }

    #[test]
    fn single_file_directories_are_opt_in_and_scoped_to_selected_parents() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "src/single/mod.rs", "content");
        write(root.path(), "src/pair/mod.rs", "content");
        write(root.path(), "src/pair/other.rs", "content");
        write(root.path(), "src/nested/mod.rs", "content");
        fs::create_dir(root.path().join("src/nested/child")).unwrap();
        fs::create_dir(root.path().join("src/empty")).unwrap();
        write(root.path(), "fixtures/single/input.rs", "content");
        write(root.path(), "src/root_file.rs", "content");
        let policy = r#"
[[rules.layout]]
target = "src"
"#;
        write(root.path(), "linter.toml", policy);
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(
            root.path(),
            "linter.toml",
            &format!("{policy}directories.allow_single_file = false"),
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("src/single"));
        assert_eq!(
            report.findings[0].message,
            "directory contains only one file"
        );
        assert!(report.findings[0].instruction.contains("Flatten"));
        write(
            root.path(),
            "linter.toml",
            &format!("{policy}directories.allow_single_file = true"),
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }

    #[test]
    fn single_file_setting_rejects_file_selections_and_invalid_types() {
        let root = tempfile::tempdir().unwrap();
        for setting in [
            "files.allow_single_file = false",
            "files.allow_single_file = true",
            "directories.allow_single_file = 'no'",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.layout]]\ntarget = '.'\n{setting}"),
            );
            assert!(matches!(
                check(root.path()),
                Err(crate::Error::Configuration(_))
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn single_file_check_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "only.rs", "content");
        fs::create_dir(root.path().join("src")).unwrap();
        symlink(outside.path(), root.path().join("src/linked")).unwrap();
        fs::create_dir(root.path().join("src/link_only")).unwrap();
        symlink(
            outside.path().join("only.rs"),
            root.path().join("src/link_only/link.rs"),
        )
        .unwrap();
        write(
            root.path(),
            "linter.toml",
            "[[rules.layout]]\ntarget = 'src'\ndirectories.allow_single_file = false",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }

    #[test]
    fn file_permissions_use_last_match_including_later_bans() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "docs/goal.md", "content");
        let ban = "[[rules.layout]]\ntarget = ['**/*.md']\nallow = false\n";
        let allow = "[[rules.layout]]\ntarget = ['docs/*.md']\nallow = true\ndescription = 'Project documentation.'\n";
        write(root.path(), "linter.toml", &format!("{ban}{allow}"));
        assert!(check(root.path()).unwrap().findings.is_empty());
        for policy in [format!("{allow}{ban}"), format!("{ban}{allow}{ban}")] {
            write(root.path(), "linter.toml", &policy);
            let report = check(root.path()).unwrap();
            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.findings[0].path, Path::new("docs/goal.md"));
            assert_eq!(
                report.findings[0].configuration,
                if policy.starts_with(allow) {
                    "rules.layout[1]"
                } else {
                    "rules.layout[2]"
                }
            );
        }
    }

    #[test]
    fn permission_allowances_do_not_disable_other_checks_or_require_files() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules.layout]]
target = "."
files.allow_empty = false
[[rules.layout]]
target = ["**/*.rs"]
allow = false
[[rules.layout]]
target = ["*.rs"]
allow = true
description = "Source files."
"#,
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
        write(root.path(), "BadName.rs", "");
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.configuration == "rules.layout[0]")
        );
    }

    #[test]
    fn ordered_permissions_reject_incomplete_and_retired_configuration() {
        let root = tempfile::tempdir().unwrap();
        for configuration in [
            "[[rules.layout]]\ntarget = ['*.md']\nallow = 'yes'",
            "[[rules.layout]]\ntarget = []\nallow = false",
            "[[rules.layout]]\ntarget = ['*.md']\nallow = true",
            "[[rules.layout]]\ntarget = ['*.md']\nallow = true\ndescription = ' '",
            "[[rules.layout]]\ntarget = ['[']\nallow = false",
            "[[rules.layout]]\ntarget = ['../outside']\nallow = false",
            "[[rules.layout]]\ntarget = ['*.md']\nallow = false\nunknown = true",
            "[[rules.layout]]\ntarget = '.'\nfiles.forbidden = ['*.md']",
            "[[rules.layout.config.layouts]]\ntarget = '.'",
            "[rules.layout]\nenabled = false\nconfig = [{ files = ['*.md'], allow = true }]",
        ] {
            write(root.path(), "linter.toml", configuration);
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{configuration}"
            );
        }
    }
    #[test]
    fn rust_preset_enforces_test_file_locations() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Cargo.toml", "[workspace]");
        let mut preset: toml::Value =
            toml::from_str(include_str!("../../../../configs/rust.toml")).unwrap();
        preset["rules"]
            .as_table_mut()
            .unwrap()
            .retain(|name, _| name == "layout");
        write(
            root.path(),
            "linter.toml",
            &toml::to_string(&preset).unwrap(),
        );
        for path in [
            "tests/global_test.rs",
            "apps/demo/tests/api_test.rs",
            "packages/demo/tests/tests.rs",
            "usecase/orders/tests/flow.rs",
            "src/lib.rs",
        ] {
            write(root.path(), path, "#[cfg(test)] mod tests {} ");
        }
        assert!(check(root.path()).unwrap().findings.is_empty());
        let forbidden = [
            "src/tests.rs",
            "src/test.rs",
            "src/foo_test.rs",
            "src/test_foo.rs",
            "src/foo-tests.rs",
            "src/foo.test.rs",
            "src/fooTest.rs",
            "src/UPPER_TEST.RS",
            "src/tests/check.rs",
            "apps/demo/src/tests/check.rs",
            "other/tests/check.rs",
        ];
        for path in forbidden {
            write(root.path(), path, "");
        }
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), forbidden.len());
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.message.starts_with("forbidden file"))
        );
    }
    #[test]
    fn bans_directories_once_and_honors_later_allowances() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "old/nested/value.rs", "content");
        write(
            root.path(),
            "linter.toml",
            "[[rules.layout]]\ntarget='old{,/**}'\nkind='any'\nallow=false",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("old"));
        write(
            root.path(),
            "linter.toml",
            "[[rules.layout]]\ntarget='old{,/**}'\nkind='any'\nallow=false\n[[rules.layout]]\ntarget='old{,/**}'\nkind='any'\nallow=true\ndescription='Retained migration inputs.'",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }
    #[test]
    fn ignores_configured_placeholders_when_assessing_directory_shape() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "src/empty/.gitkeep", "");
        write(root.path(), "src/single/.gitkeep", "");
        write(root.path(), "src/single/value.rs", "content");
        write(
            root.path(),
            "linter.toml",
            "[[rules.layout]]\ntarget='src'\ndirectories.allow_empty=false\ndirectories.allow_single_file=false\ndirectories.content_ignored=['.gitkeep']",
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 2);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message == "empty directory is not allowed")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.message == "directory contains only one file")
        );
    }
}
