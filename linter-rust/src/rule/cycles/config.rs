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
    #[serde(default = "kinds")]
    kinds: Vec<crate::DependencyKind>,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub kinds: Vec<crate::DependencyKind>,
    pub setting: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/dependency-cycles\"[{index}]");
                if definition.kinds.is_empty()
                    || definition
                        .kinds
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != definition.kinds.len()
                {
                    return Err(Error::Configuration(format!(
                        "{setting}.kinds must be nonempty and unique"
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
                    kinds: definition.kinds,
                    setting,
                })
            })
            .collect()
    }
}

fn kinds() -> Vec<crate::DependencyKind> {
    vec![crate::DependencyKind::Normal, crate::DependencyKind::Build]
}
