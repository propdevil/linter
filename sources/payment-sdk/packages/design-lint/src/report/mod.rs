mod cases;
mod diagnostic;
mod markdown;

use std::path::{Component, Path, PathBuf};

use crate::{
    Result,
    model::{Finding, Summary},
    source::Workspace,
};

pub use cases::Cases;
pub use diagnostic::Diagnostic;
pub use markdown::Markdown;

fn display_path(path: &Path, roots: &[PathBuf]) -> PathBuf {
    roots
        .iter()
        .filter_map(|root| {
            let relative = path.strip_prefix(root).ok()?;
            if relative
                .components()
                .any(|part| part == Component::ParentDir)
            {
                return None;
            }
            Some((root.components().count(), root.file_name()?, relative))
        })
        .max_by_key(|(depth, _, _)| *depth)
        .map_or_else(
            || path.to_owned(),
            |(_, name, relative)| PathBuf::from(name).join(relative),
        )
}

/// Receives format-neutral findings from the lint runner.
pub trait Reporter {
    /// Prepares output after every rule has completed successfully.
    fn begin(&mut self, _workspace: &Workspace) -> Result<()> {
        Ok(())
    }

    /// Emits one finding.
    fn finding(&mut self, finding: &Finding) -> Result<()>;

    /// Finalizes output after all rules run.
    fn finish(&mut self, summaries: &[Summary]) -> Result<()>;
}

#[cfg(test)]
mod tests;
