mod selection;
pub use selection::{Boundaries, Rules, Rust, SourceSelector};

use std::{fs, path::Path};

use serde::Deserialize;

use crate::{LintError, Result};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub rules: Rules,
    #[serde(default)]
    pub rust: Rust,
    #[serde(default)]
    pub boundaries: Boundaries,
    #[serde(default)]
    pub dependency: Dependency,
    #[serde(default)]
    pub vocabulary: Vocabulary,
    #[serde(default)]
    pub repository: Repository,
    #[serde(default)]
    pub source: Source,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    #[serde(default)]
    pub ignored_packages: Vec<String>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub package_layers: Vec<PackageLayer>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    pub directory: Option<String>,
    #[serde(default)]
    pub may_depend_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageLayer {
    pub package: String,
    pub layer: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vocabulary {
    #[serde(default)]
    pub owners: Vec<VocabularyOwner>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabularyOwner {
    pub words: Vec<String>,
    pub allowed_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    #[serde(default = "default_lines")]
    pub maximum_rust_lines: usize,
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default = "default_chain_root")]
    pub chain_root: String,
    #[serde(default = "default_chain_exclusions")]
    pub chain_exclusions: Vec<String>,
    #[serde(default = "default_chain_skeleton")]
    pub chain_skeleton: Vec<String>,
    #[serde(default = "default_chain_directories")]
    pub chain_directories: Vec<String>,
}

impl Default for Repository {
    fn default() -> Self {
        Self {
            maximum_rust_lines: default_lines(),
            forbidden_paths: Vec::new(),
            chain_root: default_chain_root(),
            chain_exclusions: default_chain_exclusions(),
            chain_skeleton: default_chain_skeleton(),
            chain_directories: default_chain_directories(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(default = "default_ignored")]
    pub ignored_directories: Vec<String>,
    #[serde(default)]
    pub self_packages: Vec<String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            ignored_directories: default_ignored(),
            self_packages: Vec::new(),
        }
    }
}

fn default_lines() -> usize {
    500
}
fn default_chain_root() -> String {
    "sdk/chains".to_owned()
}
fn default_chain_exclusions() -> Vec<String> {
    vec!["base".to_owned()]
}
fn default_chain_skeleton() -> Vec<String> {
    [
        "src/lib.rs",
        "src/address.rs",
        "src/batch.rs",
        "src/error.rs",
        "src/indexer/mod.rs",
        "src/indexer/source/mod.rs",
        "src/rpc/mod.rs",
        "src/transaction/mod.rs",
        "src/transaction/operations/mod.rs",
        "src/wallet/mod.rs",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn default_chain_directories() -> Vec<String> {
    [
        "src/indexer",
        "src/indexer/source",
        "src/rpc",
        "src/transaction",
        "src/transaction/operations",
        "src/wallet",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn default_ignored() -> Vec<String> {
    [".git", "target", "vendor", "generated", "old", "reference"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

impl Policy {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text =
            fs::read_to_string(path).map_err(|error| LintError::io("read policy", path, error))?;
        let policy: Self = toml::from_str(&text).map_err(|error| {
            LintError::configuration(format!("failed to parse {}: {error}", path.display()))
        })?;
        policy.validate()?;
        Ok(policy)
    }
}

impl Dependency {
    pub fn layer<'a>(&'a self, package: &str, path: &Path) -> Option<&'a Layer> {
        if let Some(mapping) = self
            .package_layers
            .iter()
            .find(|item| item.package == package)
        {
            return self.layers.iter().find(|layer| layer.name == mapping.layer);
        }
        let path = path.to_string_lossy().replace('\\', "/");
        self.layers
            .iter()
            .filter_map(|layer| {
                let directory = layer.directory.as_ref()?;
                let needle = format!("/{directory}/");
                path.contains(&needle).then_some(layer)
            })
            .next()
    }
}
