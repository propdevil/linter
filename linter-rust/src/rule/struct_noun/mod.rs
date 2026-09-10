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

pub struct StructNoun(Vec<Assertion>);

impl Rule for StructNoun {
    const ID: &'static str = "rust/struct-noun-naming";
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
        let language = English::new();
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
                    &language,
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
    language: &English,
) {
    if node.kind() == "struct_item" {
        let test = tests[node.start_byte()];
        let selected = match assertion.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        };
        if let Some(name) = node.child_by_field_name("name").filter(|_| selected) {
            let name = &source.text[name.byte_range()];
            if !identifier_words(name)
                .iter()
                .any(|word| assertion.accepted_words.contains(word) || language.noun(word))
            {
                findings.push(Finding {
                    rule: StructNoun::ID,
                    path: source.path.clone(),
                    configuration: assertion.setting.clone(),
                    message: format!("Rust struct `{name}` at line {} contains no recognized noun", node.start_position().row + 1),
                    instruction: "Name the value with a precise domain noun, or configure its technical vocabulary in accepted_words.".into(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        inspect(child, source, tests, assertion, findings, language);
    }
}

struct English {
    tagger: english_pos_tagger::Tagger,
    wordnet: wordnet_lemmatizer::Lemmatizer,
}
impl English {
    fn new() -> Self {
        Self {
            tagger: english_pos_tagger::Tagger::new(),
            wordnet: wordnet_lemmatizer::Lemmatizer::embedded(),
        }
    }
    fn noun(&self, word: &str) -> bool {
        self.wordnet
            .morphy(word, wordnet_lemmatizer::Pos::Noun)
            .is_some()
            || self
                .tagger
                .tag_raw_tokens(&[word])
                .first()
                .and_then(|token| token.pos.as_deref())
                .is_some_and(|part| part.starts_with("NN"))
    }
}

fn identifier_words(name: &str) -> Vec<String> {
    let mut normalized = String::new();
    let mut separator = false;
    for character in name.trim_start_matches("r#").chars() {
        if !character.is_ascii_alphanumeric() {
            separator = true;
            continue;
        }
        let follows_lowercase = normalized
            .chars()
            .last()
            .is_some_and(|value| value.is_ascii_lowercase());
        if (character.is_ascii_uppercase() && follows_lowercase)
            || (separator && !normalized.is_empty())
        {
            normalized.push('_');
        }
        normalized.push(character.to_ascii_lowercase());
        separator = false;
    }
    normalized
        .split('_')
        .map(|word| {
            word.chars()
                .filter(char::is_ascii_alphabetic)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(text: &str, config: &str) -> Result<Vec<Finding>, Error> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("lib.rs"), text).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!("[[rules.\"rust/struct-noun-naming\"]]\ntarget='**/*.rs'\n{config}"),
        )
        .unwrap();
        linter::Registry::default()
            .register::<StructNoun>()?
            .check(root.path())
            .map(|report| report.findings)
    }
    #[test]
    fn preserves_english_classifier_and_versioned_words() {
        let language = English::new();
        for word in [
            "workspace",
            "settings",
            "archive",
            "call",
            "capture",
            "mount",
            "register",
        ] {
            assert!(language.noun(word), "{word}");
        }
        assert!(!language.noun("selected"));
        assert_eq!(identifier_words("VkImageCopy2"), ["vk", "image", "copy"]);
        assert!(
            check(
                "struct VkImageCopy2; struct Workspaces; struct r#Wallet;",
                ""
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn checks_only_structs_without_donor_macro_suppressions() {
        let findings = check("struct Workspace; struct Selected; #[hl_design::naming(reason=\"external\")] struct Updated; enum Changed { Value } type Chosen = usize;", "").unwrap();
        assert_eq!(findings.len(), 2);
        assert!(findings[0].message.contains("`Selected`"));
        assert!(findings[1].message.contains("`Updated`"));
    }
    #[test]
    fn scope_does_not_hide_later_production_and_words_extend_classifier() {
        let text = "#[cfg(test)] mod checks { struct Selected; }\n#[cfg(test)] struct Updated;\n#[test] fn example() { struct Selected; }\nstruct Selected; struct Wallet;";
        let findings = check(text, "").unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("line 4"));
        assert_eq!(check(text, "scope='all'").unwrap().len(), 4);
        assert_eq!(check(text, "scope='tests'").unwrap().len(), 3);
        assert!(
            check(text, "accepted_words=['SELECTED']")
                .unwrap()
                .is_empty()
        );
        assert!(check(text, "exclude='lib.rs'").unwrap().is_empty());
    }
    #[test]
    fn integration_structs_are_test_scope() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("tests")).unwrap();
        fs::write(root.path().join("tests/input.rs"), "struct Selected;").unwrap();
        let registry = linter::Registry::default()
            .register::<StructNoun>()
            .unwrap();
        for (scope, expected) in [("production", 0), ("tests", 1)] {
            fs::write(
                root.path().join("linter.toml"),
                format!("[[rules.\"rust/struct-noun-naming\"]]\ntarget='**/*.rs'\nscope='{scope}'"),
            )
            .unwrap();
            assert_eq!(
                registry.check(root.path()).unwrap().findings.len(),
                expected
            );
        }
    }
    #[test]
    fn rejects_invalid_vocabulary_and_scope() {
        for config in [
            "accepted_words=['two words']",
            "accepted_words=['']",
            "accepted_words=['word','WORD']",
            "accepted_words=['name2']",
            "scope='unknown'",
            "exclude=[]",
            "unknown=true",
        ] {
            assert!(
                matches!(check("", config), Err(Error::Configuration(_))),
                "{config}"
            );
        }
    }
}
