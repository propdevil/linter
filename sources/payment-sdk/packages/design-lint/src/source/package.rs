use crate::{LintError, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub struct Package {
    pub name: String,
    pub root: PathBuf,
    /// Local normal, build and development dependency package names.
    pub dependencies: Vec<String>,
    /// Local normal and build edges; Cargo permits development cycles.
    pub build_dependencies: Vec<String>,
}

pub(super) fn manifests(files: &BTreeSet<PathBuf>) -> Result<BTreeMap<PathBuf, toml::Value>> {
    let mut paths = BTreeSet::new();
    for file in files {
        for directory in file.ancestors().skip(1) {
            let path = directory.join("Cargo.toml");
            if path.is_file() {
                paths.insert(path);
            }
        }
    }
    let mut output = BTreeMap::new();
    while !paths.is_empty() {
        for path in std::mem::take(&mut paths) {
            let root = path.parent().unwrap_or(Path::new(".")).to_owned();
            if output.contains_key(&root) {
                continue;
            }
            let text =
                fs::read_to_string(&path).map_err(|error| LintError::io("read", &path, error))?;
            let manifest: toml::Value = toml::from_str(&text).map_err(|error| {
                LintError::configuration(format!("failed to parse {}: {error}", path.display()))
            })?;
            output.insert(root, manifest);
        }
        for (root, manifest) in &output {
            for table in tables(manifest) {
                for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                    let Some(dependencies) = table.get(kind).and_then(toml::Value::as_table) else {
                        continue;
                    };
                    for (alias, entry) in dependencies {
                        if let Some(path) = local_path(alias, entry, root, &output)? {
                            for ancestor in path.ancestors() {
                                let manifest = ancestor.join("Cargo.toml");
                                if !output.contains_key(ancestor) && manifest.is_file() {
                                    paths.insert(manifest);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(output)
}

fn tables(manifest: &toml::Value) -> impl Iterator<Item = &toml::Value> {
    std::iter::once(manifest).chain(
        manifest
            .get("target")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|table| table.values()),
    )
}

pub(super) fn inventory(manifests: &BTreeMap<PathBuf, toml::Value>) -> Result<Vec<Package>> {
    let names = manifests
        .iter()
        .filter_map(|(root, manifest)| {
            Some((
                root.clone(),
                manifest.get("package")?.get("name")?.as_str()?.to_owned(),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut packages = Vec::new();
    for (root, name) in &names {
        let manifest = &manifests[root];
        let mut all = BTreeSet::new();
        let mut build = BTreeSet::new();
        for table in tables(manifest) {
            for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                let Some(dependencies) = table.get(kind).and_then(toml::Value::as_table) else {
                    continue;
                };
                for (alias, entry) in dependencies {
                    if let Some(path) = local_path(alias, entry, root, manifests)?
                        && let Some(target) = names.get(&path)
                    {
                        all.insert(target.clone());
                        if kind != "dev-dependencies" {
                            build.insert(target.clone());
                        }
                    }
                }
            }
        }
        packages.push(Package {
            name: name.clone(),
            root: root.clone(),
            dependencies: all.into_iter().collect(),
            build_dependencies: build.into_iter().collect(),
        });
    }
    Ok(packages)
}

fn local_path(
    alias: &str,
    entry: &toml::Value,
    root: &Path,
    manifests: &BTreeMap<PathBuf, toml::Value>,
) -> Result<Option<PathBuf>> {
    let (base, entry) = if entry.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
        let inherited = root
            .ancestors()
            .find_map(|ancestor| Some((ancestor, manifests.get(ancestor)?.get("workspace")?)))
            .and_then(|(ancestor, workspace)| {
                Some((ancestor, workspace.get("dependencies")?.get(alias)?))
            });
        inherited.ok_or_else(|| {
            LintError::configuration(format!(
                "missing workspace dependency `{alias}` for {}",
                root.display()
            ))
        })?
    } else {
        (root, entry)
    };
    let Some(path) = entry.get("path").and_then(toml::Value::as_str) else {
        return Ok(None);
    };
    let path = base.join(path);
    let canonical = fs::canonicalize(&path)
        .map_err(|error| LintError::io("resolve dependency", &path, error))?;
    Ok(Some(canonical))
}
