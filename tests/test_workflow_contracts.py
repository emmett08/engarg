"""Keep native-only CI selection complete and installed-runtime caches bounded."""

from pathlib import Path
import os
import re
import shlex
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]


def producer_step(action, marker):
    """Read one known composite step without requiring a YAML package in CI."""
    return action.split(marker, 1)[1].split("\n    - ", 1)[0]


class WorkflowContractTests(unittest.TestCase):
    def test_compatibility_producer_matches_every_eal_dependency_pin(self):
        manifest = (ROOT / "Cargo.toml").read_text()
        dependencies = re.findall(r'(?m)^eal-[\w-]+\s*=', manifest)
        matches = re.findall(
            r'(?m)^eal-[\w-]+\s*=\s*\{\s*git\s*=\s*"([^"]+)"\s*,\s*rev\s*=\s*"([^"]+)"',
            manifest,
        )
        self.assertTrue(dependencies)
        self.assertEqual(len(matches), len(dependencies))
        pins = set(matches)
        self.assertEqual(len(pins), 1)
        url, revision = pins.pop()
        self.assertEqual(url, "https://github.com/emmett08/eal-mcp")
        self.assertRegex(revision, r"^[0-9a-f]{40}$")
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        match = re.search(r"(?m)^\s+EAL_REVISION:\s*([0-9a-f]{40})\s*$", workflow)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), revision)

    def test_native_selection_covers_every_ignored_integration_target(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        commands = [
            shlex.split(line.strip().removeprefix("- run: "))
            for line in workflow.replace("\\\n", " ").splitlines()
            if re.search(r"\bcargo(?: \+\S+)? test\b", line)
        ]
        self.assertTrue(any("--all-targets" in command for command in commands))
        native = [command for command in commands if "--ignored" in command]
        self.assertTrue(native)
        self.assertTrue(all("--include-ignored" not in command for command in commands))
        targets = {
            command[index + 1]
            for command in native
            for index, value in enumerate(command[:-1])
            if value == "--test"
        }
        ignored = {
            path.stem
            for path in (ROOT / "tests").glob("*.rs")
            if re.search(r"#\s*\[\s*ignore\b", path.read_text())
        }
        self.assertEqual(targets, ignored)
        # These targets exercise actual direct package and public authoring
        # paths; a deleted Rust module filter would silently run zero tests.
        for target in ["executable_package", "executable_modes", "support_cli"]:
            self.assertTrue(any(target in command for command in native))
        self.assertNotIn("tooling_commands::native_provider", workflow)
        # Retain both the installed-release and producer-debug preflight paths.
        self.assertTrue(any(
            "tooling_docker" in command
            and "generated_interface_native_preflight" in command
            for command in native
        ))

    def test_runtime_cache_contains_only_admitted_payload_and_sensitive_key_inputs(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        checks, compatibility = workflow.split("\n  eal-compatibility:", 1)
        runtime = "${{ runner.temp }}/engarg-eal-runtime/"
        paths = {
            line.strip()[len(runtime):]
            for line in checks.splitlines()
            if line.strip().startswith(runtime)
        }
        self.assertEqual(paths, {
            "bin", ".engarg-eal-runtime-owner.json", ".engarg-eal-runtime.json",
        })
        self.assertNotIn(runtime, compatibility)
        action = (ROOT / ".github/actions/cargo-cache/action.yml").read_text()
        configuration = next(line for line in action.splitlines() if "CACHE_CONFIGURATION:" in line)
        for source in ["Cargo.lock", "**/Cargo.toml", "scripts/install-eal-runtime.sh"]:
            self.assertIn(repr(source), configuration)
        for identity in ["$RUNNER_OS", "$RUNNER_ARCH", "ImageOS", "ImageVersion", "cc --version"]:
            self.assertIn(identity, action)
        self.assertRegex(
            (ROOT / "scripts/install-eal-runtime.sh").read_text(),
            r'(?m)^TOOLCHAIN = "[0-9]+\.[0-9]+\.[0-9]+"$',
        )

    def test_producer_cache_identity_tracks_native_inputs_not_consumer_edits(self):
        action = (ROOT / ".github/actions/eal-producer/action.yml").read_text()
        environment = producer_step(action, "      id: environment\n")
        script = textwrap.dedent(environment.split("      run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            commands = root / "commands"
            commands.mkdir()
            for command, variable in {
                "git": "CHECKOUT_REVISION",
                "rustup": "RUSTC_IDENTITY",
                "cc": "CC_IDENTITY",
            }.items():
                stub = commands / command
                stub.write_text(f'#!/bin/sh\nprintf \'%s\\n\' "${{{variable}}}"\n')
                stub.chmod(0o755)
            defaults = {
                "PATH": str(commands) + os.pathsep + os.defpath,
                "GITHUB_OUTPUT": str(root / "output"),
                "EAL_REVISION": "a" * 40,
                "CHECKOUT_REVISION": "a" * 40,
                "EAL_CONFIGURATION": "producer-manifests-and-action",
                "RUSTC_IDENTITY": "rustc 1.90.0 host x86_64-unknown-linux-gnu",
                "CC_IDENTITY": "cc version 14",
                "RUNNER_OS": "Linux", "RUNNER_ARCH": "X64",
                "ImageOS": "ubuntu24", "ImageVersion": "20261025.1",
                "CARGO_PROFILE_DEV_DEBUG": "line-tables-only",
                "CARGO_PROFILE_TEST_DEBUG": "line-tables-only",
                "CARGO_INCREMENTAL": "0",
            }

            def fingerprint(**changes):
                output = root / "output"
                output.unlink(missing_ok=True)
                result = subprocess.run(
                    ["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", script],
                    cwd=root, env={**defaults, **changes}, capture_output=True, text=True,
                )
                return result, output.read_text() if output.exists() else ""

            result, baseline = fingerprint()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(baseline.startswith("key=Linux-X64-eal-producer-"))
            (root / "Cargo.toml").write_text("# unrelated consumer edit\n")
            self.assertEqual(fingerprint(GITHUB_SHA="b" * 40, CARGO_BUILD_JOBS="2")[1], baseline)
            for changes in [
                {"EAL_REVISION": "c" * 40, "CHECKOUT_REVISION": "c" * 40},
                {"EAL_CONFIGURATION": "changed-producer-or-action"},
                {"RUSTC_IDENTITY": "changed compiler"},
                {"CC_IDENTITY": "changed linker"},
                {"RUNNER_OS": "macOS"}, {"RUNNER_ARCH": "ARM64"},
                {"ImageOS": "ubuntu26"}, {"ImageVersion": "20261026.1"},
                {"CARGO_PROFILE_DEV_DEBUG": "2"},
                {"CARGO_PROFILE_DEV_OPT_LEVEL": "3"},
                {"CARGO_INCREMENTAL": "1"},
                {"RUSTFLAGS": "-C target-cpu=native"},
                {"CARGO_ENCODED_RUSTFLAGS": "-C\x1ftarget-cpu=native"},
                {"CARGO_BUILD_TARGET": "aarch64-unknown-linux-gnu"},
            ]:
                with self.subTest(changes=changes):
                    result, identity = fingerprint(**changes)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertNotEqual(identity, baseline)
            result, identity = fingerprint(CHECKOUT_REVISION="d" * 40)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(identity, "")

    def test_native_workflows_share_producer_cache_without_skipping_cargo(self):
        action = (ROOT / ".github/actions/eal-producer/action.yml").read_text()
        build = producer_step(action, "    - name: Build the pinned native EAL companions\n")
        self.assertNotIn("if:", build)
        self.assertIn("run: cargo +1.90.0 build --workspace --bins --locked", build)
        configuration = next(line for line in action.splitlines() if "EAL_CONFIGURATION:" in line)
        self.assertIn("'.github/actions/eal-producer/action.yml'", configuration)
        self.assertIn("'.cargo/config*'", configuration)
        self.assertNotIn("'.github/workflows/", configuration)
        self.assertNotIn("'Cargo.toml'", configuration)
        self.assertNotIn("github.sha", action)
        self.assertNotIn("restore-keys:", action)
        self.assertLess(action.index("git -C .eal-producer rev-parse HEAD"), action.index("uses: actions/cache/restore@"))
        for name in ["ci.yml", "ticket-acceptance.yml"]:
            workflow = (ROOT / ".github/workflows" / name).read_text()
            with self.subTest(workflow=name):
                self.assertIn("uses: ./.github/actions/eal-producer", workflow)
                self.assertNotIn("repository: emmett08/eal-mcp", workflow)
                self.assertNotRegex(workflow, r"(?m)^\s+\.eal-producer/target$")


if __name__ == "__main__":
    unittest.main()
