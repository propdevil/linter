use std::{collections::BTreeMap, fs, path::Path};

use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::Error;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Configuration {
    pub files: Files,
    pub rules: BTreeMap<String, toml::Value>,
}

impl Configuration {
    pub fn load(root: &Path) -> Result<Self, Error> {
        let path = root.join("linter.toml");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(Error::io(path, error)),
        };
        toml::from_str(&text)
            .map_err(|error| Error::Configuration(format!("{}: {error}", path.display())))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Files {
    exclude: Vec<String>,
}

impl Files {
    pub fn compile(&self) -> Result<GlobSet, Error> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.exclude {
            builder.add(glob(pattern, "files.exclude", true)?);
        }
        builder
            .build()
            .map_err(|error| Error::Configuration(error.to_string()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub config: Option<toml::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            config: None,
        }
    }
}

pub(crate) fn matcher(
    pattern: &str,
    setting: &str,
    case_sensitive: bool,
) -> Result<GlobMatcher, Error> {
    Ok(glob(pattern, setting, case_sensitive)?.compile_matcher())
}

fn glob(pattern: &str, setting: &str, case_sensitive: bool) -> Result<globset::Glob, Error> {
    if pattern.is_empty()
        || pattern.starts_with('/')
        || pattern.contains('\\')
        || pattern
            .split('/')
            .any(|part| part == ".." || part.contains(':'))
    {
        return Err(Error::Configuration(format!(
            "{setting}: expected a nonempty root-relative glob, got {pattern:?}"
        )));
    }
    GlobBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .map_err(|error| Error::Configuration(format!("{setting}: {error}")))
}

pub(crate) fn relative_path(value: &str, setting: &str) -> Result<std::path::PathBuf, Error> {
    if value.is_empty()
        || value.contains(['\\', ':'])
        || value.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(Error::Configuration(format!(
            "{setting}: expected a nonempty relative path without parent traversal, got {value:?}"
        )));
    }
    Ok(std::path::PathBuf::from(value))
}

impl Settings {
    pub fn parse(value: toml::Value, id: &str) -> Result<Self, Error> {
        if value.is_array() {
            Ok(Self {
                enabled: true,
                config: Some(value),
            })
        } else {
            value
                .try_into()
                .map_err(|error| Error::Configuration(format!("rules.{id}: {error}")))
        }
    }
}
