"""Keep native-only CI selection complete and installed-runtime caches bounded."""

from pathlib import Path
import re
import shlex
import unittest


ROOT = Path(__file__).resolve().parents[1]


class WorkflowContractTests(unittest.TestCase):
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
        self.assertTrue(any(
            "--bin" in command and "engarg" in command
            and "tooling_commands::native_provider" in command
            for command in native
        ))
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


if __name__ == "__main__":
    unittest.main()
