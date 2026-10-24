"""Offline regressions for operator-input preflight; no build or model calls."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest


SOURCE_SCRIPT = Path(__file__).resolve().parents[1] / "scripts/run-catalog-sync-authoring.sh"


class CatalogSyncAuthoringTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="engarg authoring tests ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "checkout"
        self.repo.mkdir()
        self.commands = self.root / "commands"
        self.commands.mkdir()
        # A successful preflight cannot accidentally find Rust, EAL or model tools.
        for name in ["bash", "git", "python3", "mkdir", "mktemp", "date"]:
            executable = shutil.which(name)
            self.assertIsNotNone(executable, "test prerequisite: " + name)
            (self.commands / name).symlink_to(executable)
        self.env = {
            "PATH": str(self.commands),
            "HOME": str(self.root),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_AUTHOR_NAME": "Script regression test",
            "GIT_AUTHOR_EMAIL": "test@example.invalid",
            "GIT_COMMITTER_NAME": "Script regression test",
            "GIT_COMMITTER_EMAIL": "test@example.invalid",
        }
        self.script = self.repo / "scripts/run-catalog-sync-authoring.sh"
        self.script.parent.mkdir()
        shutil.copyfile(SOURCE_SCRIPT, self.script)
        (self.repo / ".gitignore").write_text("/engarg-runs/\n/.engarg-local/\n")
        for number in range(4):
            (self.repo / ("tracked-%s.txt" % number)).write_text("original\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("commit", "-qm", "Initial regression fixture")
        self.inputs = self.repo / ".engarg-local/collector-inputs.json"
        self.inputs.parent.mkdir()
        self.query = '{ span.rollout.name = "catalog-sync-operator" }\n'
        self.configuration = {
            "kubernetes": {
                "context": "dev",
                "resource": "applications.example.invalid",
                "kind": "Application",
                "namespace": "development",
                "name": "catalog-sync-operator",
            },
            "grafana": {
                "origin": "https://grafana.example.invalid",
                "datasource_uid": "tempo-test",
                "query": self.query,
                "start_unix": 1_700_000_000,
                "end_unix": 1_700_000_060,
                "private_setting": "DO_NOT_SEND_PRIVATE_CONFIGURATION",
            },
            "private_setting": "DO_NOT_SEND_PRIVATE_CONFIGURATION",
        }
        self.write_inputs()

    def git(self, *arguments):
        return subprocess.check_output(
            [str(self.commands / "git"), *arguments],
            cwd=self.repo,
            env=self.env,
        )

    def write_inputs(self):
        self.inputs.write_text(json.dumps(self.configuration, indent=2) + "\n")

    def run_script(self, *arguments, cwd=None, env=None):
        return subprocess.run(
            [str(self.commands / "bash"), str(self.script), *map(str, arguments)],
            cwd=cwd or self.repo,
            env=env or self.env,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def successful_run_directory(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Input preflight passed", result.stdout)
        return Path(result.stdout.split("Artifacts: ", 1)[1].strip())

    def make_dirty(self):
        for number in range(4):
            (self.repo / ("tracked-%s.txt" % number)).write_text("edited %s\n" % number)
        self.git("add", "tracked-0.txt", "tracked-1.txt")

    def tracked_state(self):
        return (
            self.git("status", "--porcelain"),
            self.git("diff", "--binary"),
            self.git("diff", "--cached", "--binary"),
            [(path.name, path.read_bytes()) for path in sorted(self.repo.glob("tracked-*.txt"))],
        )

    def test_dirty_preflight_needs_only_input_tools_and_preserves_tracked_edits(self):
        self.make_dirty()
        before = self.tracked_state()
        for unavailable in ["cargo", "rustc", "eal-host", "eal-collector", "engarg"]:
            self.assertIsNone(shutil.which(unavailable, path=self.env["PATH"]))
        self.assertNotIn("OPENAI_API_KEY", self.env)
        self.assertNotIn("ENGARG_EAL_HOST", self.env)

        directory = self.successful_run_directory(self.run_script("--check-only", self.inputs))

        self.assertEqual(self.tracked_state(), before)
        self.assertEqual(self.git("check-ignore", str(directory)).strip(), str(directory).encode())
        self.assertFalse((directory / "analysis.json").exists())
        self.assertFalse((directory / "run-settings.json").exists())

    def test_omitted_catalogue_preserves_literal_query_and_private_snapshots(self):
        original = self.inputs.read_bytes()
        directory = self.successful_run_directory(self.run_script("--check-only", self.inputs))
        request_raw = (directory / "request.json").read_text()
        request = json.loads(request_raw)
        self.assertTrue(request["claim"].endswith(self.query))
        self.assertEqual(request["claim"].count(self.query), 1)
        self.assertEqual(request["evidence"], [])
        self.assertNotIn("DO_NOT_SEND_PRIVATE_CONFIGURATION", request_raw)
        self.assertEqual((directory / "collector-inputs.snapshot.json").read_bytes(), original)
        identity = json.loads((directory / "input-identity.json").read_text())
        self.assertEqual(identity["catalogue_selection"], "engarg built-in")
        self.assertIsNone(identity["operator_catalogue"])
        self.assertIsNone(identity["catalogue_sha256"])
        self.assertFalse((directory / "collectors.snapshot.json").exists())
        self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
        for artifact in directory.iterdir():
            self.assertEqual(stat.S_IMODE(artifact.stat().st_mode), 0o600, artifact.name)

    def test_explicit_catalogue_retains_exact_bytes_and_identity(self):
        catalogue = self.inputs.with_name("selected.collectors.json")
        descriptor = {
            "schema": "eal.collector-catalogue/1",
            "acquisition_contract": "eal.acquisition/1",
            "collectors": [{
                "id": "test_capture",
                "version": "1",
                "purpose": "Read a retained test capture",
                "parameters": ["capture"],
                "input_schema": {
                    "type": "object", "additionalProperties": False,
                    "properties": {"capture": {"type": "string", "minLength": 1}},
                    "required": ["capture"],
                },
                "result_schema": {
                    "type": "object", "additionalProperties": False,
                    "properties": {"collected": {"type": "boolean"}},
                    "required": ["collected"],
                },
                "command": {
                    "argv": ["{operator.collector}", "--capture", "{capture}"],
                    "inherit_env": [], "timeout_seconds": 30,
                    "max_output_bytes": 1_048_576, "parallel_safe": False,
                },
                "acquisition_predicate": "collected == true", "constraints": [],
            }],
        }
        raw = (json.dumps(descriptor, indent=4) + "\n\n").encode()
        catalogue.write_bytes(raw)
        directory = self.successful_run_directory(
            self.run_script("--check-only", self.inputs, catalogue)
        )
        self.assertEqual((directory / "collectors.snapshot.json").read_bytes(), raw)
        identity = json.loads((directory / "input-identity.json").read_text())
        self.assertEqual(identity["catalogue_selection"], "external")
        self.assertEqual(identity["catalogue_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(identity["operator_catalogue"], str(catalogue))
        self.assertEqual(catalogue.read_bytes(), raw)

    def test_missing_explicit_catalogue_explains_optional_default(self):
        missing = self.inputs.with_name("approved.collectors.json")
        result = self.run_script("--check-only", self.inputs, missing)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Collector catalogue does not exist", result.stderr)
        self.assertIn("Omit CATALOGUE.json", result.stderr)
        self.assertIn("built-in planning tools", result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        self.assertFalse((self.repo / "engarg-runs").exists())

    def test_input_preflight_ignores_model_only_settings(self):
        env = dict(self.env, ENGARG_MAX_REPAIRS="not-a-model-budget",
                   ENGARG_MAX_OUTPUT_TOKENS="not-a-token-budget",
                   ENGARG_EAL_HOST="/missing/host")
        directory = self.successful_run_directory(
            self.run_script("--check-only", self.inputs, env=env)
        )
        self.assertTrue((directory / "request.json").is_file())
        self.assertFalse((directory / "run-settings.json").exists())

    def test_relative_input_path_is_relative_to_invocation_directory(self):
        # macOS /var aliases /private/var. Exercise an aliased parent on every
        # platform: subprocess cwd uses its physical path, while operator paths
        # can retain a different spelling of the same admitted regular file.
        alias = self.root / "checkout-alias"
        alias.symlink_to(self.repo, target_is_directory=True)
        directory = self.successful_run_directory(
            self.run_script("--check-only", "../.engarg-local/collector-inputs.json", cwd=alias / "scripts")
        )
        identity = json.loads((directory / "input-identity.json").read_text())
        self.assertTrue((alias / ".engarg-local/collector-inputs.json").samefile(identity["operator_inputs"]))

    def test_invalid_scope_fails_without_request_or_traceback(self):
        for field, value in [("context", "production"), ("namespace", "")]:
            with self.subTest(field=field):
                original = self.configuration["kubernetes"][field]
                self.configuration["kubernetes"][field] = value
                self.write_inputs()
                result = self.run_script("--check-only", self.inputs)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Input preflight failed", result.stderr)
                self.assertNotIn("Traceback", result.stderr)
                self.configuration["kubernetes"][field] = original
        self.assertFalse(list((self.repo / "engarg-runs").glob("*/request.json")))

    @unittest.skipUnless(hasattr(os, "O_NOFOLLOW"), "requires no-follow file admission")
    def test_input_symlink_is_rejected_instead_of_resolved_before_admission(self):
        alias = self.inputs.with_name("alias.json")
        alias.symlink_to(self.inputs)
        result = self.run_script("--check-only", alias)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Input preflight failed", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_full_run_rejects_dirty_source_before_build_or_inference(self):
        self.make_dirty()
        before = self.tracked_state()
        marker = self.root / "unexpected-invocations"
        for command in ["cargo", "rustc", "eal-host", "eal-mcp", "engarg", "eal-collector"]:
            path = self.commands / command
            path.write_text(
                "#!/usr/bin/env bash\n"
                'printf "%s\\n" "$0" >> "$UNEXPECTED_INVOCATIONS"\n'
                "exit 99\n"
            )
            path.chmod(0o700)
        env = dict(self.env, UNEXPECTED_INVOCATIONS=str(marker),
                   ENGARG_EAL_HOST=str(self.commands / "eal-host"),
                   OPENAI_API_KEY="synthetic-key-must-not-be-used")

        result = self.run_script(self.inputs, env=env)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Full runs require a clean tracked checkout", result.stderr)
        self.assertIn("--check-only accepts tracked changes", result.stderr)
        self.assertNotIn("synthetic-key", result.stdout + result.stderr)
        self.assertFalse(marker.exists(), "dirty source must fail before invoking build or provider tools")
        self.assertEqual(self.tracked_state(), before)


if __name__ == "__main__":
    unittest.main()
