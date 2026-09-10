use crate::{Error, Selector, Target};
use heck::ToSnakeCase;
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    words: Vec<String>,
}

pub(super) struct Assertion {
    pub selector: Selector,
    pub setting: String,
    pub words: BTreeSet<String>,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"forbidden-words\"[{index}]");
                let mut words = BTreeSet::new();
                for word in value.words {
                    let normalized = word.to_snake_case();
                    if !word.chars().all(char::is_alphanumeric)
                        || !word.chars().any(char::is_alphabetic)
                        || normalized.contains('_')
                        || !words.insert(normalized)
                    {
                        return Err(Error::Configuration(format!(
                            "{setting}.words: expected unique individual words, got {word:?}"
                        )));
                    }
                }
                Ok(Assertion {
                    selector: value.target.compile(&format!("{setting}.target"), true)?,
                    setting,
                    words,
                })
            })
            .collect()
    }
}
