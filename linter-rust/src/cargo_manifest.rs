use crate::DependencyKind;
use serde::Deserialize;
use std::collections::BTreeMap;
type Dependencies = BTreeMap<String, toml::Spanned<Specification>>;
#[derive(Deserialize)]
pub(crate) struct Document {
    pub package: Option<Package>,
    pub workspace: Option<Workspace>,
    #[serde(default)]
    dependencies: Dependencies,
    #[serde(default, rename = "build-dependencies")]
    build: Dependencies,
    #[serde(default, rename = "dev-dependencies")]
    development: Dependencies,
    #[serde(default)]
    target: BTreeMap<String, Target>,
}
#[derive(Deserialize)]
pub(crate) struct Package {
    pub name: String,
    pub workspace: Option<String>,
}
#[derive(Deserialize)]
pub(crate) struct Workspace {
    #[serde(default)]
    pub dependencies: Dependencies,
}
#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    dependencies: Dependencies,
    #[serde(default, rename = "build-dependencies")]
    build: Dependencies,
    #[serde(default, rename = "dev-dependencies")]
    development: Dependencies,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum Specification {
    Version(String),
    Detailed(BTreeMap<String, toml::Value>),
}
impl Specification {
    fn value(&self, key: &str) -> Option<&toml::Value> {
        match self {
            Self::Version(_) => None,
            Self::Detailed(values) => values.get(key),
        }
    }
    pub fn path(&self) -> Option<&str> {
        self.value("path").and_then(toml::Value::as_str)
    }
    pub fn package(&self) -> Option<&str> {
        self.value("package").and_then(toml::Value::as_str)
    }
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Version(v) => Some(v),
            Self::Detailed(_) => self.value("version").and_then(toml::Value::as_str),
        }
    }
    pub fn source(&self) -> Option<String> {
        if self.path().is_some() {
            return None;
        }
        if let Some(git) = self.value("git").and_then(toml::Value::as_str) {
            let mut source = format!("git:{git}");
            for key in ["branch", "tag", "rev"] {
                if let Some(value) = self.value(key).and_then(toml::Value::as_str) {
                    source.push_str(&format!(";{key}={value}"));
                }
            }
            return Some(source);
        }
        Some(format!(
            "registry:{}",
            self.value("registry")
                .and_then(toml::Value::as_str)
                .unwrap_or("crates-io")
        ))
    }
    pub fn optional(&self) -> bool {
        self.value("optional").and_then(toml::Value::as_bool) == Some(true)
    }
    pub fn inherited(&self) -> Result<bool, linter::Error> {
        self.validate().map_err(linter::Error::Analysis)?;
        Ok(self.value("workspace").and_then(toml::Value::as_bool) == Some(true))
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(version) = self.version() {
            cargo_metadata::semver::VersionReq::parse(version)
                .map_err(|e| format!("invalid dependency version: {e}"))?;
        }
        if let Self::Version(version) = self {
            return if version.trim().is_empty() {
                Err("empty dependency version".into())
            } else {
                Ok(())
            };
        }
        let Self::Detailed(values) = self else {
            return Ok(());
        };
        for (key, value) in values {
            let valid = match key.as_str() {
                "path" | "package" | "version" | "git" | "branch" | "tag" | "rev" | "registry"
                | "target" => value.as_str().is_some_and(|v| !v.trim().is_empty()),
                "workspace" | "optional" | "default-features" | "public" | "lib" => value.is_bool(),
                "features" => value
                    .as_array()
                    .is_some_and(|a| a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty()))),
                "artifact" => {
                    value.is_str()
                        || value
                            .as_array()
                            .is_some_and(|a| a.iter().all(toml::Value::is_str))
                }
                _ => false,
            };
            if !valid {
                return Err(format!(
                    "unsupported or invalid Cargo dependency option {key:?}"
                ));
            }
        }
        if self.value("workspace").is_some() {
            if self.value("workspace").and_then(toml::Value::as_bool) != Some(true) {
                return Err("workspace inheritance must be true".into());
            }
            if values.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "workspace" | "optional" | "features" | "default-features" | "public"
                )
            }) {
                return Err("inherited dependency overrides its source".into());
            }
        } else if !["path", "git", "version"]
            .iter()
            .any(|k| values.contains_key(*k))
        {
            return Err("dependency requires path, git, version, or workspace=true".into());
        }
        if self.path().is_some() && self.value("git").is_some() {
            return Err("dependency cannot specify path and git".into());
        }
        Ok(())
    }
}
impl Document {
    pub fn edges(
        &self,
    ) -> Vec<(
        DependencyKind,
        Option<&str>,
        &str,
        &toml::Spanned<Specification>,
    )> {
        let mut output = Vec::new();
        for (kind, dependencies) in [
            (DependencyKind::Normal, &self.dependencies),
            (DependencyKind::Build, &self.build),
            (DependencyKind::Development, &self.development),
        ] {
            output.extend(
                dependencies
                    .iter()
                    .map(|(alias, spec)| (kind, None, alias.as_str(), spec)),
            );
        }
        for (target, tables) in &self.target {
            for (kind, dependencies) in [
                (DependencyKind::Normal, &tables.dependencies),
                (DependencyKind::Build, &tables.build),
                (DependencyKind::Development, &tables.development),
            ] {
                output.extend(
                    dependencies
                        .iter()
                        .map(|(alias, spec)| (kind, Some(target.as_str()), alias.as_str(), spec)),
                );
            }
        }
        output
    }
}
