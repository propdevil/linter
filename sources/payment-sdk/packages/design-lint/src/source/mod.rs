use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::Span;
mod exception;
mod package;
mod scope;
#[cfg(test)]
mod tests;
pub use scope::snake_case;

use crate::{LintError, Location, Result, policy::Source as SourcePolicy};

pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    pub syntax: syn::File,
    pub package: String,
    pub domain: String,
    pub test: bool,
    exceptions: Vec<(usize, String)>,
}

pub use package::Package;

pub struct Workspace {
    pub roots: Vec<PathBuf>,
    pub(crate) policy_roots: Vec<PathBuf>,
    pub sources: Vec<SourceFile>,
    pub packages: Vec<Package>,
    pub directories: Vec<PathBuf>,
}

impl Workspace {
    pub fn production(&self) -> impl Iterator<Item = &SourceFile> {
        self.sources.iter().filter(|source| !source.test)
    }

    pub fn sources(&self) -> &[SourceFile] {
        &self.sources
    }
    pub fn paths(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn load(paths: Vec<PathBuf>, policy: &SourcePolicy) -> Result<Self> {
        let mut files = BTreeSet::new();
        let mut directories = BTreeSet::new();
        let mut roots = Vec::new();
        for path in paths {
            let canonical =
                fs::canonicalize(&path).map_err(|error| LintError::io("open", &path, error))?;
            let root = if canonical.is_file() {
                canonical.parent().unwrap_or(&canonical).to_owned()
            } else {
                canonical.clone()
            };
            roots.push(root);
            collect(&canonical, policy, &mut files, &mut directories)?;
        }
        let manifests = package::manifests(&files)?;
        let packages = package::inventory(&manifests)?;
        let policy_roots = roots
            .iter()
            .map(|root| {
                root.ancestors()
                    .find(|ancestor| {
                        manifests
                            .get(*ancestor)
                            .is_some_and(|manifest| manifest.get("workspace").is_some())
                    })
                    .or_else(|| {
                        root.ancestors()
                            .find(|ancestor| manifests.contains_key(*ancestor))
                    })
                    .unwrap_or(root)
                    .to_owned()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut sources = Vec::new();
        for path in files {
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let owner = packages
                .iter()
                .filter(|package| path.starts_with(&package.root))
                .max_by_key(|package| package.root.components().count());
            if owner.is_some_and(|package| policy.self_packages.contains(&package.name)) {
                continue;
            }
            let text =
                fs::read_to_string(&path).map_err(|error| LintError::io("read", &path, error))?;
            let syntax = syn::parse_file(&text).map_err(|source| LintError::Parse {
                path: path.clone(),
                source,
            })?;
            let package = owner
                .map_or("unknown-package", |owner| owner.name.as_str())
                .to_owned();
            let domain = scope::domain(&path, owner);
            let test = scope::test_path(&path) || crate::rule::production::test_only(&syntax.attrs);
            let exceptions = exception::collect(&text, &syntax);
            sources.push(SourceFile {
                path,
                text,
                syntax,
                package,
                domain,
                test,
                exceptions,
            });
        }
        scope::classify(&mut sources);
        Ok(Self {
            roots,
            policy_roots,
            sources,
            packages,
            directories: directories.into_iter().collect(),
        })
    }
}

impl SourceFile {
    pub fn excerpt(&self, span: Span) -> String {
        let start = span.start();
        let end = span.end();
        self.text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let number = index + 1;
                if number < start.line || number > end.line {
                    return None;
                }
                let from = if number == start.line {
                    start.column
                } else {
                    0
                };
                let to = if number == end.line {
                    end.column
                } else {
                    line.chars().count()
                };
                Some(
                    line.chars()
                        .skip(from)
                        .take(to.saturating_sub(from))
                        .collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn location(&self, span: Span) -> Location {
        let start = span.start();
        let source = self
            .text
            .lines()
            .nth(start.line.saturating_sub(1))
            .unwrap_or_default()
            .trim()
            .to_owned();
        Location {
            path: self.path.clone(),
            line: start.line.max(1),
            column: start.column + 1,
            source,
        }
    }

    pub fn suppressed(&self, rule: &str, line: usize) -> bool {
        self.exceptions
            .iter()
            .any(|(comment, id)| id == rule && *comment <= line && line - comment <= 2)
    }
}

fn collect(
    path: &Path,
    policy: &SourcePolicy,
    files: &mut BTreeSet<PathBuf>,
    directories: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if path.is_symlink() {
        return Ok(());
    }
    if path.is_file() {
        files.insert(path.to_owned());
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if policy
        .ignored_directories
        .iter()
        .any(|ignored| ignored == name)
    {
        return Ok(());
    }
    directories.insert(path.to_owned());
    let entries = fs::read_dir(path).map_err(|error| LintError::io("read", path, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| LintError::io("read", path, error))?;
        collect(&entry.path(), policy, files, directories)?;
    }
    Ok(())
}

pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
