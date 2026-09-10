"""Installer tests use temporary bundles and never alter a Codex profile."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import install


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="linter installer ")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.source = self.base / "checkout"
        self.root = self.base / "distribution"
        skill = self.source / "skills/software-design/SKILL.md"
        skill.parent.mkdir(parents=True)
        skill.write_text(
            "---\nname: software-design\ndescription: Review design.\n---\nGuidance.\n"
        )
        install.write_json(self.source / ".codex-plugin/plugin.json", {
            "name": "linter", "version": "0.1.0", "author": {"name": "Propdevil"},
            "skills": "./skills/", "mcpServers": "./.mcp.json",
        })
        self.binaries = {}
        for name in ("linter", "linter-mcp"):
            path = self.base / name
            path.write_text("native executable stand-in")
            path.chmod(0o755)
            self.binaries[name] = path

    def provision(self):
        with patch.object(install, "build", return_value=self.binaries), \
                patch.object(install.shutil, "which", return_value="codex"), \
                patch.object(install.subprocess, "run") as run:
            destination = install.install(self.source, self.root)
        return destination, run

    def test_bundles_both_components_with_absolute_server_command(self):
        destination, run = self.provision()
        server = json.loads((destination / ".mcp.json").read_text())["mcpServers"]["linter"]
        self.assertEqual(server["command"], str(destination / "bin/linter-mcp"))
        self.assertTrue((destination / "bin/linter").is_file())
        self.assertTrue((destination / "skills/software-design/SKILL.md").is_file())
        self.assertEqual(run.call_args_list[0].args[0], [
            "codex", "plugin", "marketplace", "add", str(self.root),
        ])
        self.assertEqual(run.call_args_list[1].args[0], [
            "codex", "plugin", "add", "linter@propdevil-linter",
        ])

    def test_reinstall_preserves_catalog_and_refreshes_skill_and_cache(self):
        destination, _ = self.provision()
        catalog = self.root / ".agents/plugins/marketplace.json"
        original_catalog = catalog.read_bytes()
        manifest = destination / ".codex-plugin/plugin.json"
        original_version = json.loads(manifest.read_text())["version"]
        (self.source / "skills/software-design/SKILL.md").write_text("Updated guidance")
        self.provision()
        self.assertEqual(catalog.read_bytes(), original_catalog)
        self.assertNotEqual(json.loads(manifest.read_text())["version"], original_version)
        self.assertEqual((destination / "skills/software-design/SKILL.md").read_text(),
                         "Updated guidance")

    def test_foreign_marketplace_is_rejected_before_build(self):
        install.write_json(self.root / ".agents/plugins/marketplace.json", {"name": "foreign"})
        with patch.object(install, "build") as build, self.assertRaises(ValueError):
            install.install(self.source, self.root)
        build.assert_not_called()

    def test_failed_build_preserves_previous_installation(self):
        destination, _ = self.provision()
        previous = (destination / ".codex-plugin/plugin.json").read_bytes()
        with patch.object(install.shutil, "which", return_value="codex"), \
                patch.object(install, "build", side_effect=ValueError("build failed")), \
                self.assertRaisesRegex(ValueError, "build failed"):
            install.install(self.source, self.root)
        self.assertEqual((destination / ".codex-plugin/plugin.json").read_bytes(), previous)

    def test_codex_failure_leaves_valid_bundle_for_retry(self):
        with patch.object(install, "build", return_value=self.binaries), \
                patch.object(install.shutil, "which", return_value="codex"), \
                patch.object(install.subprocess, "run",
                             side_effect=subprocess.CalledProcessError(1, "codex")), \
                self.assertRaises(subprocess.CalledProcessError):
            install.install(self.source, self.root)
        self.assertTrue((self.root / "plugins/linter/bin/linter-mcp").is_file())
        self.provision()

    def test_cargo_artifacts_respect_actual_output_directory(self):
        artifacts = "\n".join(json.dumps({
            "reason": "compiler-artifact", "target": {"name": name}, "executable": str(path),
        }) for name, path in self.binaries.items())
        result = subprocess.CompletedProcess([], 0, stdout=artifacts)
        with patch.object(install.subprocess, "run", return_value=result) as run:
            self.assertEqual(install.build(self.source), self.binaries)
        self.assertIn("--locked", run.call_args.args[0])
        self.assertIn("--release", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
