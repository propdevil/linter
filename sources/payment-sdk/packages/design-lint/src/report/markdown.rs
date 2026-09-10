use std::{
    io::{self, Write},
    path::PathBuf,
};

use crate::{
    LintError, Result,
    model::{Finding, Summary},
    report::Reporter,
};

/// Emits a single Markdown review document.
pub struct Markdown<Output = io::Stdout> {
    output: Output,
    started: bool,
    roots: Vec<PathBuf>,
}

impl<Output> Markdown<Output> {
    /// Creates a Markdown reporter with an injected output.
    pub fn new(output: Output) -> Self {
        Self {
            output,
            started: false,
            roots: Vec::new(),
        }
    }

    /// Returns the injected output.
    pub fn into_inner(self) -> Output {
        self.output
    }
}

impl Default for Markdown<io::Stdout> {
    fn default() -> Self {
        Self::new(io::stdout())
    }
}

impl<Output: Write> Markdown<Output> {
    fn start(&mut self) -> Result<()> {
        if !self.started {
            writeln!(self.output, "# Linting review\n")
                .map_err(|error| LintError::report("Markdown heading", error))?;
            self.started = true;
        }
        Ok(())
    }
}

impl<Output: Write> Reporter for Markdown<Output> {
    fn begin(&mut self, workspace: &crate::source::Workspace) -> Result<()> {
        self.roots.clone_from(&workspace.policy_roots);
        Ok(())
    }

    fn finding(&mut self, finding: &Finding) -> Result<()> {
        self.start()?;
        render(&mut self.output, finding, &self.roots)
            .map_err(|error| LintError::report("Markdown finding", error))
    }

    fn finish(&mut self, summaries: &[Summary]) -> Result<()> {
        self.start()?;
        writeln!(self.output, "## Summary\n")
            .map_err(|error| LintError::report("Markdown summary", error))?;
        for summary in summaries {
            writeln!(
                self.output,
                "- `{}`: {} {}(s)",
                summary.rule,
                summary.findings,
                summary.severity.as_str()
            )
            .map_err(|error| LintError::report("Markdown summary", error))?;
        }
        Ok(())
    }
}

pub(super) fn render(
    output: &mut impl Write,
    finding: &Finding,
    roots: &[PathBuf],
) -> io::Result<()> {
    writeln!(
        output,
        "## `{}`\n\n- [ ] Reviewed\n- Rule: `{}`\n- Severity: `{}`\n- Location: `{}:{}:{}`\n- Decision:\n\n{}\n\nHelp: {}\n",
        finding.subject,
        finding.rule,
        finding.severity.as_str(),
        super::display_path(&finding.location.path, roots).display(),
        finding.location.line,
        finding.location.column,
        finding.message,
        finding.help,
    )?;
    code(output, &finding.location.source)?;
    for related in &finding.related {
        writeln!(
            output,
            "### Related: {}\n\nLocation: `{}:{}:{}`\n",
            related.label,
            super::display_path(&related.location.path, roots).display(),
            related.location.line,
            related.location.column
        )?;
        code(output, &related.location.source)?;
    }
    if let Some(review) = &finding.review {
        if !review.metadata.is_empty() {
            writeln!(output, "### Evidence\n")?;
            for (key, value) in &review.metadata {
                writeln!(output, "- {key}: {value}")?;
            }
            writeln!(output)?;
        }
        if !review.dependencies.is_empty() {
            writeln!(output, "### Dependencies\n")?;
            for dependency in &review.dependencies {
                writeln!(output, "- `{dependency}`")?;
            }
            writeln!(output)?;
        }
        if !review.questions.is_empty() {
            writeln!(output, "### Review questions\n")?;
            for question in &review.questions {
                writeln!(output, "- [ ] {question}")?;
            }
            writeln!(output)?;
        }
    }
    Ok(())
}

fn code(output: &mut impl Write, source: &str) -> io::Result<()> {
    let longest = source
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.saturating_add(1).max(3));
    writeln!(output, "{fence}rust\n{source}\n{fence}\n")
}
