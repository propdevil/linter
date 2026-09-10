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
    #[serde(default = "columns")]
    max_columns: usize,
    #[serde(default = "tabs")]
    tab_width: usize,
}

fn columns() -> usize {
    20
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
            .map(|(index, value)| value.compile(index))
            .collect()
    }
}

impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let setting = format!("rules.\"rust/max-indent\"[{index}]");
        if self.max_columns == 0 || self.tab_width == 0 {
            return Err(Error::Configuration(format!(
                "{setting}: max_columns and tab_width must be positive"
            )));
        }
        Ok(Assertion {
            target: self.target.compile(&format!("{setting}.target"), true)?,
            exclude: self
                .exclude
                .map(|target| target.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            max_columns: self.max_columns,
            tab_width: self.tab_width,
            setting,
        })
    }
}
