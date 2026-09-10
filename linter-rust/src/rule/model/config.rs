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
    #[serde(default = "fields")]
    min_shared_fields: usize,
    #[serde(default = "overlap")]
    min_overlap_percent: usize,
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
    pub min_shared_fields: usize,
    pub min_overlap_percent: usize,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/wire-domain-model-duplication\"[{index}]");
                if definition.min_shared_fields < 3 || !(1..=100).contains(&definition.min_overlap_percent) {
                    return Err(Error::Configuration(format!("{setting}: min_shared_fields must be at least 3 and min_overlap_percent between 1 and 100")));
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
                    min_shared_fields: definition.min_shared_fields,
                    min_overlap_percent: definition.min_overlap_percent,
                    setting,
                })
            })
            .collect()
    }
}

fn fields() -> usize {
    3
}
fn overlap() -> usize {
    75
}
