use crate::{Error, Selector, Target};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    #[serde(default)]
    kind: Kind,
    #[serde(default)]
    case: Option<Case>,
    #[serde(default)]
    max_words: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) enum Case {
    #[serde(rename = "snake_case")]
    Snake,
    #[serde(rename = "camel_case")]
    Camel,
    #[serde(rename = "pascal_case")]
    Pascal,
    #[serde(rename = "kebab_case")]
    Kebab,
}

pub(super) struct Assertion {
    pub kind: Kind,
    pub selector: Selector,
    pub setting: String,
    pub case: Option<Case>,
    pub max_words: Option<usize>,
}

impl Definition {
    fn compile(self, setting: String) -> Result<Assertion, Error> {
        let max_words = self.max_words;
        if max_words == Some(0) {
            return Err(Error::Configuration(format!(
                "{setting}.max_words: expected a positive integer"
            )));
        }
        Ok(Assertion {
            kind: self.kind,
            selector: self.target.compile(&format!("{setting}.target"), true)?,
            setting,
            case: self.case,
            max_words,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    #[default]
    Any,
    File,
    Directory,
}

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| definition.compile(format!("rules.filename[{index}]")))
            .collect()
    }
}
