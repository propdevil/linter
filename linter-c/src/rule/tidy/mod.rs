use crate::compilation::Database;
use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use std::{collections::BTreeSet, fs, path::Path, process::Command};
mod config;
use config::Assertion;
pub use config::Config;

pub struct Tidy {
    assertions: Vec<Assertion>,
}
impl Rule for Tidy {
    const ID: &'static str = "c/tidy";
    type Analysis = ();
    type Config = Config;
    fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            assertions: config.compile()?,
        })
    }
    fn configured(&self) -> bool {
        !self.assertions.is_empty()
    }
    fn check(&self, project: &Project, _: &()) -> Result<RuleResult, Error> {
        let mut findings = Vec::new();
        for assertion in &self.assertions {
            assertion.inspect(project, &mut findings)?;
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}
impl Assertion {
    fn inspect(&self, project: &Project, findings: &mut Vec<Finding>) -> Result<(), Error> {
        let root = fs::canonicalize(project.root()).map_err(|source| Error::Io {
            path: project.root().into(),
            source,
        })?;
        let selected: BTreeSet<_> = project
            .entries()
            .filter(|entry| entry.kind.is_file() && self.selected(&entry.path))
            .map(|entry| root.join(&entry.path))
            .collect();
        if selected.is_empty() {
            return Ok(());
        }
        let database = Database::select(&root.join(&self.database), &selected)?;
        for file in &database.files {
            self.invoke(&root, &database, file, findings)?;
        }
        Ok(())
    }
    fn selected(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "c")
            && self.target.matches(path)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|selector| selector.matches(path))
    }
    fn invoke(
        &self,
        root: &Path,
        database: &Database,
        file: &Path,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        let executable = if self.executable.components().count() > 1 {
            root.join(&self.executable)
        } else {
            self.executable.clone()
        };
        let mut command = Command::new(executable);
        command
            .args(["--quiet", "--use-color=false", "--warnings-as-errors=*"])
            .arg("-p")
            .arg(database.path())
            .arg(format!("--checks={}", self.checks));
        for argument in &self.extra_args {
            command.arg(format!("--extra-arg={argument}"));
        }
        command.arg(file);
        let output =
            crate::process::run(&mut command, root, self.timeout_ms, self.max_output_bytes)?;
        self.diagnostics(root, file, output, findings)
    }
    fn diagnostics(
        &self,
        root: &Path,
        file: &Path,
        output: crate::process::Output,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let messages: Vec<_> = text
            .lines()
            .filter(|line| {
                [": warning:", ": error:", ": fatal error:"]
                    .iter()
                    .any(|marker| line.contains(marker))
            })
            .collect();
        if !output.status.success() && messages.is_empty() {
            return Err(Error::Analysis(format!(
                "{}: clang-tidy failed ({}): {text}",
                file.display(),
                output.status
            )));
        }
        for message in messages {
            findings.push(Finding {
                rule: Tidy::ID,
                path: file.strip_prefix(root).unwrap_or(file).into(),
                span: None,
                related: Vec::new(),
                configuration: self.setting.clone(),
                message: message.into(),
                instruction: "Resolve the reported clang-tidy diagnostic.".into(),
            });
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn check(script: &str) -> Result<linter::Report, Error> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("unit.c"), "int x;").unwrap();
        fs::write(
            root.path().join("compile_commands.json"),
            r#"[
{"directory":".","file":"unit.c","arguments":["cc","-c","unit.c"]}]
"#,
        )
        .unwrap();
        let executable = root.path().join("tidy");
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            r#"
[[rules."c/tidy"]]
target = "*.c"
executable = "./tidy"
compilation_database = "compile_commands.json"
checks = "clang-analyzer-*"
"#,
        )
        .unwrap();
        linter::Registry::default()
            .register::<Tidy>()?
            .check(root.path())
    }
    #[test]
    fn captures_diagnostics_and_distinguishes_execution_failure() {
        assert!(check("exit 0").unwrap().findings.is_empty());
        let report =
            check("printf 'unit.c:1:1: warning: possible leak [check]\\n'; exit 1").unwrap();
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].path, std::path::PathBuf::from("unit.c"));
        assert!(check("printf 'invalid options' >&2; exit 2").is_err());
    }
}
