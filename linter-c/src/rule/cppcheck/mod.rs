use crate::compilation::Database;
use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use std::{collections::BTreeSet, fs, path::Path, process::Command};
mod config;
use config::Assertion;
pub use config::Config;

pub struct Cppcheck {
    assertions: Vec<Assertion>,
}
impl Rule for Cppcheck {
    const ID: &'static str = "c/cppcheck";
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
        self.invoke(&root, &database, &self.database, findings)?;
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
            .args(["--quiet", "--error-exitcode=1"])
            .arg("--template={file}:{line}:{column}: {severity}: {message} [{id}]")
            .arg(format!(
                "--project={}",
                database.path().join("compile_commands.json").display()
            ))
            .arg(format!("--enable={}", self.checks.join(",")))
            .arg(format!("--std={}", self.standard));
        if self.inconclusive {
            command.arg("--inconclusive");
        }
        for suppression in &self.suppressions {
            command.arg(format!("--suppress={suppression}"));
        }
        let output =
            crate::process::run(&mut command, root, self.timeout_ms, self.max_output_bytes)?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let messages: Vec<_> = text
            .lines()
            .filter(|line| {
                [
                    ": warning:",
                    ": error:",
                    ": style:",
                    ": performance:",
                    ": portability:",
                    ": information:",
                ]
                .iter()
                .any(|marker| line.contains(marker))
            })
            .collect();
        if !output.status.success() && messages.is_empty() {
            return Err(Error::Analysis(format!(
                "{}: cppcheck failed ({}): {text}",
                file.display(),
                output.status
            )));
        }
        for message in messages {
            findings.push(Finding {
                rule: Cppcheck::ID,
                path: file.strip_prefix(root).unwrap_or(file).into(),
                span: None,
                related: Vec::new(),
                configuration: self.setting.clone(),
                message: message.into(),
                instruction: "Resolve the reported cppcheck diagnostic.".into(),
            });
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn check(script: &str, fields: &str) -> Result<linter::Report, Error> {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("unit.c"), "int x;").unwrap();
        fs::write(
            root.path().join("compile_commands.json"),
            r#"[
{"directory":".","file":"unit.c","arguments":["cc","-c","unit.c"]}]
"#,
        )
        .unwrap();
        let executable = root.path().join("cppcheck");
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!(
                r#"
[[rules."c/cppcheck"]]
target = "*.c"
executable = "./cppcheck"
compilation_database = "compile_commands.json"
checks = ["warning", "performance", "portability"]
standard = "c11"
{fields}
"#
            ),
        )
        .unwrap();
        linter::Registry::default()
            .register::<Cppcheck>()?
            .check(root.path())
    }
    #[test]
    fn captures_diagnostics_and_distinguishes_execution_failure() {
        assert!(check("exit 0", "").unwrap().findings.is_empty());
        let report = check(
            "printf 'unit.c:1:1: warning: possible leak [memleak]\\n' >&2; exit 1",
            "",
        )
        .unwrap();
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].message.contains("[memleak]"));
        assert!(check("printf 'invalid options' >&2; exit 2", "").is_err());
    }
    #[test]
    fn configuration_failures_are_not_silently_accepted() {
        for fields in [
            "extra_args = ['--fix']",
            "timeout_ms = 0",
            "max_output_bytes = 0",
        ] {
            assert!(matches!(
                check("exit 0", fields),
                Err(Error::Configuration(_))
            ));
        }
        assert!(
            check("exit 9", "exclude = '*.c'")
                .unwrap()
                .findings
                .is_empty()
        );
    }
}
