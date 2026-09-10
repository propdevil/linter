use crate::{Error, Finding, Project, Rule, RuleResult, Status};
mod config;
use config::Assertion;
pub use config::Config;

pub struct ForbiddenWords {
    assertions: Vec<Assertion>,
}
impl Rule for ForbiddenWords {
    const ID: &'static str = "forbidden-words";
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
        use std::collections::BTreeSet;
        for entry in project.entries().filter(|entry| {
            (entry.kind.is_file() || entry.kind.is_dir()) && self.selector.matches(&entry.path)
        }) {
            let mut path = entry.path.clone();
            if entry.kind.is_file() {
                path.set_extension("");
            }
            let mut hits = BTreeSet::new();
            for component in path.components() {
                if let Some(text) = component.as_os_str().to_str() {
                    for word in text.to_snake_case().split('_') {
                        if self.words.contains(word) {
                            hits.insert(word.to_owned());
                        }
                    }
                }
            }
            if !hits.is_empty() {
                findings.push(Finding {
                    span: None,
                    related: Vec::new(),
                    rule: ForbiddenWords::ID,
                    path: entry.path.clone(),
                    configuration: self.setting.clone(),
                    message: format!(
                        "path contains forbidden word(s): {}",
                        hits.into_iter().collect::<Vec<_>>().join(
                            "\
                , "
                        )
                    ),
                    instruction: "Rename the file or offending directory component to de\
                scribe its responsibility."
                        .into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};
    fn check(root: &Path) -> Result<crate::Report, crate::Error> {
        crate::Registry::default()
            .register::<super::ForbiddenWords>()?
            .check(root)
    }
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    #[test]
    fn forbidden_filename_words_match_tokens_not_substrings_or_contents() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "linter.toml",
            r#"
[[rules."forbidden-words"]]
target = "src/*"
words = ["core", "helper", "utils"]
"#,
        );
        for file in [
            "core.rs",
            "payment_helper.rs",
            "paymentHelper.rs",
            "payment-utils.rs",
        ] {
            write(root.path(), &format!("src/{file}"), "content");
        }
        write(root.path(), "src/score.rs", "core helper utils");
        write(root.path(), "src/helpful.rs", "content");
        write(root.path(), "src/data.core", "content");
        fs::create_dir(root.path().join("src/helper")).unwrap();
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 5);
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.message.contains("forbidden word"))
        );
        write(
            root.path(),
            "linter.toml",
            "[[rules.\"forbidden-words\"]]\ntarget = 'src/*'\nwords = []",
        );
        assert!(check(root.path()).unwrap().findings.is_empty());
    }

    #[test]
    fn forbidden_filename_words_reject_invalid_settings() {
        let root = tempfile::tempdir().unwrap();
        for setting in [
            "words = ['']",
            "words = ['two words']",
            "words = ['two_words']",
            "words = ['core', 'Core']",
            "words = ['*core*']",
            "directories.forbidden_words = ['core']",
        ] {
            write(
                root.path(),
                "linter.toml",
                &format!("[[rules.\"forbidden-words\"]]\ntarget = '.'\n{setting}"),
            );
            assert!(
                matches!(check(root.path()), Err(crate::Error::Configuration(_))),
                "{setting}"
            );
        }
    }
    #[test]
    fn checks_ancestor_words_of_selected_files_and_honors_exclusions() {
        let root = tempfile::tempdir().unwrap();
        for path in [
            "src/helpers/payment.rs",
            "src/score.rs",
            "src/data.core",
            "ignored/helper.rs",
        ] {
            write(root.path(), path, "content");
        }
        write(
            root.path(),
            "linter.toml",
            r#"
[files]
exclude = ["ignored/**"]
[[rules."forbidden-words"]]
target = "**/*.rs"
words = ["helpers", "helper", "core"]
"#,
        );
        let report = check(root.path()).unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, Path::new("src/helpers/payment.rs"));
    }
}
