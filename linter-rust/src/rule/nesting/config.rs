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
    max_depth: usize,
    #[serde(default = "default_guards")]
    ignore_guard_clauses: bool,
}

fn default_guards() -> bool {
    true
}

fn default_limit() -> usize {
    2
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
    pub max_depth: usize,
    pub ignore_guard_clauses: bool,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/nesting\"[{index}]");
                if definition.max_depth == 0 {
                    return Err(Error::Configuration(format!(
                        "{setting}.max_depth: expected a positive integer"
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
                    max_depth: definition.max_depth,
                    ignore_guard_clauses: definition.ignore_guard_clauses,
                    setting,
                })
            })
            .collect()
    }
}
