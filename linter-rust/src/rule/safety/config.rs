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
            .map(|(index, value)| value.compile(index))
            .collect()
    }
}

impl Definition {
    fn compile(self, index: usize) -> Result<Assertion, Error> {
        let definition = self;
        let setting = format!("rules.\"rust/unsafe-boundary\"[{index}]");
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
    }
}
