use linter::{Error, Selector, Target};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    functions: Vec<String>,
    #[serde(default)]
    macros: Vec<String>,
    #[serde(default)]
    global_types: Vec<String>,
    #[serde(default)]
    global_words: Vec<String>,
    #[serde(default)]
    exclude: Option<Target>,
    #[serde(default)]
    scope: Scope,
    allowed_targets: Option<Target>,
    #[serde(default)]
    allowed_modules: Vec<String>,
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
    pub functions: Vec<String>,
    pub macros: Vec<String>,
    pub global_types: Vec<String>,
    pub global_words: Vec<String>,
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub scope: Scope,
    pub allowed_targets: Option<Selector>,
    pub allowed_modules: Vec<Vec<String>>,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, definition)| {
                let setting = format!("rules.\"rust/environment-variable-access\"[{index}]");
                if definition.functions.is_empty() {
                    return Err(Error::Configuration(format!(
                        "{setting}.functions cannot be empty"
                    )));
                }
                if definition.global_types.is_empty() != definition.global_words.is_empty() {
                    return Err(Error::Configuration(format!(
                        "{setting}: global_types and global_words must be configured together"
                    )));
                }
                for value in definition
                    .functions
                    .iter()
                    .chain(&definition.macros)
                    .chain(&definition.global_types)
                    .chain(&definition.global_words)
                {
                    if value.split("::").any(|part| {
                        part.is_empty()
                            || !part.bytes().enumerate().all(|(index, byte)| {
                                byte == b'_'
                                    || byte.is_ascii_alphabetic()
                                    || (index > 0 && byte.is_ascii_digit())
                            })
                    }) {
                        return Err(Error::Configuration(format!(
                            "{setting}: invalid API or word '{value}'"
                        )));
                    }
                }
                if definition
                    .global_words
                    .iter()
                    .any(|word| !word.bytes().all(|byte| byte.is_ascii_lowercase()))
                {
                    return Err(Error::Configuration(format!(
                        "{setting}.global_words must be lowercase words"
                    )));
                }
                let mut modules = Vec::new();
                for module in definition.allowed_modules {
                    let parts: Vec<String> = module.split("::").map(str::to_owned).collect();
                    if parts.iter().any(|part| {
                        part.is_empty()
                            || !part.bytes().enumerate().all(|(index, byte)| {
                                byte == b'_'
                                    || byte.is_ascii_alphabetic()
                                    || (index > 0 && byte.is_ascii_digit())
                            })
                    }) {
                        return Err(Error::Configuration(format!(
                            "{setting}.allowed_modules: expected module paths like platform::ffi"
                        )));
                    }
                    modules.push(parts);
                }
                Ok(Assertion {
                    functions: definition.functions,
                    macros: definition.macros,
                    global_types: definition.global_types,
                    global_words: definition.global_words,
                    target: definition
                        .target
                        .compile(&format!("{setting}.target"), true)?,
                    exclude: definition
                        .exclude
                        .map(|value| value.compile(&format!("{setting}.exclude"), true))
                        .transpose()?,
                    scope: definition.scope,
                    allowed_targets: definition
                        .allowed_targets
                        .map(|target| target.compile(&format!("{setting}.allowed_targets"), true))
                        .transpose()?,
                    allowed_modules: modules,
                    setting,
                })
            })
            .collect()
    }
}
