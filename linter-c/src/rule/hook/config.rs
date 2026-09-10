use linter::{Error, Selector, Target};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    exclude: Option<Target>,
    macros: Vec<String>,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub macros: BTreeSet<String>,
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
        let value = self;
        let setting = format!("rules.\"c/test-only-state\"[{index}]");
        if value.macros.is_empty()
            || value
                .macros
                .iter()
                .any(|name| !crate::recovery::identifier(name))
        {
            return Err(Error::Configuration(format!(
                "{setting}.macros: expected nonempty exact C function names"
            )));
        }
        Ok(Assertion {
            target: value.target.compile(&format!("{setting}.target"), true)?,
            exclude: value
                .exclude
                .map(|target| target.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            macros: value.macros.into_iter().collect(),
            setting,
        })
    }
}
