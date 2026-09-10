use linter::{Error, Project};
use pulldown_cmark::{Event, Options, Parser};
use std::{fs, ops::Range, path::PathBuf};

pub struct Source {
    pub path: PathBuf,
    pub text: String,
    pub events: Vec<(Event<'static>, Range<usize>)>,
}

pub struct Analysis {
    pub sources: Vec<Source>,
}

impl linter::Analysis for Analysis {
    fn load(project: &Project) -> Result<Self, Error> {
        let mut sources = Vec::new();
        for entry in project.entries().filter(|entry| entry.kind.is_file()) {
            let Some(extension) = entry.path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !["md", "markdown"]
                .iter()
                .any(|value| extension.eq_ignore_ascii_case(value))
            {
                continue;
            }
            let path = project.root().join(&entry.path);
            let text = fs::read_to_string(&path).map_err(|source| Error::Io { path, source })?;
            let events = Parser::new_ext(&text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
                .into_offset_iter()
                .map(|(event, range)| (event.into_static(), range))
                .collect();
            sources.push(Source {
                path: entry.path.clone(),
                text,
                events,
            });
        }
        Ok(Self { sources })
    }
}
