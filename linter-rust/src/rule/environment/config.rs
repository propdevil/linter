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
            .map(|(index, definition)| definition.compile(index))
            .collect()
    }
}

impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let setting = format!("rules.\"rust/environment-variable-access\"[{index}]");
        self.validate(&setting)?;
        let mut modules = Vec::new();
        for module in self.allowed_modules {
            let parts: Vec<String> = module.split("::").map(str::to_owned).collect();
            if parts.iter().any(|part| !identifier(part)) {
                return Err(Error::Configuration(format!(
                    "{setting}.allowed_modules: expected module paths like platform::ffi"
                )));
            }
            modules.push(parts);
        }
        Ok(Assertion {
            functions: self.functions,
            macros: self.macros,
            global_types: self.global_types,
            global_words: self.global_words,
            target: self.target.compile(&format!("{setting}.target"), true)?,
            exclude: self
                .exclude
                .map(|value| value.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            scope: self.scope,
            allowed_targets: self
                .allowed_targets
                .map(|target| target.compile(&format!("{setting}.allowed_targets"), true))
                .transpose()?,
            allowed_modules: modules,
            setting,
        })
    }

    fn validate(&self, setting: &str) -> Result<(), Error> {
        if self.functions.is_empty() {
            return Err(Error::Configuration(format!(
                "{setting}.functions cannot be empty"
            )));
        }
        if self.global_types.is_empty() != self.global_words.is_empty() {
            return Err(Error::Configuration(format!(
                "{setting}: global_types and global_words must be configured together"
            )));
        }
        for value in self
            .functions
            .iter()
            .chain(&self.macros)
            .chain(&self.global_types)
            .chain(&self.global_words)
        {
            if value.split("::").any(|part| !identifier(part)) {
                return Err(Error::Configuration(format!(
                    "{setting}: invalid API or word '{value}'"
                )));
            }
        }
        if self
            .global_words
            .iter()
            .any(|word| !word.bytes().all(|byte| byte.is_ascii_lowercase()))
        {
            return Err(Error::Configuration(format!(
                "{setting}.global_words must be lowercase words"
            )));
        }
        Ok(())
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}
