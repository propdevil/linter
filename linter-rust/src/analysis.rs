use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use cargo_metadata::{MetadataCommand, Package};
use linter::{Error, Project};
use tree_sitter::{Parser, Tree};

/// Rust syntax and Cargo declarations for one validation run.
pub struct Analysis {
    pub sources: Vec<Source>,
    pub packages: BTreeMap<PathBuf, Package>,
}

pub struct Source {
    pub path: PathBuf,
    pub text: String,
    pub syntax: Tree,
}

impl linter::Analysis for Analysis {
    fn directives(&self) -> Vec<linter::Directive> {
        self.sources
            .iter()
            .flat_map(crate::directive::collect)
            .collect()
    }

    fn load(project: &Project) -> Result<Self, Error> {
        let root =
            fs::canonicalize(project.root()).map_err(|error| Error::Analysis(error.to_string()))?;
        let packages = Self::packages(project, &root)?;
        let sources = Self::sources(project, &root)?;
        Ok(Self { sources, packages })
    }
}

impl Analysis {
    fn packages(
        project: &Project,
        root: &std::path::Path,
    ) -> Result<BTreeMap<PathBuf, Package>, Error> {
        let manifests: BTreeSet<_> = project
            .entries()
            .filter(|entry| {
                entry.kind.is_file()
                    && entry
                        .path
                        .file_name()
                        .is_some_and(|name| name == "Cargo.toml")
            })
            .map(|entry| root.join(&entry.path))
            .collect();
        let mut packages = BTreeMap::new();
        let mut visited = BTreeSet::new();
        for manifest in &manifests {
            if visited.contains(manifest) {
                continue;
            }
            let metadata = MetadataCommand::new()
                .manifest_path(manifest)
                .no_deps()
                .other_options(vec!["--offline".into()])
                .exec()
                .map_err(|error| Error::Analysis(format!("{}: {error}", manifest.display())))?;
            visited.insert(
                metadata
                    .workspace_root
                    .join("Cargo.toml")
                    .into_std_path_buf(),
            );
            for package in metadata.packages {
                let manifest = package.manifest_path.clone().into_std_path_buf();
                visited.insert(manifest.clone());
                if !manifests.contains(&manifest) {
                    continue;
                }
                packages.insert(manifest, package);
            }
        }
        Ok(packages)
    }
    fn sources(project: &Project, root: &std::path::Path) -> Result<Vec<Source>, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|error| Error::Analysis(error.to_string()))?;
        let mut sources = Vec::new();
        for entry in project.entries().filter(|entry| {
            entry.kind.is_file()
                && entry
                    .path
                    .extension()
                    .is_some_and(|extension| extension == "rs")
        }) {
            let text = fs::read_to_string(root.join(&entry.path))
                .map_err(|error| Error::Analysis(format!("{}: {error}", entry.path.display())))?;
            let syntax = parser.parse(&text, None).ok_or_else(|| {
                Error::Analysis(format!("{}: Rust parsing failed", entry.path.display()))
            })?;
            if syntax.root_node().has_error() {
                return Err(Error::Analysis(format!(
                    "{}: Rust syntax contains parsing errors",
                    entry.path.display()
                )));
            }
            sources.push(Source {
                path: entry.path.clone(),
                text,
                syntax,
            });
        }
        Ok(sources)
    }
}
