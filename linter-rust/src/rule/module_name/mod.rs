use std::fs;

use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use tree_sitter::Node;

use crate::{
    Analysis, Source,
    scope::{integration, mark_tests},
};
use config::{Assertion, Scope};

mod config;
pub use config::Config;

pub struct ModuleName(Vec<Assertion>);

impl Rule for ModuleName {
    const ID: &'static str = "rust/module-name";
    type Analysis = Analysis;
    type Config = Config;

    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self(config.compile()?))
    }

    fn configured(&self) -> bool {
        !self.0.is_empty()
    }

    fn check(&self, project: &Project, analysis: &Analysis) -> Result<RuleResult, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let mut findings = Vec::new();
        for source in &analysis.sources {
            let mut tests = vec![false; source.text.len()];
            if integration(source, &root, analysis) {
                tests.fill(true);
            } else {
                mark_tests(source.syntax.root_node(), &source.text, &mut tests);
            }
            for assertion in self.0.iter().filter(|assertion| {
                assertion.target.matches(&source.path)
                    && !assertion
                        .exclude
                        .as_ref()
                        .is_some_and(|exclude| exclude.matches(&source.path))
            }) {
                inspect(
                    source.syntax.root_node(),
                    source,
                    &tests,
                    assertion,
                    &mut findings,
                );
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

fn inspect(
    node: Node<'_>,
    source: &Source,
    tests: &[bool],
    assertion: &Assertion,
    findings: &mut Vec<Finding>,
) {
    if node.kind() == "mod_item" {
        let test = tests[node.start_byte()];
        let selected = match assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        if let Some(name) = node.child_by_field_name("name").filter(|_| selected) {
            let name = &source.text[name.byte_range()];
            use heck::ToSnakeCase;
            let normalized = name.trim_start_matches("r#").to_snake_case();
            let hits: std::collections::BTreeSet<_> = normalized
                .split('_')
                .filter(|word| assertion.forbidden_words.contains(*word))
                .collect();
            if !hits.is_empty() {
                findings.push(Finding { span: Some(linter::Span::new(&source.text, node.byte_range())), related: Vec::new(),
                    rule: ModuleName::ID,
                    path: source.path.clone(),
                    configuration: assertion.setting.clone(),
                    message: format!("Rust module `{name}` contains forbidden word(s): {}", hits.into_iter().collect::<Vec<_>>().join(", ")),
                    instruction: "Name the module for its owned entity, capability, algorithm, or external mechanism.".into(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(path: &str, text: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/module-name\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<ModuleName>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    const CONFIG: &str =
        "forbidden_words=['util','utils','core','common','shared','helper','helpers','misc']";
    #[test]
    fn checks_rust_module_declarations_only() {
        let source = "mod util {} mod r#common {} mod utility {} fn helper() {} struct SharedState; use external_crate::core; const PROSE:&str=\"mod misc {}\";";
        let found = check("lib.rs", source, CONFIG).unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`util`"))
        );
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`r#common`"))
        );
    }
    #[test]
    fn uses_complete_words_and_finds_nested_external_and_path_modules() {
        let source = "mod owner {mod shared_values; mod helper_domain{} mod CoreData{}} #[path=\"entities.rs\"] mod common; #[path=\"helpers.rs\"] mod transactions; mod utility; mod score;";
        let found = check("src/lib.rs", source, CONFIG).unwrap();
        assert_eq!(found.len(), 4);
        assert!(
            found
                .iter()
                .any(|finding| finding.message.contains("`common`"))
        );
        assert!(found.iter().all(|finding| finding.span.is_some()));
        assert!(
            check("src/helpers.rs", "struct Entity;", CONFIG)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn detects_external_declaration_even_when_file_is_loaded() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), "mod common;").unwrap();
        fs::write(root.path().join("common.rs"), "pub struct Entity;").unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/module-name\"]]\ntarget='*.rs'\n{CONFIG}"),
        )
        .unwrap();
        let report = linter::Registry::default()
            .register::<ModuleName>()
            .unwrap()
            .check(root.path())
            .unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, std::path::Path::new("lib.rs"));
    }
    #[test]
    fn scopes_targets_and_vocabulary_are_configured() {
        let text = "mod util{} #[cfg(test)] mod common{}";
        assert_eq!(check("lib.rs", text, CONFIG).unwrap().len(), 1);
        assert_eq!(
            check("lib.rs", text, &format!("{CONFIG}\nscope='all'"))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            check("lib.rs", text, &format!("{CONFIG}\nscope='tests'"))
                .unwrap()
                .len(),
            1
        );
        assert!(check("tests/input.rs", text, CONFIG).unwrap().is_empty());
        assert_eq!(
            check("tests/input.rs", text, &format!("{CONFIG}\nscope='tests'"))
                .unwrap()
                .len(),
            2
        );
        assert!(
            check("lib.rs", text, &format!("{CONFIG}\nexclude='*.rs'"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            check("lib.rs", text, "forbidden_words=['UTIL']")
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn rejects_invalid_or_empty_vocabulary_and_bad_settings() {
        for config in [
            "",
            "forbidden_words=[]",
            "forbidden_words=['a','A']",
            "forbidden_words=['two words']",
            "forbidden_words=['']",
            "forbidden_words=['misc']\nunknown=true",
            "forbidden_words=['misc']\nscope='unknown'",
            "forbidden_words=['misc']\nexclude=[]",
        ] {
            assert!(
                matches!(check("lib.rs", "", config), Err(Error::Configuration(_))),
                "{config}"
            );
        }
    }
    #[test]
    fn directives_and_implementation_pass() {
        let text =
            "// linter:disable rust/module-name -- mirrors external library namespace\nmod common;";
        assert!(check("lib.rs", text, CONFIG).unwrap().is_empty());
        for text in [include_str!("mod.rs"), include_str!("config.rs")] {
            assert!(check("lib.rs", text, CONFIG).unwrap().is_empty());
        }
    }
}
