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
    compilation_database: PathBuf,
    checks: String,
    #[serde(default)]
    extra_args: Vec<String>,
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
    pub database: PathBuf,
    pub checks: String,
    pub extra_args: Vec<String>,
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
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
        let value = self;
        let setting = format!("rules.\"c/tidy\"[{index}]");
        if value.executable.as_os_str().is_empty()
            || value.compilation_database.as_os_str().is_empty()
            || value.checks.trim().is_empty()
            || value.timeout_ms == 0
            || value.max_output_bytes == 0
            || value
                .extra_args
                .iter()
                .any(|value| value.is_empty() || value.contains('\0'))
        {
            return Err(Error::Configuration(format!(
                "{setting}: invalid tool settings"
            )));
        }
        Ok(Assertion {
            target: value.target.compile(&format!("{setting}.target"), true)?,
            exclude: value
                .exclude
                .map(|target| target.compile(&format!("{setting}.exclude"), true))
                .transpose()?,
            executable: value.executable,
            database: value.compilation_database,
            checks: value.checks,
            extra_args: value.extra_args,
            timeout_ms: value.timeout_ms,
            max_output_bytes: value.max_output_bytes,
            setting,
        })
    }
}
