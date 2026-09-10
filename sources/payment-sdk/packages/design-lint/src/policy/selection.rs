use super::Policy;
use crate::{
    LintError, Result,
    source::{SourceFile, Workspace},
};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

/// Original SDK rules always run. Additional rules are selected explicitly.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default)]
    pub enabled: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rust {
    #[serde(default = "forbidden_modules")]
    pub forbidden_modules: Vec<String>,
}
impl Default for Rust {
    fn default() -> Self {
        Self {
            forbidden_modules: forbidden_modules(),
        }
    }
}
fn forbidden_modules() -> Vec<String> {
    [
        "common", "core", "shared", "util", "utils", "helper", "helpers", "misc",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundaries {
    #[serde(default)]
    pub environment: Vec<SourceSelector>,
    #[serde(default)]
    pub process: Vec<SourceSelector>,
}

/// Relative paths match complete components; optional package and path intersect.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSelector {
    pub package: Option<String>,
    pub path: Option<String>,
}
impl SourceSelector {
    pub fn matches(&self, source: &SourceFile, workspace: &Workspace) -> bool {
        (self.package.is_some() || self.path.is_some())
            && self
                .package
                .as_ref()
                .is_none_or(|name| name == &source.package)
            && self.path.as_ref().is_none_or(|path| {
                workspace
                    .policy_roots
                    .iter()
                    .any(|root| source.path.starts_with(root.join(path)))
            })
    }
    fn validate(&self) -> Result<()> {
        if self.package.is_none() && self.path.is_none() {
            return Err(LintError::configuration(
                "boundary selector requires package or path",
            ));
        }
        if self
            .package
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(LintError::configuration("boundary package cannot be empty"));
        }
        if let Some(path) = &self.path {
            relative(path)?;
        }
        Ok(())
    }
}
fn relative(value: &str) -> Result<()> {
    if value.is_empty()
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(LintError::configuration(format!(
            "expected a nonempty relative path, got `{value}`"
        )));
    }
    Ok(())
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        let mut layers = BTreeSet::new();
        for layer in &self.dependency.layers {
            if layer.name.is_empty() || !layers.insert(&layer.name) {
                return Err(LintError::configuration(format!(
                    "empty or duplicate layer `{}`",
                    layer.name
                )));
            }
            if let Some(directory) = &layer.directory {
                relative(directory)?;
            }
        }
        for layer in &self.dependency.layers {
            for target in &layer.may_depend_on {
                if !layers.contains(target) {
                    return Err(LintError::configuration(format!(
                        "unknown layer `{target}`"
                    )));
                }
            }
        }
        let mut packages = BTreeSet::new();
        for mapping in &self.dependency.package_layers {
            if !layers.contains(&mapping.layer) || !packages.insert(&mapping.package) {
                return Err(LintError::configuration(format!(
                    "invalid or duplicate mapping for `{}`",
                    mapping.package
                )));
            }
        }
        for selector in self
            .boundaries
            .environment
            .iter()
            .chain(&self.boundaries.process)
        {
            selector.validate()?;
        }
        let mut enabled = BTreeSet::new();
        for id in &self.rules.enabled {
            if !enabled.insert(id) || !crate::rule::adopted::contains(id) {
                return Err(LintError::configuration(format!(
                    "unknown or duplicate adopted rule `{id}`"
                )));
            }
        }
        Ok(())
    }
}
