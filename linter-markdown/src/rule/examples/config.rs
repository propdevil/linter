use linter::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    exclude: Option<Target>,
    #[serde(default = "yes")]
    require_title: bool,
    #[serde(default = "one")]
    min_cases: usize,
    #[serde(default = "yes")]
    require_closed_fences: bool,
}

fn yes() -> bool {
    true
}
fn one() -> usize {
    1
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub require_title: bool,
    pub min_cases: usize,
    pub require_closed_fences: bool,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"markdown/examples\"[{index}]");
                if !value.require_title && value.min_cases == 0 && !value.require_closed_fences {
                    return Err(Error::Configuration(format!(
                        "{setting}: enable at least one check"
                    )));
                }
                Ok(Assertion {
                    target: value.target.compile(&format!("{setting}.target"), true)?,
                    exclude: value
                        .exclude
                        .map(|target| target.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    require_title: value.require_title,
                    min_cases: value.min_cases,
                    require_closed_fences: value.require_closed_fences,
                    setting,
                })
            })
            .collect()
    }
}
