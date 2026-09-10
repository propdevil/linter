use crate::{Finding, Report, Span, Status};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Directive {
    pub path: PathBuf,
    pub span: Span,
    pub target: Option<Span>,
    pub rule: String,
    pub reason: String,
}

impl Directive {
    pub fn parse(path: &Path, text: &str, span: Span, target: Option<Span>) -> Option<Self> {
        let text = text
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim();
        let text = text.strip_prefix("linter:")?;
        let (rule, reason) = text
            .strip_prefix("disable ")
            .and_then(|text| text.split_once("--"))
            .unwrap_or(("", ""));
        Some(Self {
            path: path.to_owned(),
            span,
            target,
            rule: rule.trim().into(),
            reason: reason.trim().trim_end_matches("*/").trim().into(),
        })
    }

    fn problem(&self, known: &BTreeSet<&str>) -> Option<&str> {
        if self.rule.is_empty() || self.reason.is_empty() {
            Some("expected linter:disable RULE -- concrete reason")
        } else if !known.contains(self.rule.as_str()) {
            Some("directive names an unregistered rule")
        } else if self.target.is_none() {
            Some("directive is not attached to a following item or statement")
        } else {
            None
        }
    }

    fn finding(&self, message: &str) -> Finding {
        Finding { rule: "directive", path: self.path.clone(), span: Some(self.span.clone()),
            related: Vec::new(), configuration: "source directive".into(), message: message.into(),
            instruction: "Attach one registered rule ID and a concrete reason to the next item or statement; remove unused directives.".into() }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Suppressed {
    pub finding: Finding,
    pub reason: String,
}

pub(crate) fn apply(report: &mut Report, directives: Vec<Directive>, known: BTreeSet<&str>) {
    if directives.is_empty() {
        return;
    }
    let mut used = vec![false; directives.len()];
    for finding in std::mem::take(&mut report.findings) {
        let matched = directives.iter().enumerate().find(|(_, directive)| {
            directive.problem(&known).is_none()
                && directive.path == finding.path
                && directive.rule == finding.rule
                && directive
                    .target
                    .as_ref()
                    .zip(finding.span.as_ref())
                    .is_some_and(|(target, span)| {
                        span.start >= target.start && span.end <= target.end
                    })
        });
        if let Some((index, directive)) = matched {
            used[index] = true;
            report.suppressed.push(Suppressed {
                finding,
                reason: directive.reason.clone(),
            });
        } else {
            report.findings.push(finding);
        }
    }
    for (index, directive) in directives.iter().enumerate() {
        if let Some(problem) = directive.problem(&known) {
            report.findings.push(directive.finding(problem));
        } else if !used[index] {
            report
                .findings
                .push(directive.finding("directive did not suppress any finding"));
        }
    }
    report.rules.insert("directive", Status::Completed);
}
