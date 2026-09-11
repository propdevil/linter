# Linter

Software design guidance and repository linting for **Codex** and **Claude Code**.
One installation gives your agent the design skill, MCP server, and configuration
presets. No Python, Rust, or source checkout is needed.

## Install

**Codex**

```sh
curl -fsSL https://github.com/propdevil/linter/releases/latest/download/install.sh | sh -s -- codex
```

**Claude Code**

```sh
curl -fsSL https://github.com/propdevil/linter/releases/latest/download/install.sh | sh -s -- claude
```

**Both**

```sh
curl -fsSL https://github.com/propdevil/linter/releases/latest/download/install.sh | sh -s -- both
```

Have your selected client CLI (`codex` or `claude`) installed first, then run the
command in a terminal. Start a **new client session** afterward.

Supports macOS and Linux on ARM64 and x86-64. Uses standard shell tools, `curl`,
`tar`, and `shasum` or `sha256sum`. The installer downloads a prebuilt release,
verifies its SHA-256 checksum, and registers the skill and MCP server. It does
not build anything or change your project's files.


## Use

Open your project in your client and ask:

> Use the linter software-design skill. Inspect this project, configure
> linter.toml for its structure, and run the linter. Explain the findings before
> changing the design.

Claude Code also supports `/linter:software-design`.

The agent can read the Rust, C, or generic preset through MCP, tailor
`linter.toml` to your project, and call `check` to validate saved files. Existing
configuration is preserved. Findings include evidence and repair instructions.
Unconfigured rules are not a clean bill of health: make sure the intended rules ran.

See [the design skill](skills/software-design/SKILL.md) for rule examples.

## Update and options

Rerun the installation command to download the latest release. Restart your client.
Files live under `~/.local/share/propdevil/linter`; keep this directory because
the server runs from it. The installer prints the standalone CLI's full path.

Pin a version by adding `--version v0.1.1` after the client argument.
Choose an installation directory with `--root /absolute/path/to/linter`.
Omit the client argument to install for every supported client on PATH.

If installation fails, fix the reported error and rerun the same command.
Use `codex plugin list` or `claude plugin list` to confirm the plugin is installed.

## Publish a release

Maintainers need Rust only to develop and test the project. GitHub Actions builds
and packages all four platform binaries, the skill, presets, and plugin metadata.
It publishes the installer and `SHA256SUMS` with the release assets.

Set the workspace version in `Cargo.toml`, commit it, and push a matching tag:

```sh
git tag v0.1.1
git push origin v0.1.1
```

The workflow runs tests, formatting, Clippy, installer tests, and self-lint before
publishing. A manually triggered workflow builds downloadable Actions artifacts
without publishing a release. Pull requests and pushes to `main` run validation.
