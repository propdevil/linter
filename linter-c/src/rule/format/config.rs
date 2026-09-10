use linter::{Error, Selector, Target};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Definition>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: Target,
    exclude: Option<Target>,
    executable: PathBuf,
    style: String,
    fallback_style: String,
    #[serde(default = "timeout")]
    timeout_ms: u64,
    #[serde(default = "output_limit")]
    max_output_bytes: u64,
}

fn timeout() -> u64 {
    30_000
}
fn output_limit() -> u64 {
    1_048_576
}

pub(super) struct Assertion {
    pub target: Selector,
    pub exclude: Option<Selector>,
    pub executable: PathBuf,
    pub style: String,
    pub fallback_style: String,
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
    pub setting: String,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0.into_iter().enumerate().map(|(index, value)| {
            let setting = format!("rules.\"c/format\"[{index}]");
            if value.executable.as_os_str().is_empty() || value.style.trim().is_empty()
                || value.fallback_style.trim().is_empty()
                || value.timeout_ms == 0 || value.max_output_bytes == 0 {
                return Err(Error::Configuration(format!(
                    "{setting}: executable, style and fallback_style are required; limits must be positive"
                )));
            }
            Ok(Assertion {
                target: value.target.compile(&format!("{setting}.target"), true)?,
                exclude: value.exclude.map(|target| {
                    target.compile(&format!("{setting}.exclude"), true)
                }).transpose()?,
                executable: value.executable, style: value.style,
                fallback_style: value.fallback_style,
                timeout_ms: value.timeout_ms, max_output_bytes: value.max_output_bytes, setting,
            })
        }).collect()
    }
}
