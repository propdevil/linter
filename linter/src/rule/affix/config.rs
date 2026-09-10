use crate::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    max_prefix: Option<usize>,
    max_suffix: Option<usize>,
}

pub(super) struct Assertion {
    pub selector: Selector,
    pub setting: String,
    pub prefix: Option<usize>,
    pub suffix: Option<usize>,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"shared-affix\"[{index}]");
                if (value.max_prefix.is_none() && value.max_suffix.is_none())
                    || value.max_prefix == Some(0)
                    || value.max_suffix == Some(0)
                {
                    return Err(Error::Configuration(format!(
                        "{setting}: configure at least one positive max_prefix or max_suffix"
                    )));
                }
                Ok(Assertion {
                    selector: value.target.compile(&format!("{setting}.target"), true)?,
                    setting,
                    prefix: value.max_prefix,
                    suffix: value.max_suffix,
                })
            })
            .collect()
    }
}
