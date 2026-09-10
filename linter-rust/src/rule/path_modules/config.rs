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
    #[serde(default = "default_limit")]
    max_child_domains: usize,
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
    pub max_child_domains: usize,
    pub setting: String,
}

fn default_limit() -> usize {
    1
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
        let setting = format!("rules.\"rust/path-module-flattening\"[{index}]");
        if definition.max_child_domains == 0 {
            return Err(Error::Configuration(format!(
                "{setting}.max_child_domains: expected a positive limit"
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
            max_child_domains: definition.max_child_domains,
            setting,
        })
    }
}
