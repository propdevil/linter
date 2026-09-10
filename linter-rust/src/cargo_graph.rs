use crate::cargo_manifest::Document;
use linter::{Error, Project, Span};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Declared Cargo edges, including graphs that Cargo cannot build.
pub struct CargoGraph {
    pub packages: BTreeMap<PathBuf, CargoPackage>,
}
pub struct CargoPackage {
    pub name: String,
    pub directory: PathBuf,
    pub manifest: PathBuf,
    pub text: String,
    pub dependencies: Vec<CargoDependency>,
}
pub struct CargoDependency {
    pub alias: String,
    pub package: String,
    pub requirement: Option<String>,
    pub source: Option<String>,
    pub manifest: Option<PathBuf>,
    pub kind: DependencyKind,
    pub target: Option<String>,
    pub optional: bool,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Normal,
    Build,
    Development,
}
impl linter::Analysis for CargoGraph {
    fn load(project: &Project) -> Result<Self, Error> {
        let root = fs::canonicalize(project.root()).map_err(|e| Error::Analysis(e.to_string()))?;
        let mut documents = BTreeMap::new();
        for entry in project
            .entries()
            .filter(|e| e.kind.is_file() && e.path.file_name().is_some_and(|n| n == "Cargo.toml"))
        {
            let path = root.join(&entry.path);
            let text = fs::read_to_string(&path).map_err(|e| failure(&entry.path, e))?;
            let document: Document = toml::from_str(&text).map_err(|e| failure(&entry.path, e))?;
            for (_, _, _, dependency) in document.edges() {
                dependency
                    .get_ref()
                    .validate()
                    .map_err(|e| failure(&entry.path, e))?;
            }
            if let Some(workspace) = &document.workspace {
                for dependency in workspace.dependencies.values() {
                    let specification = dependency.get_ref();
                    specification
                        .validate()
                        .map_err(|e| failure(&entry.path, e))?;
                    if specification.inherited()? || specification.optional() {
                        return Err(failure(
                            &entry.path,
                            "workspace dependencies cannot inherit or be optional",
                        ));
                    }
                }
            }
            documents.insert(path, (text, document));
        }
        let mut packages = BTreeMap::new();
        for (path, (text, document)) in &documents {
            let Some(package) = &document.package else {
                continue;
            };
            if package.name.trim().is_empty() {
                return Err(failure(path, "package name must not be empty"));
            }
            let workspace = workspace(path, document, &documents)?;
            let mut dependencies = Vec::new();
            for (kind, target, alias, spec) in document.edges() {
                let span = Span::new(text, spec.span());
                let mut specification = spec.get_ref();
                let mut base = path.parent().unwrap_or(&root);
                if specification.inherited()? {
                    let (workspace_path, workspace_document) = workspace.ok_or_else(|| {
                        failure(path, "inherited dependency has no discovered workspace")
                    })?;
                    specification = workspace_document
                        .workspace
                        .as_ref()
                        .and_then(|w| w.dependencies.get(alias))
                        .map(|v| v.get_ref())
                        .ok_or_else(|| {
                            failure(path, format!("workspace dependency {alias:?} is missing"))
                        })?;
                    if specification.inherited()? {
                        return Err(failure(
                            path,
                            "workspace dependencies cannot inherit themselves",
                        ));
                    }
                    base = workspace_path.parent().unwrap_or(&root);
                }
                specification.validate().map_err(|e| failure(path, e))?;
                let resolved = specification
                    .path()
                    .map(|dependency| {
                        fs::canonicalize(base.join(dependency).join("Cargo.toml"))
                            .map_err(|e| failure(path, e))
                    })
                    .transpose()?;
                let package_name = specification.package().unwrap_or(alias).to_owned();
                if let Some(target_path) = &resolved
                    && let Some((_, target_document)) = documents.get(target_path)
                {
                    let actual = target_document.package.as_ref().ok_or_else(|| {
                        failure(path, "path dependency points at a virtual manifest")
                    })?;
                    if actual.name != package_name {
                        return Err(failure(
                            path,
                            format!(
                                "dependency {alias:?} expects {package_name:?}, found {:?}",
                                actual.name
                            ),
                        ));
                    }
                }
                dependencies.push(CargoDependency {
                    alias: alias.into(),
                    package: package_name,
                    requirement: specification.version().map(str::to_owned),
                    source: resolved
                        .as_ref()
                        .map(|path| format!("path:{}", path.display()))
                        .or_else(|| specification.source()),
                    manifest: resolved
                        .filter(|p| documents.get(p).is_some_and(|(_, d)| d.package.is_some()))
                        .map(|p| p.strip_prefix(&root).unwrap_or(&p).to_owned()),
                    kind,
                    target: target.map(str::to_owned),
                    optional: spec.get_ref().optional(),
                    span,
                });
            }
            let manifest = path
                .strip_prefix(&root)
                .map_err(|e| failure(path, e))?
                .to_owned();
            let directory = manifest
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned();
            packages.insert(
                manifest.clone(),
                CargoPackage {
                    name: package.name.clone(),
                    directory,
                    manifest,
                    text: text.clone(),
                    dependencies,
                },
            );
        }
        Ok(Self { packages })
    }
}
fn workspace<'a>(
    path: &Path,
    document: &Document,
    documents: &'a BTreeMap<PathBuf, (String, Document)>,
) -> Result<Option<(&'a PathBuf, &'a Document)>, Error> {
    if let Some(explicit) = document.package.as_ref().and_then(|p| p.workspace.as_ref()) {
        let target = fs::canonicalize(
            path.parent()
                .unwrap_or(Path::new("."))
                .join(explicit)
                .join("Cargo.toml"),
        )
        .map_err(|e| failure(path, e))?;
        return documents
            .get_key_value(&target)
            .filter(|(_, (_, d))| d.workspace.is_some())
            .map(|(p, (_, d))| Some((p, d)))
            .ok_or_else(|| failure(path, "explicit workspace manifest was not discovered"));
    }
    Ok(documents
        .iter()
        .filter(|(p, (_, d))| {
            d.workspace.is_some() && path.starts_with(p.parent().unwrap_or(Path::new(".")))
        })
        .max_by_key(|(p, _)| p.components().count())
        .map(|(p, (_, d))| (p, d)))
}
fn failure(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Analysis(format!("{}: {error}", path.display()))
}
