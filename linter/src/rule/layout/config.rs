use std::{collections::BTreeSet, path::PathBuf};

use globset::GlobMatcher;
use serde::Deserialize;

use crate::{
    Error,
    config::{matcher, relative_path},
};

#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Config(Vec<Block>);

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Block {
    Permission(PermissionConfig),
    Structure(Box<Definition>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionConfig {
    target: crate::Target,
    allow: bool,
    #[serde(default)]
    kind: Kind,
    description: Option<String>,
    #[serde(default = "case_sensitive")]
    case_sensitive: bool,
}

fn case_sensitive() -> bool {
    true
}

pub(super) struct Permission {
    pub setting: String,
    pub selector: crate::Selector,
    pub allow: bool,
    pub kind: Kind,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    #[default]
    File,
    Directory,
    Any,
}
impl Kind {
    pub fn matches(&self, kind: std::fs::FileType) -> bool {
        match self {
            Self::File => !kind.is_dir(),
            Self::Directory => kind.is_dir(),
            Self::Any => true,
        }
    }
}

pub(super) enum Check {
    Permission(Permission),
    Structure(Box<Assertion>),
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Mode {
    #[default]
    Permissive,
    Restrictive,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    target: crate::Target,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    files: Selection,
    #[serde(default)]
    directories: Selection,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selection {
    required: Vec<String>,
    allowed: Vec<Allowance>,
    case_sensitive: bool,
    allow_empty: bool,
    allow_single_file: Option<bool>,
    content_ignored: Vec<String>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            required: Vec::new(),
            allowed: Vec::new(),
            case_sensitive: true,
            allow_empty: true,
            allow_single_file: None,
            content_ignored: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Allowance {
    target: String,
    description: String,
}

pub(super) struct Requirements {
    pub required: Vec<PathBuf>,
    pub allowed: Vec<GlobMatcher>,
    pub allow_empty: bool,
    pub allow_single_file: bool,
    pub content_ignored: Vec<GlobMatcher>,
}

pub(super) struct Assertion {
    pub setting: String,
    pub selector: crate::Selector,
    pub mode: Mode,
    pub files: Requirements,
    pub directories: Requirements,
}

impl Config {
    pub(super) fn compile(self) -> Result<Vec<Check>, Error> {
        self.0
            .into_iter()
            .enumerate()
            .map(|(index, block)| block.compile(format!("rules.layout[{index}]")))
            .collect()
    }
}

impl Block {
    fn compile(self, setting: String) -> Result<Check, Error> {
        match self {
            Self::Structure(definition) => definition.compile(setting),
            Self::Permission(permission) => permission.compile(setting),
        }
    }
}

impl PermissionConfig {
    fn compile(self, setting: String) -> Result<Check, Error> {
        if self.allow
            && self
                .description
                .as_ref()
                .is_none_or(|text| text.trim().is_empty())
        {
            return Err(Error::Configuration(format!(
                "{setting}.description: describe the allowed entries' purpose"
            )));
        }
        let selector = self
            .target
            .compile(&format!("{setting}.target"), self.case_sensitive)?;
        Ok(Check::Permission(Permission {
            setting,
            selector,
            allow: self.allow,
            kind: self.kind,
        }))
    }
}

impl Definition {
    fn validate(&self, setting: &str) -> Result<(), Error> {
        if !self.files.content_ignored.is_empty() || self.files.allow_single_file.is_some() {
            return Err(Error::Configuration(format!(
                "{setting}.files: content_ignored and allow_single_file are only valid f\
                or directories"
            )));
        }
        for (label, selection) in [("files", &self.files), ("directories", &self.directories)] {
            if self.mode == Mode::Permissive && !selection.allowed.is_empty() {
                return Err(Error::Configuration(format!(
                    "{setting}.{label}.allowed: requires restrictive mode; use an ordere\
                d allow block to override a ban"
                )));
            }
        }
        Ok(())
    }

    fn compile(self, setting: String) -> Result<Check, Error> {
        self.validate(&setting)?;
        let selector = self.target.compile(&format!("{setting}.target"), true)?;
        let files = self.files.compile(&format!("{setting}.files"))?;
        let directories = self
            .directories
            .compile(&format!("{setting}.directories"))?;
        for file in &files.required {
            if directories
                .required
                .iter()
                .any(|path| path.starts_with(file))
                || files
                    .required
                    .iter()
                    .any(|path| path != file && path.starts_with(file))
            {
                return Err(Error::Configuration(format!(
                    "{setting}: required file {} also needs to be a directory",
                    file.display()
                )));
            }
        }
        Ok(Check::Structure(Box::new(Assertion {
            setting,
            selector,
            mode: self.mode,
            files,
            directories,
        })))
    }
}

impl Selection {
    fn compile(self, setting: &str) -> Result<Requirements, Error> {
        let required: Vec<_> = self
            .required
            .into_iter()
            .map(|value| relative_path(&value, &format!("{setting}.required")))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        let mut seen = BTreeSet::new();
        let allowed: Vec<_> = self
            .allowed
            .into_iter()
            .enumerate()
            .map(|(index, allowance)| {
                let key = format!("{setting}.allowed[{index}]");
                if allowance.description.trim().is_empty() {
                    return Err(Error::Configuration(format!(
                        "{key}.description: describe the allowed entries' purpose"
                    )));
                }
                if !seen.insert(allowance.target.clone()) {
                    return Err(Error::Configuration(format!(
                        "{key}.target: duplicate allowance"
                    )));
                }
                matcher(
                    &allowance.target,
                    &format!("{key}.target"),
                    self.case_sensitive,
                )
            })
            .collect::<Result<_, _>>()?;
        let requirements = Requirements {
            required,
            allowed,
            allow_empty: self.allow_empty,
            allow_single_file: self.allow_single_file.unwrap_or(true),
            content_ignored: self
                .content_ignored
                .iter()
                .map(|pattern| matcher(pattern, &format!("{setting}.content_ignored"), true))
                .collect::<Result<_, _>>()?,
        };
        Ok(requirements)
    }
}

impl Requirements {
    pub fn allows(&self, path: &std::path::Path) -> bool {
        self.allowed.iter().any(|pattern| pattern.is_match(path))
    }
}
