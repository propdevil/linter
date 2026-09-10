use linter::{Error, Project};
use std::{fs, path::PathBuf};
use tree_sitter::Tree;

pub struct Source {
    pub path: PathBuf,
    pub text: String,
    pub syntax: Tree,
}

/// C syntax prepared once per validation run.
pub struct Analysis {
    pub sources: Vec<Source>,
}

impl linter::Analysis for Analysis {
    fn directives(&self) -> Vec<linter::Directive> {
        self.sources
            .iter()
            .flat_map(crate::directive::collect)
            .collect()
    }

    fn load(project: &Project) -> Result<Self, Error> {
        let mut sources = Vec::new();
        for entry in project.entries().filter(|entry| {
            entry.kind.is_file()
                && entry
                    .path
                    .extension()
                    .is_some_and(|extension| extension == "c" || extension == "h")
        }) {
            let text = fs::read_to_string(project.root().join(&entry.path))
                .map_err(|error| Error::Analysis(format!("{}: {error}", entry.path.display())))?;
            let syntax = super::syntax::parse(&entry.path, &text)?;
            sources.push(Source {
                path: entry.path.clone(),
                text,
                syntax,
            });
        }
        Ok(Self { sources })
    }
}
