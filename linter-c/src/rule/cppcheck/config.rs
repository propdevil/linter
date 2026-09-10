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
    checks: Vec<String>,
    standard: String,
    #[serde(default)]
    suppressions: Vec<String>,
    #[serde(default)]
    inconclusive: bool,
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
    pub checks: Vec<String>,
    pub standard: String,
    pub suppressions: Vec<String>,
    pub inconclusive: bool,
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
    pub setting: String,
}
impl Config {
    pub(super) fn compile(self) -> Result<Vec<Assertion>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let setting = format!("rules.\"c/cppcheck\"[{index}]");
                if value.executable.as_os_str().is_empty()
                    || value.compilation_database.as_os_str().is_empty()
                    || value.checks.is_empty()
                    || value.standard.trim().is_empty()
                    || value.timeout_ms == 0
                    || value.max_output_bytes == 0
                    || value
                        .checks
                        .iter()
                        .chain(value.suppressions.iter())
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
                    standard: value.standard,
                    suppressions: value.suppressions,
                    inconclusive: value.inconclusive,
                    timeout_ms: value.timeout_ms,
                    max_output_bytes: value.max_output_bytes,
                    setting,
                })
            })
            .collect()
    }
}
