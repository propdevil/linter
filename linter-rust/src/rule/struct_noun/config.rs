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
    accepted_words: Vec<String>,
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
    pub accepted_words: std::collections::BTreeSet<String>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| value.compile(index))
            .collect()
    }
}

impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let definition = self;
        let setting = format!("rules.\"rust/struct-noun-naming\"[{index}]");
        let mut words = std::collections::BTreeSet::new();
        for word in definition.accepted_words {
            if word.is_empty()
                || !word.chars().all(|c| c.is_ascii_alphabetic())
                || !words.insert(word.to_ascii_lowercase())
            {
                return Err(Error::Configuration(format!(
                    "{setting}.accepted_words: expected unique ASCII alphabetic words"
                )));
            }
        }
        Ok(Assertion {
            target: definition
                .target
                .compile(&format!("{setting}.target"), true)?,
            exclude: definition
                .exclude
                .map(|value| value.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            scope: definition.scope,
            accepted_words: words,
            setting,
        })
    }
}
