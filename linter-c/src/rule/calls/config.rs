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
    functions: Vec<String>,
    description: String,
    instruction: String,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub functions: BTreeSet<String>,
    pub setting: String,
    pub description: String,
    pub instruction: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"c/forbidden-call\"[{index}]");
                if value.functions.is_empty()
                    || value.functions.iter().any(|name| {
                        name.is_empty()
                            || !name.bytes().enumerate().all(|(index, byte)| {
                                byte == b'_'
                                    || byte.is_ascii_alphabetic()
                                    || (index > 0 && byte.is_ascii_digit())
                            })
                    })
                {
                    return Err(Error::Configuration(format!(
                        "{setting}.functions: expected nonempty exact C function names"
                    )));
                }
                if value.description.trim().is_empty() || value.instruction.trim().is_empty() {
                    return Err(Error::Configuration(format!(
                        "{setting}: description and instruction must be nonempty"
                    )));
                }
                Ok(Assertion {
                    target: value.target.compile(&format!("{setting}.target"), true)?,
                    exclude: value
                        .exclude
                        .map(|target| target.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    functions: value.functions.into_iter().collect(),
                    setting,
                    description: value.description,
                    instruction: value.instruction,
                })
            })
            .collect()
    }
}
