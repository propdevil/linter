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
    #[serde(default)]
    blocking_functions: Vec<String>,
    #[serde(default)]
    blocking_methods: Vec<Methods>,
    #[serde(default)]
    blocking_contexts: Vec<String>,
    #[serde(default)]
    guard_adapters: Vec<String>,
}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scope {
    #[default]
    Production,
    Tests,
    All,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Methods {
    pub receiver: String,
    pub methods: Vec<String>,
    #[serde(default)]
    pub constructors: Vec<String>,
    #[serde(default)]
    pub fluent_methods: Vec<String>,
    #[serde(default)]
    pub returns_guard: bool,
}
pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub allowed: Option<Selector>,
    pub scope: Scope,
    pub functions: BTreeSet<String>,
    pub methods: Vec<Methods>,
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
fn validate(value: &str, path: bool, setting: &str) -> Result<(), Error> {
    if (!path && value.contains("::")) || value.split("::").any(|word| !identifier(word)) {
        return Err(Error::Configuration(format!(
            "{setting}: invalid API identifier '{value}'"
        )));
    }
    Ok(())
}

impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let setting = format!("rules.\"rust/async-blocking-operation\"[{index}]");
        if self.blocking_functions.is_empty() && self.blocking_methods.is_empty() {
            return Err(Error::Configuration(format!(
                "{setting}: configure blocking_functions or blocking_methods"
            )));
        }
        for path in self
            .blocking_functions
            .iter()
            .chain(&self.blocking_contexts)
        {
            validate(path, true, &setting)?;
        }
        for method in &self.guard_adapters {
            validate(method, false, &setting)?;
        }
        for policy in &self.blocking_methods {
            policy.validate(&setting)?;
        }
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
            functions: self.blocking_functions.into_iter().collect(),
            methods: self.blocking_methods,
            contexts: self.blocking_contexts.into_iter().collect(),
            adapters: self.guard_adapters.into_iter().collect(),
            setting,
        })
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

impl Methods {
    fn validate(&self, setting: &str) -> Result<(), Error> {
        validate(&self.receiver, true, setting)?;
        if self.methods.is_empty() {
            return Err(Error::Configuration(format!(
                "{setting}.blocking_methods: methods cannot be empty"
            )));
        }
        for method in self.methods.iter().chain(&self.fluent_methods) {
            validate(method, false, setting)?;
        }
        for path in &self.constructors {
            validate(path, true, setting)?;
        }
        Ok(())
    }
}
