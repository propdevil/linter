use crate::{Error, Selector, Target};
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
    #[serde(default = "columns")]
    max_columns: usize,
    #[serde(default = "tabs")]
    tab_width: usize,
}

fn columns() -> usize {
    100
}

fn tabs() -> usize {
    4
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub max_columns: usize,
    pub tab_width: usize,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"line-width\"[{index}]");
                for (name, limit) in [
                    ("max_columns", value.max_columns),
                    ("tab_width", value.tab_width),
                ] {
                    if limit == 0 {
                        return Err(Error::Configuration(format!(
                            "{setting}.{name}: expected a positive integer"
                        )));
                    }
                }
                Ok(Assertion {
                    target: value.target.compile(&format!("{setting}.target"), true)?,
                    exclude: value
                        .exclude
                        .map(|target| target.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    max_columns: value.max_columns,
                    tab_width: value.tab_width,
                    setting,
                })
            })
            .collect()
    }
}
