use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about = "Validate repository rules using linter.toml")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print an embedded configuration preset.
    Config {
        #[arg(value_enum, default_value = "default")]
        preset: Preset,
    },
    /// Create linter.toml from a preset without overwriting an existing file.
    Init {
        #[arg(value_enum)]
        preset: Preset,
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Check saved files without modifying the repository.
    Check {
        #[arg(default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Preset {
    Default,
    Rust,
    C,
}

impl Preset {
    fn contents(self) -> &'static str {
        match self {
            Self::Default => include_str!("../../../configs/default.toml"),
            Self::Rust => include_str!("../../../configs/rust.toml"),
            Self::C => include_str!("../../../configs/c.toml"),
        }
    }
}

fn main() -> ExitCode {
    let (root, json) = match Arguments::parse().command {
        Command::Check { root, json } => (root, json),
        Command::Config { preset } => {
            return match io::stdout().lock().write_all(preset.contents().as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("write configuration: {error}");
                    ExitCode::from(2)
                }
            };
        }
        Command::Init { preset, root } => {
            let path = root.join("linter.toml");
            let result = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .and_then(|mut file| file.write_all(preset.contents().as_bytes()));
            return match result {
                Ok(()) => {
                    eprintln!("Created {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("create {}: {error}", path.display());
                    ExitCode::from(2)
                }
            };
        }
    };
    check(root, json)
}

fn registry() -> Result<linter::Registry, linter::Error> {
    linter::register(linter::Registry::default())
        .and_then(linter_rust::register)
        .and_then(linter_c::register)
}

fn check(root: PathBuf, json: bool) -> ExitCode {
    let result = registry().and_then(|registry| registry.check(&root));
    let (code, text) = match result {
        Ok(report) => {
            let code = u8::from(!report.findings.is_empty());
            let text = if json {
                serde_json::to_string_pretty(&report).map(|text| format!("{text}\n"))
            } else {
                Ok(report.to_string())
            };
            (code, text)
        }
        Err(error) => {
            if !json {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
            (
                2,
                serde_json::to_string_pretty(&serde_json::json!({"error": error.to_string()}))
                    .map(|text| format!("{text}\n")),
            )
        }
    };
    match text {
        Ok(text) => match io::stdout().lock().write_all(text.as_bytes()) {
            Ok(()) => ExitCode::from(code),
            Err(error) => {
                eprintln!("write output: {error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("encode report: {error}");
            ExitCode::from(2)
        }
    }
}
