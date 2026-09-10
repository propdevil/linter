#!/usr/bin/env python3
"""Build and install the local Linter skill and MCP bundle into Codex."""

import argparse
import datetime
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


MARKETPLACE = "propdevil-linter"


def catalog():
    return {
        "name": MARKETPLACE,
        "interface": {"displayName": "Propdevil Local"},
        "plugins": [{
            "name": "linter",
            "source": {"source": "local", "path": "./plugins/linter"},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": "Productivity",
        }],
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def validate_destination(root):
    marketplace = root / ".agents/plugins/marketplace.json"
    if marketplace.exists():
        if json.loads(marketplace.read_text(encoding="utf-8")) != catalog():
            raise ValueError(f"Refusing to change a different marketplace: {marketplace}")
    destination = root / "plugins/linter"
    if destination.is_symlink():
        raise ValueError(f"Plugin destination must not be a symlink: {destination}")
    if destination.exists():
        manifest = destination / ".codex-plugin/plugin.json"
        if not manifest.is_file():
            raise ValueError(f"Existing destination is not a Linter plugin: {destination}")
        data = json.loads(manifest.read_text(encoding="utf-8"))
        if data.get("name") != "linter" or data.get("author", {}).get("name") != "Propdevil":
            raise ValueError(f"Existing destination belongs to another plugin: {destination}")
    return marketplace, destination


def build(source):
    command = [
        "cargo", "build", "--release", "--locked", "-p", "linter-mcp",
        "-p", "linter-cli", "--message-format=json",
    ]
    result = subprocess.run(command, cwd=source, stdout=subprocess.PIPE, text=True, check=False)
    binaries = {}
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-message":
            rendered = message.get("message", {}).get("rendered")
            if rendered:
                print(rendered, file=sys.stderr, end="")
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            binaries[message["target"]["name"]] = Path(message["executable"])
    result.check_returncode()
    for name in ("linter", "linter-mcp"):
        if name not in binaries or not binaries[name].is_file():
            raise ValueError(f"Cargo did not produce the {name} executable")
    return binaries


def bundle(source, staging, destination, binaries):
    manifest = json.loads((source / ".codex-plugin/plugin.json").read_text(encoding="utf-8"))
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d%H%M%S%f")
    manifest["version"] = manifest["version"].split("+", 1)[0] + "+codex." + stamp
    write_json(staging / ".codex-plugin/plugin.json", manifest)
    shutil.copytree(source / "skills", staging / "skills")
    (staging / "bin").mkdir()
    for name in ("linter", "linter-mcp"):
        shutil.copy2(binaries[name], staging / "bin" / binaries[name].name)
    server = destination / "bin" / binaries["linter-mcp"].name
    write_json(staging / ".mcp.json", {
        "mcpServers": {"linter": {"command": str(server), "args": []}}
    })


def install(source, root):
    skill = source / "skills/software-design/SKILL.md"
    if not skill.is_file():
        raise ValueError(f"Incomplete checkout: missing {skill}")
    marketplace, destination = validate_destination(root)
    if not shutil.which("codex"):
        raise ValueError("Codex CLI is required; install it and put codex on PATH first")
    binaries = build(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".linter-install-", dir=root) as temporary:
        staging = Path(temporary) / "linter"
        bundle(source, staging, destination, binaries)
        backup = Path(temporary) / "previous"
        if destination.exists():
            destination.rename(backup)
        try:
            staging.rename(destination)
            if not marketplace.exists():
                write_json(marketplace, catalog())
        except BaseException:
            if destination.exists():
                shutil.rmtree(destination)
            if backup.exists():
                backup.rename(destination)
            raise
    # A CLI failure leaves a valid local bundle available for a retry.
    subprocess.run(["codex", "plugin", "marketplace", "add", str(root)], check=True)
    subprocess.run(["codex", "plugin", "add", f"linter@{MARKETPLACE}"], check=True)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path,
        default=Path.home() / ".local/share/propdevil/linter",
        help="Local marketplace directory (default: ~/.local/share/propdevil/linter)",
    )
    args = parser.parse_args()
    root = args.root.expanduser().resolve()
    source = Path(__file__).resolve().parents[1]
    try:
        destination = install(source, root)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Installation failed: {error}", file=sys.stderr)
        return 1
    print(f"Installed skill and MCP server: {destination}")
    print(f"CLI: {destination / 'bin' / ('linter.exe' if os.name == 'nt' else 'linter')}")
    print("Start a new Codex thread to load the skill and MCP tools.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
