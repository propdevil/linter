use clap::{Args, ValueEnum};
use serde_json::json;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const CODEX: &str = concat!(
    "{\"name\":\"propdevil-linter\",\"interface\":{\"displayName\":\"",
    "Propdevil Linter\"},\"plugins\":[{\"name\":\"linter\",\"source\"",
    ":{\"source\":\"local\",\"path\":\"./plugins/linter\"},\"policy\":",
    "{\"installation\":\"AVAILABLE\",\"authentication\":\"ON_INSTAL",
    "L\"},\"category\":\"Productivity\"}]}\n",
);
const CLAUDE: &str = concat!(
    "{\"name\":\"propdevil-linter\",\"owner\":{\"name\":\"Propdevil\"}",
    ",\"plugins\":[{\"name\":\"linter\",\"source\":\"./plugins/linter",
    "\"}]}\n",
);
const CATALOGS: &[(&str, &str)] = &[
    (".agents/plugins/marketplace.json", CODEX),
    (".claude-plugin/marketplace.json", CLAUDE),
];

#[derive(Clone, Copy, ValueEnum)]
enum Client {
    Auto,
    Codex,
    Claude,
    Both,
}

impl Client {
    fn commands(self) -> io::Result<Vec<PathBuf>> {
        let names: &[&str] = match self {
            Self::Auto | Self::Both => &["codex", "claude"],
            Self::Codex => &["codex"],
            Self::Claude => &["claude"],
        };
        let mut commands = Vec::new();
        for name in names {
            let path = env::split_paths(&env::var_os("PATH").unwrap_or_default())
                .map(|directory| directory.join(name))
                .find(|path| executable(path));
            match path {
                Some(path) => commands.push(path),
                None if matches!(self, Self::Auto) => {}
                None => return Err(io::Error::other(format!("{name} is not on PATH"))),
            }
        }
        if commands.is_empty() {
            return Err(io::Error::other("Install Codex CLI or Claude Code first"));
        }
        Ok(commands)
    }
}

#[derive(Args)]
pub(crate) struct Installer {
    #[arg(value_enum, default_value = "auto")]
    client: Client,
    /// Installation directory; defaults to LINTER_HOME or ~/.local/share/propdevil/linter.
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long, default_value = "propdevil/linter")]
    repo: String,
}

impl Installer {
    pub(crate) fn run(self) -> ExitCode {
        match self.install() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("install: {error}");
                ExitCode::from(2)
            }
        }
    }

    fn root(&self) -> io::Result<PathBuf> {
        self.root
            .clone()
            .or_else(|| env::var_os("LINTER_HOME").map(PathBuf::from))
            .or_else(|| {
                env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local/share/propdevil/linter"))
            })
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| io::Error::other("Specify --root or set HOME"))
    }

    fn install(&self) -> io::Result<()> {
        let commands = self.client.commands()?;
        let binary = env::current_exe()?;
        let root = self.root()?;
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let bundle = Bundle {
            root,
            repo: &self.repo,
        };
        bundle.prepare(&binary)?;
        for command in commands {
            register(&command, &bundle.root)?;
        }
        println!(
            concat!(
                "Installed the design skill and MCP server.\nCLI: {}\n",
                "Start a new client session to use the plugin."
            ),
            bundle.root.join("plugins/linter/bin/linter").display()
        );
        Ok(())
    }
}

struct Bundle<'a> {
    root: PathBuf,
    repo: &'a str,
}

impl Bundle<'_> {
    fn validate(&self) -> io::Result<()> {
        let parts: Vec<_> = self.repo.split('/').collect();
        if parts.len() != 2
            || parts
                .iter()
                .any(|part| part.is_empty() || *part == "." || *part == "..")
            || !self
                .repo
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
        {
            return Err(io::Error::other("Invalid GitHub repository"));
        }
        for relative in ["plugins", "plugins/linter"] {
            if fs::symlink_metadata(self.root.join(relative))
                .is_ok_and(|meta| meta.file_type().is_symlink())
            {
                return Err(io::Error::other("Plugin path cannot be a symlink"));
            }
        }
        let destination = self.root.join("plugins/linter");
        if destination.exists()
            && fs::read_to_string(destination.join(".linter-install"))
                .ok()
                .as_deref()
                != Some(&format!("{}\n", self.repo))
        {
            return Err(io::Error::other(format!(
                "Refusing to replace an unrelated directory: {}",
                destination.display()
            )));
        }
        for (path, contents) in CATALOGS {
            let path = self.root.join(path);
            if path.exists() && fs::read_to_string(&path)? != *contents {
                return Err(io::Error::other(format!(
                    "Unrelated marketplace: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    fn prepare(&self, binary: &Path) -> io::Result<()> {
        self.validate()?;
        let mcp = binary.with_file_name("linter-mcp");
        if !executable(&mcp) {
            return Err(io::Error::other(concat!(
                "Missing executable linter-mcp beside linter; ",
                "use the release bundle or build both applications"
            )));
        }
        let staging = tempfile::Builder::new()
            .prefix(".linter-install.")
            .tempdir_in(&self.root)?;
        let payload = staging.path().join("linter");
        fs::create_dir_all(payload.join("bin"))?;
        fs::copy(binary, payload.join("bin/linter"))?;
        fs::copy(mcp, payload.join("bin/linter-mcp"))?;
        self.assets(&payload)?;
        fs::create_dir_all(self.root.join("plugins"))?;
        let destination = self.root.join("plugins/linter");
        let previous = staging.path().join("previous");
        if destination.exists() {
            fs::rename(&destination, &previous)?;
        }
        if let Err(error) = fs::rename(&payload, &destination) {
            if previous.exists()
                && let Err(rollback) = fs::rename(&previous, &destination)
            {
                let retained = staging.keep();
                return Err(io::Error::other(format!(
                    "Install failed: {error}; rollback failed: {rollback}; backup: {}",
                    retained.join("previous").display()
                )));
            }
            return Err(error);
        }
        // Keep the installed bundle if client registration fails, so it can be retried.
        for (path, contents) in CATALOGS {
            write(&self.root, path, contents)?;
        }
        Ok(())
    }

    fn assets(&self, payload: &Path) -> io::Result<()> {
        let files = [
            (
                "skills/software-design/SKILL.md",
                include_str!("../../../skills/software-design/SKILL.md"),
            ),
            (
                ".codex-plugin/plugin.json",
                include_str!("../../../.codex-plugin/plugin.json"),
            ),
            (
                ".claude-plugin/plugin.json",
                include_str!("../../../.claude-plugin/plugin.json"),
            ),
            ("README.md", include_str!("../../../README.md")),
            ("configs/default.toml", super::Preset::Default.contents()),
            ("configs/rust.toml", super::Preset::Rust.contents()),
            ("configs/c.toml", super::Preset::C.contents()),
        ];
        for (path, contents) in files {
            write(payload, path, contents)?;
        }
        write(payload, ".linter-install", &format!("{}\n", self.repo))?;
        let command = self.root.join("plugins/linter/bin/linter-mcp");
        let manifest = json!({"mcpServers": {"linter": {"command": command, "args": []}}});
        write(payload, ".mcp.json", &manifest.to_string())
    }
}

fn write(root: &Path, relative: &str, contents: &str) -> io::Result<()> {
    let path = root.join(relative);
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("Missing parent directory"))?,
    )?;
    fs::write(path, contents)
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn register(command: &Path, root: &Path) -> io::Result<()> {
    let status = Command::new(command)
        .args(["plugin", "marketplace", "add"])
        .arg(root)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "{} marketplace registration failed: {status}",
            command.display()
        )));
    }
    let operations: &[&[&str]] = if command.file_name().is_some_and(|name| name == "codex") {
        &[&["plugin", "add", "linter@propdevil-linter"]]
    } else {
        &[
            &[
                "plugin",
                "install",
                "linter@propdevil-linter",
                "--scope",
                "user",
            ],
            &[
                "plugin",
                "update",
                "linter@propdevil-linter",
                "--scope",
                "user",
            ],
        ]
    };
    for arguments in operations {
        let status = Command::new(command)
            .args(*arguments)
            .stdin(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "{} plugin registration failed: {status}",
                command.display()
            )));
        }
    }
    Ok(())
}
