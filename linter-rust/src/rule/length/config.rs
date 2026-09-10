use linter::Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub max_lines: usize,
    pub target: linter::Target,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_lines: 500,
            target: "**/*.rs".into(),
        }
    }
}

impl Config {
    pub(super) fn validate(self) -> Result<Self, Error> {
        if self.max_lines == 0 {
            return Err(Error::Configuration(
                "rust/file-length.max_lines: expected a positive integer".into(),
            ));
        }
        Ok(self)
    }
}
