use linter::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    #[serde(default)]
    exclude: Option<Target>,
    #[serde(default)]
    scope: Scope,
    #[serde(default)]
    ignored_words: Vec<String>,
    state_words: Vec<String>,
    #[serde(default = "variants")]
    min_variants: usize,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scope {
    #[default]
    Production,
    Tests,
    All,
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub scope: Scope,
    pub ignored_words: std::collections::BTreeSet<String>,
    pub state_words: std::collections::BTreeSet<String>,
    pub min_variants: usize,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/string-backed-finite-state\"[{index}]");
                if definition.min_variants < 2 {
                    return Err(Error::Configuration(format!(
                        "{setting}.min_variants: expected at least two"
                    )));
                }
                let state_words = words(definition.state_words, &format!("{setting}.state_words"))?;
                if state_words.is_empty() {
                    return Err(Error::Configuration(format!(
                        "{setting}.state_words: expected at least one state word"
                    )));
                }
                let ignored_words = words(
                    definition.ignored_words,
                    &format!("{setting}.ignored_words"),
                )?;
                Ok(Assertion {
                    target: definition
                        .target
                        .compile(&format!("{setting}.target"), true)?,
                    exclude: definition
                        .exclude
                        .map(|value| value.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    scope: definition.scope,
                    state_words,
                    ignored_words,
                    min_variants: definition.min_variants,
                    setting,
                })
            })
            .collect()
    }
}

fn variants() -> usize {
    3
}
fn words(values: Vec<String>, setting: &str) -> Result<std::collections::BTreeSet<String>, Error> {
    let mut words = std::collections::BTreeSet::new();
    for value in values {
        if value.is_empty()
            || !value
                .chars()
                .all(|character| character.is_ascii_alphabetic())
            || !words.insert(value.to_ascii_lowercase())
        {
            return Err(Error::Configuration(format!(
                "{setting}: expected unique ASCII alphabetic words"
            )));
        }
    }
    Ok(words)
}
impl Assertion {
    pub fn candidate(&self, name: &str) -> bool {
        let normalized = name.to_ascii_lowercase();
        let word = normalized.rsplit('_').next().unwrap_or_default();
        self.state_words.contains(word) && !self.ignored_words.contains(word)
    }
    pub fn selected(&self, test: bool) -> bool {
        match self.scope {
            Scope::Production => !test,
            Scope::Tests => test,
            Scope::All => true,
        }
    }
}
