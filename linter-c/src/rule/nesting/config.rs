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
    #[serde(default = "default_limit")]
    max_depth: usize,
}

fn default_limit() -> usize {
    6
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub max_depth: usize,
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
        let setting = format!("rules.\"c/nesting\"[{index}]");
        if value.max_depth == 0 {
            return Err(Error::Configuration(format!(
                "{setting}.max_depth: expected a positive limit"
            )));
        }
        Ok(Assertion {
            target: value.target.compile(&format!("{setting}.target"), true)?,
            exclude: value
                .exclude
                .map(|target| target.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            max_depth: value.max_depth,
            setting,
        })
    }
}
