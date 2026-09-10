//! Named repository and Rust design rules for explicit registry composition.

pub(crate) mod adopted;
mod registry;
pub use adopted::{
    AccessorBloat, AsyncBlocking, BooleanState, CatchAllModule, CeremonialStructure,
    DeepControlFlow, DuplicateEntity, EnvironmentAccess, FiniteStateString, FreeFunction,
    GodObject, IgnoredResult, ModelDuplication, PlatformCommand, ReceiverRepetition, SingleUse,
    StructNaming,
};
pub use registry::Registry;
pub(crate) mod production;
pub(crate) mod references;
mod repository;
mod rust;
pub(crate) mod syntax;

use crate::{Finding, Location, Policy, Result, Severity, source::Workspace};

/// One check with a stable identifier and severity shared by all its findings.
pub trait Rule {
    fn id(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>>;
}

/// Checks local Cargo dependency direction and cycles.
pub struct DependencyDirection;

impl Rule for DependencyDirection {
    fn id(&self) -> &'static str {
        "dependency-direction"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::dependencies(workspace, policy)
    }
}

/// Keeps chain-owned vocabulary inside configured ownership boundaries.
pub struct OwnedVocabulary;

impl Rule for OwnedVocabulary {
    fn id(&self) -> &'static str {
        "owned-vocabulary"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::vocabulary(workspace, policy)
    }
}

/// Limits production Rust file length.
pub struct FileLength;

impl Rule for FileLength {
    fn id(&self) -> &'static str {
        "file-length"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::file_length(workspace, policy)
    }
}

/// Rejects superseded repository paths.
pub struct ForbiddenPath;

impl Rule for ForbiddenPath {
    fn id(&self) -> &'static str {
        "forbidden-path"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::forbidden_paths(workspace, policy)
    }
}

/// Rejects empty repository-owned directories.
pub struct EmptyDirectory;

impl Rule for EmptyDirectory {
    fn id(&self) -> &'static str {
        "empty-directory"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::empty_directories(workspace, policy)
    }
}

/// Checks the configured concrete-chain module layout.
pub struct ChainLayout;

impl Rule for ChainLayout {
    fn id(&self) -> &'static str {
        "chain-layout"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::chain_layout(workspace, policy)
    }
}

/// Rejects source directories containing only one Rust file.
pub struct SingleFileDirectory;

impl Rule for SingleFileDirectory {
    fn id(&self) -> &'static str {
        "single-file-directory"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        repository::single_file_directories(workspace, policy)
    }
}

/// Limits each capability trait to three methods.
pub struct TraitMethodCount;

impl Rule for TraitMethodCount {
    fn id(&self) -> &'static str {
        "trait-method-count"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        rust::traits(workspace, policy)
    }
}

/// Rejects structs that carry no state.
pub struct EmptyStruct;

impl Rule for EmptyStruct {
    fn id(&self) -> &'static str {
        "empty-struct"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        rust::empty_structs(workspace, policy)
    }
}

/// Limits semantic words in struct names.
pub struct StructWordCount;

impl Rule for StructWordCount {
    fn id(&self) -> &'static str {
        "struct-word-count"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        rust::struct_names(workspace, policy)
    }
}

/// Requires Self-returning constructors to be associated functions.
pub struct SelfConstructorStatic;

impl Rule for SelfConstructorStatic {
    fn id(&self) -> &'static str {
        "self-constructor-static"
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, workspace: &Workspace, policy: &Policy) -> Result<Vec<Finding>> {
        rust::constructors(workspace, policy)
    }
}

fn finding(
    rule: &'static str,
    subject: impl Into<String>,
    location: Location,
    message: impl Into<String>,
    help: impl Into<String>,
) -> Finding {
    let mut finding = Finding::error(rule, subject, location);
    finding.message = message.into();
    finding.help = help.into();
    finding
}

#[cfg(test)]
#[path = "test.rs"]
mod tests;
