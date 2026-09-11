use super::api::{self, Methods};
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
    allowed_targets: Option<Target>,
    #[serde(default)]
    scope: Scope,
}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scope {
    #[default]
    Production,
    Tests,
    All,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub allowed: Option<Selector>,
    pub scope: Scope,
    pub functions: BTreeSet<String>,
    pub methods: &'static [Methods],
    pub contexts: BTreeSet<String>,
    pub adapters: BTreeSet<String>,
    pub setting: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| definition.compile(index))
            .collect()
    }
}
impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let setting = format!("rules.\"rust/async-blocking-operation\"[{index}]");
        Ok(Assertion {
            target: self.target.compile(&format!("{setting}.target"), true)?,
            exclude: self
                .exclude
                .map(|value| value.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            allowed: self
                .allowed_targets
                .map(|value| value.compile(&format!("{setting}.allowed_targets"), true))
                .transpose()?,
            scope: self.scope,
            functions: api::functions(),
            methods: api::METHODS,
            contexts: api::CONTEXTS.iter().map(|name| (*name).into()).collect(),
            adapters: api::ADAPTERS.iter().map(|name| (*name).into()).collect(),
            setting,
        })
    }
}
