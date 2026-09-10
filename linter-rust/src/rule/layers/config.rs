use std::collections::BTreeSet;

use linter::Error;
use linter::{Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    name: String,
    target: Target,
    dependencies: Vec<String>,
}

pub(super) struct Layer {
    pub name: String,
    pub selector: Selector,
    pub dependencies: BTreeSet<String>,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Layer>, Error> {
        let mut names = BTreeSet::new();
        for definition in &self.0 {
            if definition.name.trim().is_empty() || !names.insert(definition.name.clone()) {
                return Err(Error::Configuration(
                    "rust/layers: layer names must be nonempty and unique".into(),
                ));
            }
        }
        self.0
            .into_iter()
            .map(|definition| {
                let mut dependencies = BTreeSet::new();
                for name in definition.dependencies {
                    if !names.contains(&name) || !dependencies.insert(name.clone()) {
                        return Err(Error::Configuration(format!(
                            "rust/layers: unknown or duplicate dependency layer {name:?}"
                        )));
                    }
                }
                let selector = definition.target.compile("rust/layers.target", true)?;
                Ok(Layer {
                    name: definition.name,
                    selector,
                    dependencies,
                })
            })
            .collect()
    }
}
