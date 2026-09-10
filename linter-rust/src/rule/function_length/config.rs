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
    max_lines: usize,
}

fn default_limit() -> usize {
    50
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
    pub max_lines: usize,
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
        let setting = format!("rules.\"rust/function-length\"[{index}]");
        if definition.max_lines == 0 {
            return Err(Error::Configuration(format!(
                "{setting}.max_lines: expected a positive integer"
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
            max_lines: definition.max_lines,
            setting,
        })
    }
}
