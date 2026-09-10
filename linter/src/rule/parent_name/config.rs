use crate::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    exclude: Option<Target>,
    #[serde(default)]
    ignored_names: Vec<String>,
}

pub(super) struct Assertion {
    pub selector: Selector,
    pub exclude: Option<Selector>,
    pub ignored_names: Vec<String>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"redundant-parent-name\"[{index}]");
                if value.ignored_names.iter().any(|name| {
                    name.is_empty()
                        || name.contains(['/', '\\'])
                        || matches!(
                            name.as_str(),
                            "\
                ." | "\
                .."
                        )
                }) {
                    return Err(Error::Configuration(format!(
                        "{setting}.ignored_names: expect\
                ed nonempty file stems, without path separators"
                    )));
                }
                Ok(Assertion {
                    selector: value.target.compile(&format!("{setting}.target"), true)?,
                    exclude: value
                        .exclude
                        .map(|target| target.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    ignored_names: value.ignored_names,
                    setting,
                })
            })
            .collect()
    }
}
