use std::{env, path::PathBuf, process::ExitCode};

use design_lint::{
    AccessorBloat, AsyncBlocking, BooleanState, BroadTrait, Cases, CatchAllModule,
    CeremonialStructure, DeepControlFlow, DependencyDirection, Diagnostic, DuplicateEntity,
    EmptyDirectory, EnvironmentAccess, FileLength, FiniteStateString, FreeFunction, GodObject,
    GuiToolkitLeakage, IgnoredResult, LintError, Linter, Markdown, ModelDuplication,
    PlatformCommand, ReceiverRepetition, Registry, Reporter, Result, Severity,
    SingleFileDirectory, SingleUse, StructNaming,
};

enum Output {
    Diagnostic,
    Markdown,
    Cases(PathBuf),
}

/// Every rule keyed by its stable diagnostic id. Adopt rules one at a time
/// with `--rule <id>` (repeatable); with no `--rule`, every rule runs.
fn registry(selected: &[String]) -> Registry {
    let all: &[(&str, fn(Registry) -> Registry)] = &[
        ("dependency-direction", |r| r.register(DependencyDirection)),
        ("unclassified-free-function", |r| r.register(FreeFunction)),
        ("duplicate-entity-base", |r| r.register(DuplicateEntity)),
        ("boolean-state-cluster", |r| r.register(BooleanState)),
        ("broad-trait-responsibilities", |r| r.register(BroadTrait)),
        ("environment-variable-access", |r| r.register(EnvironmentAccess)),
        ("platform-command-boundary", |r| r.register(PlatformCommand)),
        ("ignored-fallible-result", |r| r.register(IgnoredResult)),
        ("async-blocking-operation", |r| r.register(AsyncBlocking)),
        ("struct-noun-naming", |r| r.register(StructNaming)),
        ("receiver-name-repetition", |r| r.register(ReceiverRepetition)),
        ("gui-toolkit-type-leakage", |r| r.register(GuiToolkitLeakage)),
        ("god-object-growth", |r| r.register(GodObject)),
        ("redundant-accessor", |r| r.register(AccessorBloat)),
        ("wire-domain-model-duplication", |r| r.register(ModelDuplication)),
        ("single-use-free-function", |r| r.register(SingleUse)),
        ("deep-control-flow", |r| r.register(DeepControlFlow)),
        ("file-length", |r| r.register(FileLength)),
        ("string-backed-finite-state", |r| r.register(FiniteStateString)),
        ("catch-all-module-name", |r| r.register(CatchAllModule)),
        ("empty-directory", |r| r.register(EmptyDirectory)),
        ("single-file-directory", |r| r.register(SingleFileDirectory)),
        ("ceremonial-structure", |r| r.register(CeremonialStructure)),
    ];
    let mut registry = Registry::new();
    for (id, add) in all {
        if selected.is_empty() || selected.iter().any(|value| value == id) {
            registry = add(registry);
        }
    }
    registry
}

fn main() -> ExitCode {
    match run(env::args_os().skip(1)) {
        Ok(success) if success => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<bool> {
    let mut output = Output::Diagnostic;
    let mut paths = Vec::new();
    let mut rules: Vec<String> = Vec::new();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "--markdown" {
            output = Output::Markdown;
        } else if argument == "--cases" {
            output = Output::Cases(PathBuf::from(
                arguments
                    .next()
                    .ok_or(LintError::Argument("--cases requires an output directory"))?,
            ));
        } else if argument == "--rule" {
            rules.push(
                arguments
                    .next()
                    .ok_or(LintError::Argument("--rule requires a rule id"))?
                    .to_string_lossy()
                    .into_owned(),
            );
        } else {
            paths.push(PathBuf::from(argument));
        }
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("src"));
    }

    let cases = matches!(output, Output::Cases(_));
    let mut reporter: Box<dyn Reporter> = match output {
        Output::Diagnostic => Box::new(Diagnostic::default()),
        Output::Markdown => Box::new(Markdown::default()),
        Output::Cases(root) => Box::new(Cases::new(root)),
    };
    let summaries = Linter::new(registry(&rules)).run(paths, reporter.as_mut())?;
    Ok(cases
        || !summaries
            .iter()
            .any(|summary| summary.severity == Severity::Error && summary.findings != 0))
}
