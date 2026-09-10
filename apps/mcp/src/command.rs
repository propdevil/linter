use clap::{Parser, Subcommand};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    name = "linter-mcp",
    version,
    about = "Repository validation MCP server"
)]
pub(crate) struct Arguments {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the bundled skill or install it at a new file path.
    Skill {
        /// Create this file and missing parent directories; never overwrite.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
}

impl Arguments {
    pub(crate) fn export(self) -> io::Result<bool> {
        let Some(Command::Skill { output }) = self.command else {
            return Ok(false);
        };
        match output {
            Some(path) => {
                if let Some(parent) = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    fs::create_dir_all(parent)?;
                }
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?
                    .write_all(crate::skill::TEXT.as_bytes())?;
            }
            None => io::stdout()
                .lock()
                .write_all(crate::skill::TEXT.as_bytes())?,
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_stdio_and_rejects_unknown_arguments() {
        assert!(
            !Arguments::try_parse_from(["linter-mcp"])
                .unwrap()
                .export()
                .unwrap()
        );
        for args in [
            vec!["linter-mcp", "unknown"],
            vec!["linter-mcp", "skill", "--output"],
            vec!["linter-mcp", "skill", "--force"],
        ] {
            assert!(Arguments::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn export_creates_parents_preserves_existing_files_and_reports_errors() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("skills/software-design/SKILL.md");
        let export = |path: &std::path::Path| {
            Arguments::try_parse_from([
                std::ffi::OsStr::new("linter-mcp"),
                std::ffi::OsStr::new("skill"),
                std::ffi::OsStr::new("--output"),
                path.as_os_str(),
            ])
            .unwrap()
            .export()
        };
        assert!(export(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), crate::skill::TEXT);
        fs::write(&path, "keep existing skill").unwrap();
        assert_eq!(
            export(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep existing skill");
        assert!(export(&path.join("child")).is_err());
        assert!(export(root.path()).is_err());
    }
}
