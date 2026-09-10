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
    forbidden_words: Vec<String>,
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
    pub forbidden_words: std::collections::BTreeSet<String>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/module-name\"[{index}]");
                let mut words = std::collections::BTreeSet::new();
                for word in definition.forbidden_words {
                    if word.is_empty()
                        || !word.chars().all(|c| c.is_ascii_alphabetic())
                        || !words.insert(word.to_ascii_lowercase())
                    {
                        return Err(Error::Configuration(format!(
                            "{setting}.forbidden_words: expected unique ASCII alphabetic words"
                        )));
                    }
                }
                if words.is_empty() {
                    return Err(Error::Configuration(format!(
                        "{setting}.forbidden_words: expected at least one word"
                    )));
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
                    forbidden_words: words,
                    setting,
                })
            })
            .collect()
    }
}
