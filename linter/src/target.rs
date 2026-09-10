use std::path::Path;

use crate::{Error, config::matcher};
use globset::GlobMatcher;
use serde::Deserialize;

/// Root-relative selectors; a string or a nonempty list of strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Target {
    One(String),
    Many(Vec<String>),
}

impl From<&str> for Target {
    fn from(value: &str) -> Self {
        Self::One(value.into())
    }
}

pub struct Selector(Vec<GlobMatcher>);

impl Target {
    pub fn compile(self, setting: &str, case_sensitive: bool) -> Result<Selector, Error> {
        let patterns = match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        };
        if patterns.is_empty() {
            return Err(Error::Configuration(format!(
                "{setting}: target cannot be empty"
            )));
        }
        Ok(Selector(
            patterns
                .iter()
                .map(|pattern| matcher(pattern, setting, case_sensitive))
                .collect::<Result<_, _>>()?,
        ))
    }
}

impl Selector {
    pub fn matches(&self, path: &Path) -> bool {
        self.0.iter().any(|pattern| pattern.is_match(path))
    }
}
