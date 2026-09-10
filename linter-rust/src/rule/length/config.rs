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
    #[serde(default = "maximum")]
    max_lines: usize,
}

fn maximum() -> usize {
    500
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub max_lines: usize,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"rust/file-length\"[{index}]");
                if value.max_lines == 0 {
                    return Err(Error::Configuration(format!(
                        "{setting}.max_lines: expected a positive integer"
                    )));
                }
                Ok(Assertion {
                    target: value.target.compile(&format!("{setting}.target"), true)?,
                    exclude: value
                        .exclude
                        .map(|target| target.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    max_lines: value.max_lines,
                    setting,
                })
            })
            .collect()
    }
}
