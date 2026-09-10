use linter::{Error, Finding, Project, Rule, RuleResult, Status};
use std::{fs, path::Path, process::Command};
mod config;
use config::Assertion;
pub use config::Config;

pub struct Format {
    assertions: Vec<Assertion>,
}
impl Rule for Format {
    const ID: &'static str = "c/format";
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
            for entry in project.entries().filter(|entry| entry.kind.is_file()) {
                if assertion.selected(&entry.path) {
                    assertion.inspect(project, &entry.path, &mut findings)?;
                }
            }
        }
        Ok(RuleResult {
            status: Status::Completed,
            findings,
        })
    }
}

impl Assertion {
    fn selected(&self, path: &Path) -> bool {
        matches!(path.extension().and_then(|s| s.to_str()), Some("c" | "h"))
            && self.target.matches(path)
            && !self
                .exclude
                .as_ref()
                .is_some_and(|selector| selector.matches(path))
    }
    fn inspect(
        &self,
        project: &Project,
        relative: &Path,
        findings: &mut Vec<Finding>,
    ) -> Result<(), Error> {
        let root = fs::canonicalize(project.root()).map_err(|source| Error::Io {
            path: project.root().into(),
            source,
        })?;
        let path = root.join(relative);
        let before = fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let executable = if self.executable.components().count() > 1 {
            root.join(&self.executable)
        } else {
            self.executable.clone()
        };
        let output = crate::process::run(
            Command::new(executable)
                .arg(format!("--style={}", self.style))
                .arg(format!("--fallback-style={}", self.fallback_style))
                .arg("--")
                .arg(&path),
            &root,
            self.timeout_ms,
            self.max_output_bytes,
        )?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(Error::Analysis(format!(
                "{}: formatter failed ({}): {}",
                relative.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        if before != output.stdout {
            findings.push(Finding {
                rule: Format::ID,
                path: relative.into(),
                span: None,
                related: Vec::new(),
                configuration: self.setting.clone(),
                message: "C source differs from the configured formatter output".into(),
                instruction: "Run the configured formatter and review the formatting changes."
                    .into(),
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
        let executable = root.path().join("formatter");
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.path().join("file.c"), "int x;\n").unwrap();
        fs::write(
            root.path().join("linter.toml"),
            format!(
                r#"
[[rules."c/format"]]
target = "*.c"
executable = "./formatter"
style = "LLVM"
fallback_style = "LLVM"
{fields}
"#
            ),
        )
        .unwrap();
        let result = linter::Registry::default()
            .register::<Format>()?
            .check(root.path());
        assert_eq!(
            fs::read_to_string(root.path().join("file.c")).unwrap(),
            "int x;\n"
        );
        result
    }
    #[test]
    fn compares_output_without_modifying_sources() {
        assert!(check("printf 'int x;\\n'", "").unwrap().findings.is_empty());
        assert_eq!(check("printf 'int  x;\\n'", "").unwrap().findings.len(), 1);
        assert!(check("exit 7", "").is_err());
        assert!(check("printf bad >&2", "").is_err());
    }
    #[test]
    fn failures_limits_and_exclusions_are_observable() {
        assert!(check("while :; do :; done", "timeout_ms = 20").is_err());
        assert!(check("printf 123456789", "max_output_bytes = 4").is_err());
        assert!(
            check("exit 9", "exclude = '*.c'")
                .unwrap()
                .findings
                .is_empty()
        );
        assert!(matches!(
            check("exit 0", "timeout_ms = 0"),
            Err(Error::Configuration(_))
        ));
        assert!(matches!(
            check("exit 0", "arguments = ['-i']"),
            Err(Error::Configuration(_))
        ));
    }
}
