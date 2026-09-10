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
            document.validate().map_err(|e| failure(&entry.path, e))?;
            documents.insert(path, (text, document));
        }
        let discovery = Discovery { root, documents };
        let packages = discovery.packages()?;
        Ok(Self { packages })
    }
}
struct Discovery {
    root: PathBuf,
    documents: BTreeMap<PathBuf, (String, Document)>,
}
impl Discovery {
    fn packages(&self) -> Result<BTreeMap<PathBuf, CargoPackage>, Error> {
        let mut packages = BTreeMap::new();
        for (path, (text, document)) in &self.documents {
            let Some(package) = &document.package else {
                continue;
            };
            if package.name.trim().is_empty() {
                return Err(failure(path, "package name must not be empty"));
            }
            let dependencies = self.dependencies(path, text, document)?;
            let manifest = path
                .strip_prefix(&self.root)
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
        Ok(packages)
    }
    fn dependencies(
        &self,
        path: &Path,
        text: &str,
        document: &Document,
    ) -> Result<Vec<CargoDependency>, Error> {
        let workspace = workspace(path, document, &self.documents)?;
        let mut dependencies = Vec::new();
        for (kind, target, alias, spec) in document.edges() {
            let (specification, base) =
                self.specification(path, alias, spec.get_ref(), workspace)?;
            let resolved = specification
                .path()
                .map(|dependency| {
                    fs::canonicalize(base.join(dependency).join("Cargo.toml"))
                        .map_err(|e| failure(path, e))
                })
                .transpose()?;
            let package = specification.package().unwrap_or(alias).to_owned();
            self.validate_target(path, alias, &package, resolved.as_deref())?;
            let source = resolved
                .as_ref()
                .map(|p| format!("path:{}", p.display()))
                .or_else(|| specification.source());
            let manifest = resolved
                .filter(|p| {
                    self.documents
                        .get(p)
                        .is_some_and(|(_, d)| d.package.is_some())
                })
                .map(|p| p.strip_prefix(&self.root).unwrap_or(&p).to_owned());
            dependencies.push(CargoDependency {
                alias: alias.into(),
                package,
                requirement: specification.version().map(str::to_owned),
                source,
                manifest,
                kind,
                target: target.map(str::to_owned),
                optional: spec.get_ref().optional(),
                span: Span::new(text, spec.span()),
            });
        }
        Ok(dependencies)
    }
    fn specification<'a>(
        &'a self,
        path: &'a Path,
        alias: &str,
        specification: &'a crate::cargo_manifest::Specification,
        workspace: Option<(&'a PathBuf, &'a Document)>,
    ) -> Result<(&'a crate::cargo_manifest::Specification, &'a Path), Error> {
        if !specification.inherited()? {
            specification.validate().map_err(|e| failure(path, e))?;
            return Ok((specification, path.parent().unwrap_or(&self.root)));
        }
        let (workspace_path, document) = workspace
            .ok_or_else(|| failure(path, "inherited dependency has no discovered workspace"))?;
        let inherited = document
            .workspace
            .as_ref()
            .and_then(|w| w.dependencies.get(alias))
            .map(|v| v.get_ref())
            .ok_or_else(|| failure(path, format!("workspace dependency {alias:?} is missing")))?;
        if inherited.inherited()? {
            return Err(failure(
                path,
                "workspace dependencies cannot inherit themselves",
            ));
        }
        inherited.validate().map_err(|e| failure(path, e))?;
        Ok((inherited, workspace_path.parent().unwrap_or(&self.root)))
    }
    fn validate_target(
        &self,
        path: &Path,
        alias: &str,
        expected: &str,
        target: Option<&Path>,
    ) -> Result<(), Error> {
        let Some((_, document)) = target.and_then(|p| self.documents.get(p)) else {
            return Ok(());
        };
        let actual = document
            .package
            .as_ref()
            .ok_or_else(|| failure(path, "path dependency points at a virtual manifest"))?;
        if actual.name != expected {
            return Err(failure(
                path,
                format!(
                    "dependency {alias:?} expects {expected:?}, found {:?}",
                    actual.name
                ),
            ));
        }
        Ok(())
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
