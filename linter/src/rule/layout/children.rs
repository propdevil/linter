use super::config::{Assertion, Mode, Requirements};
use crate::{Entry, Error, Finding, Project};
use std::path::Path;

fn children<'a>(project: &'a Project, directory: &'a Path) -> impl Iterator<Item = &'a Entry> {
    project.entries().filter(move |entry| {
        entry.path != Path::new(".")
            && entry.path.parent().is_some_and(|parent| {
                parent == directory
                    || (directory == Path::new(".") && parent.as_os_str().is_empty())
            })
    })
}

impl Assertion {
    pub(super) fn inspect_children(
        &self,
        project: &Project,
        directory: &Path,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        for entry in children(project, directory) {
            let (requirements, label) = if entry.kind.is_dir() {
                (&self.directories, "directories")
            } else {
                (&self.files, "files")
            };
            self.inspect_shape(project, entry, requirements, label, findings)?;
            if self.mode != Mode::Restrictive {
                continue;
            }
            let name = Path::new(entry.path.file_name().unwrap_or_default());
            let required = self.files.required.iter().any(|path| path == name)
                || self
                    .directories
                    .required
                    .iter()
                    .any(|path| path.starts_with(name))
                || self
                    .files
                    .required
                    .iter()
                    .any(|path| path != name && path.starts_with(name));
            if required {
                continue;
            }
            if (!entry.kind.is_file() && !entry.kind.is_dir()) || !requirements.allows(name) {
                findings.push(self.finding(
                    directory,
                    format!("unexpected entry {}", name.display()),
                    format!(
                        "Remove {} or add a target and purpose description in {}.{label}.allowed.",
                        entry.path.display(),
                        self.setting
                    ),
                ));
            }
        }
        Ok(())
    }

    fn inspect_shape(
        &self,
        project: &Project,
        entry: &Entry,
        policy: &Requirements,
        label: &str,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        if !entry.kind.is_dir() && !entry.kind.is_file() {
            return Ok(());
        }
        let empty = if entry.kind.is_dir() {
            let contents: Vec<_> = children(project, &entry.path)
                .filter(|child| {
                    !policy.content_ignored.iter().any(|pattern| {
                        pattern
                            .is_match(child.path.strip_prefix(&entry.path).unwrap_or(&child.path))
                    })
                })
                .collect();
            if !policy.allow_single_file && contents.len() == 1 && contents[0].kind.is_file() {
                findings.push(self.finding(&entry.path, "directory contains only one file".into(),
                    "Flatten this directory when appropriate, or narrow the configured target to preserve intentional boundaries.".into()));
            }
            contents.is_empty()
        } else {
            let path = project.root().join(&entry.path);
            std::fs::symlink_metadata(&path)
                .map_err(|error| Error::io(&path, error))?
                .len()
                == 0
        };
        if !policy.allow_empty && empty {
            let noun = if entry.kind.is_dir() {
                "directory"
            } else {
                "file"
            };
            findings.push(self.finding(&entry.path, format!("empty {noun} is not allowed"),
                format!("Add meaningful content, remove this {noun}, or set {}.{label}.allow_empty = true.", self.setting)));
        }
        Ok(())
    }
}
