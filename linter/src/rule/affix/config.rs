use crate::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    max_prefix: Option<usize>,
    max_suffix: Option<usize>,
    suffix_words: Option<Vec<String>>,
}

pub(super) struct Assertion {
    pub selector: Selector,
    pub setting: String,
    pub prefix: Option<usize>,
    pub suffix: Option<usize>,
    pub suffix_words: Option<Vec<String>>,
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
        let value = self;
        let setting = format!("rules.\"shared-affix\"[{index}]");
        if (value.max_prefix.is_none() && value.max_suffix.is_none())
            || value.max_prefix == Some(0)
            || value.max_suffix == Some(0)
        {
            return Err(Error::Configuration(format!(
                "{setting}: configure at least one positive max_prefix or max_suffix"
            )));
        }
        if let Some(words) = &value.suffix_words {
            if value.max_suffix.is_none() || words.is_empty() {
                return Err(Error::Configuration(format!(
                    "{setting}.suffix_words: requires max_suffix and nonempty words"
                )));
            }
            let mut seen = std::collections::BTreeSet::new();
            for word in words {
                if word.is_empty()
                    || !word.chars().all(|character| character.is_ascii_lowercase())
                    || !seen.insert(word)
                {
                    return Err(Error::Configuration(format!(
                        "{setting}.suffix_words: expected unique lowercase words"
                    )));
                }
            }
        }
        Ok(Assertion {
            selector: value.target.compile(&format!("{setting}.target"), true)?,
            setting,
            prefix: value.max_prefix,
            suffix: value.max_suffix,
            suffix_words: value.suffix_words,
        })
    }
}
