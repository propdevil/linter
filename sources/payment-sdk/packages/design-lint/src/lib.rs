//! Small, policy-driven checks for the architectural rules this repository relies on.

mod error;
mod model;
mod policy;
mod report;
pub mod rule;
pub mod source;

pub use error::{LintError, Result};
pub use model::{Finding, Location, Related, Review, Severity, Summary};
pub use policy::{Boundaries, Policy, Rules, Rust, SourceSelector};
pub use report::{Cases, Diagnostic, Markdown, Reporter};
pub use rule::{Registry, Rule};
use source::Workspace;
use std::path::PathBuf;

#[cfg(test)]
mod test_support;

/// Runs the focused repository and Rust API rules.
pub struct Linter {
    policy: Policy,
    registry: Registry,
}

/// Every built-in rule in execution order; policy selects adopted rules from this catalog.
pub(crate) fn standard_rules() -> Registry {
    Registry::new()
        .register(rule::DependencyDirection)
        .register(rule::OwnedVocabulary)
        .register(rule::FileLength)
        .register(rule::ForbiddenPath)
        .register(rule::EmptyDirectory)
        .register(rule::ChainLayout)
        .register(rule::SingleFileDirectory)
        .register(rule::TraitMethodCount)
        .register(rule::EmptyStruct)
        .register(rule::StructWordCount)
        .register(rule::SelfConstructorStatic)
        .register(rule::ReceiverRepetition)
        .register(rule::CatchAllModule)
        .register(rule::StructNaming)
        .register(rule::FreeFunction)
        .register(rule::SingleUse)
        .register(rule::DeepControlFlow)
        .register(rule::EnvironmentAccess)
        .register(rule::PlatformCommand)
        .register(rule::IgnoredResult)
        .register(rule::AsyncBlocking)
        .register(rule::BooleanState)
        .register(rule::FiniteStateString)
        .register(rule::GodObject)
        .register(rule::AccessorBloat)
        .register(rule::DuplicateEntity)
        .register(rule::ModelDuplication)
        .register(rule::CeremonialStructure)
}

impl Linter {
    pub fn new(policy: Policy, registry: Registry) -> Self {
        Self { policy, registry }
    }

    pub fn standard_with_policy(policy: Policy) -> Result<Self> {
        let registry = Registry::standard(&policy)?;
        Ok(Self::new(policy, registry))
    }

    pub fn run(&self, paths: Vec<PathBuf>, reporter: &mut dyn Reporter) -> Result<Vec<Summary>> {
        self.policy.validate()?;
        self.registry.validate()?;
        let workspace = Workspace::load(paths, &self.policy.source)?;
        // Complete analysis before allowing a reporter to replace persistent output.
        let mut output = Vec::new();
        let mut summaries = Vec::new();
        for rule in self.registry.iter() {
            let mut findings = rule.check(&workspace, &self.policy)?;
            if findings
                .iter()
                .any(|finding| finding.rule != rule.id() || finding.severity != rule.severity())
            {
                return Err(LintError::configuration(format!(
                    "rule `{}` returned inconsistent metadata",
                    rule.id()
                )));
            }
            if rule::adopted::contains(rule.id()) {
                findings.retain(|finding| {
                    !workspace.sources().iter().any(|source| {
                        source.path == finding.location.path
                            && source.suppressed(rule.id(), finding.location.line)
                    })
                });
            }
            findings.sort_by(|left, right| {
                (
                    &left.location.path,
                    left.location.line,
                    left.location.column,
                    &left.subject,
                )
                    .cmp(&(
                        &right.location.path,
                        right.location.line,
                        right.location.column,
                        &right.subject,
                    ))
            });
            summaries.push(Summary {
                rule: rule.id(),
                severity: rule.severity(),
                findings: findings.len(),
            });
            output.extend(findings);
        }
        reporter.begin(&workspace)?;
        for finding in &output {
            reporter.finding(finding)?;
        }
        reporter.finish(&summaries)?;
        Ok(summaries)
    }
}

#[cfg(test)]
mod runner_tests;
