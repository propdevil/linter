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
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/async-blocking-operation\"[{index}]");
                if definition.blocking_functions.is_empty()
                    && definition.blocking_methods.is_empty()
                {
                    return Err(Error::Configuration(format!(
                        "{setting}: configure blocking_functions or blocking_methods"
                    )));
                }
                for path in definition
                    .blocking_functions
                    .iter()
                    .chain(&definition.blocking_contexts)
                {
                    validate(path, true, &setting)?;
                }
                for method in &definition.guard_adapters {
                    validate(method, false, &setting)?;
                }
                for policy in &definition.blocking_methods {
                    validate(&policy.receiver, true, &setting)?;
                    if policy.methods.is_empty() {
                        return Err(Error::Configuration(format!(
                            "{setting}.blocking_methods: methods cannot be empty"
                        )));
                    }
                    for method in policy.methods.iter().chain(&policy.fluent_methods) {
                        validate(method, false, &setting)?;
                    }
                    for path in &policy.constructors {
                        validate(path, true, &setting)?;
                    }
                }
                Ok(Assertion {
                    target: definition
                        .target
                        .compile(&format!("{setting}.target"), true)?,
                    exclude: definition
                        .exclude
                        .map(|value| value.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    allowed: definition
                        .allowed_targets
                        .map(|value| value.compile(&format!("{setting}.allowed_targets"), true))
                        .transpose()?,
                    scope: definition.scope,
                    functions: definition.blocking_functions.into_iter().collect(),
                    methods: definition.blocking_methods,
                    contexts: definition.blocking_contexts.into_iter().collect(),
                    adapters: definition.guard_adapters.into_iter().collect(),
                    setting,
                })
            })
            .collect()
    }
}
fn validate(value: &str, path: bool, setting: &str) -> Result<(), Error> {
    if (!path && value.contains("::"))
        || value.split("::").any(|word| {
            word.is_empty()
                || !word.bytes().enumerate().all(|(index, byte)| {
                    byte == b'_'
                        || byte.is_ascii_alphabetic()
                        || (index > 0 && byte.is_ascii_digit())
                })
        })
    {
        return Err(Error::Configuration(format!(
            "{setting}: invalid API identifier '{value}'"
        )));
    }
    Ok(())
}
