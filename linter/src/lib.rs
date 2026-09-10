use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

use globset::GlobSet;
use serde::Serialize;

mod config;
mod target;
pub use rule::filename::Filename;
pub use target::{Selector, Target};
mod diagnostic;
mod registry;
pub use diagnostic::{Evidence, Span};
mod rule;

pub use registry::Registry;
pub use rule::layout::{Config as LayoutConfig, Layout};
pub use rule::{Analysis, Rule, RuleResult};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Configuration(String),
    #[error("analysis failed: {0}")]
    Analysis(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Completed,
    Disabled,
    Unconfigured,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Evidence>,
    pub rule: &'static str,
    pub path: PathBuf,
    pub configuration: String,
    pub message: String,
    pub instruction: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub rules: BTreeMap<&'static str, Status>,
    pub findings: Vec<Finding>,
}

impl fmt::Display for Report {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for finding in &self.findings {
            writeln!(output, "{}: {}", finding.path.display(), finding.message)?;
            writeln!(output, "  {}", finding.instruction)?;
            writeln!(output, "  Configured by {}.", finding.configuration)?;
        }
        for (rule, status) in &self.rules {
            match status {
                Status::Completed => writeln!(
                    output,
                    "{rule}: {} finding(s)",
                    self.findings
                        .iter()
                        .filter(|finding| finding.rule == *rule)
                        .count()
                )?,
                Status::Disabled => writeln!(output, "{rule}: disabled")?,
                Status::Unconfigured => writeln!(output, "{rule}: unconfigured")?,
            }
        }
        Ok(())
    }
}

/// Discovered input for registered rules. Paths are relative to root.
pub struct Project {
    root: PathBuf,
    entries: Entries,
}

impl Project {
    fn load(root: &Path, exclusions: &GlobSet, scan: bool) -> Result<Self, Error> {
        let metadata = fs::symlink_metadata(root).map_err(|error| Error::io(root, error))?;
        if !metadata.is_dir() {
            return Err(Error::io(
                root,
                std::io::Error::other("root must be a directory, not a symlink or file"),
            ));
        }
        let entries = if scan {
            Entries::read(root, exclusions)?
        } else {
            Entries::default()
        };
        Ok(Self {
            root: root.to_path_buf(),
            entries,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }
}

#[derive(Default)]
pub(crate) struct Entries(Vec<Entry>);

#[derive(Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: fs::FileType,
}

impl Entries {
    fn read(root: &Path, exclusions: &GlobSet) -> Result<Self, Error> {
        let metadata = fs::symlink_metadata(root).map_err(|error| Error::io(root, error))?;
        let mut entries = vec![Entry {
            path: PathBuf::from("."),
            kind: metadata.file_type(),
        }];
        let mut pending = vec![PathBuf::from(".")];
        while let Some(relative) = pending.pop() {
            let path = root.join(&relative);
            for entry in fs::read_dir(&path).map_err(|error| Error::io(&path, error))? {
                let entry = entry.map_err(|error| Error::io(&path, error))?;
                let child = if relative == Path::new(".") {
                    PathBuf::from(entry.file_name())
                } else {
                    relative.join(entry.file_name())
                };
                let kind = entry
                    .file_type()
                    .map_err(|error| Error::io(entry.path(), error))?;
                if exclusions.is_match(&child)
                    || (kind.is_dir()
                        && exclusions.is_match(format!("{}/", child.to_string_lossy())))
                {
                    continue;
                }
                if kind.is_dir() {
                    pending.push(child.clone());
                }
                entries.push(Entry { path: child, kind });
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self(entries))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.0.iter()
    }

    pub fn directories(&self) -> impl Iterator<Item = &Path> {
        self.0
            .iter()
            .filter(|entry| entry.kind.is_dir())
            .map(|entry| entry.path.as_path())
    }
}

pub use rule::{affix::SharedAffix, forbidden_words::ForbiddenWords};

pub use rule::line_width::LineWidth;
pub use rule::parent_name::ParentName;

pub use rule::indentation::MaxIndent;
