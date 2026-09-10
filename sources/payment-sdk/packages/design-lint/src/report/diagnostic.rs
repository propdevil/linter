use std::{
    io::{self, Write},
    path::PathBuf,
};

use crate::{
    LintError, Result,
    model::{Finding, Summary},
    report::Reporter,
};

/// Emits compiler-style diagnostics and summaries.
pub struct Diagnostic<Output = io::Stderr> {
    output: Output,
    roots: Vec<PathBuf>,
}

impl<Output> Diagnostic<Output> {
    /// Creates a diagnostic reporter with an injected output.
    pub fn new(output: Output) -> Self {
        Self {
            output,
            roots: Vec::new(),
        }
    }

    /// Returns the injected output.
    pub fn into_inner(self) -> Output {
        self.output
    }
}

impl Default for Diagnostic<io::Stderr> {
    fn default() -> Self {
        Self::new(io::stderr())
    }
}

impl<Output: Write> Diagnostic<Output> {
    fn render(&mut self, finding: &Finding) -> io::Result<()> {
        writeln!(
            self.output,
            "{}[{}]: {}\n  --> {}:{}:{}\n   = help: {}",
            finding.severity.as_str(),
            finding.rule,
            finding.message,
            super::display_path(&finding.location.path, &self.roots).display(),
            finding.location.line,
            finding.location.column,
            finding.help
        )?;
        if !finding.location.source.is_empty() {
            writeln!(self.output, "{}", finding.location.source)?;
        }
        for related in &finding.related {
            writeln!(
                self.output,
                "   = related: {}\n  --> {}:{}:{}\n{}",
                related.label,
                super::display_path(&related.location.path, &self.roots).display(),
                related.location.line,
                related.location.column,
                related.location.source
            )?;
        }
        if let Some(review) = &finding.review {
            for (key, value) in &review.metadata {
                writeln!(self.output, "   = {key}: {value}")?;
            }
            for dependency in &review.dependencies {
                writeln!(self.output, "   = dependency: {dependency}")?;
            }
            for question in &review.questions {
                writeln!(self.output, "   = review: {question}")?;
            }
        }
        Ok(())
    }
}

impl<Output: Write> Reporter for Diagnostic<Output> {
    fn begin(&mut self, workspace: &crate::source::Workspace) -> Result<()> {
        self.roots.clone_from(&workspace.policy_roots);
        Ok(())
    }

    fn finding(&mut self, finding: &Finding) -> Result<()> {
        self.render(finding)
            .map_err(|error| LintError::report("diagnostic", error))
    }

    fn finish(&mut self, summaries: &[Summary]) -> Result<()> {
        for summary in summaries {
            writeln!(
                self.output,
                "{}: {} {}(s)",
                summary.rule,
                summary.findings,
                summary.severity.as_str()
            )
            .map_err(|error| LintError::report("diagnostic summary", error))?;
        }
        Ok(())
    }
}
