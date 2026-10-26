"""Offline regressions for coherent, revision-pinned runtime installation."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/install-eal-runtime.sh"
REVISION = "a" * 40
URL = "https://github.com/example/fixture-eal"
BINARIES = {"eal-host", "eal-mcp", "eal-method-worker",
            "eal-command-supervisor", "eal-process-supervisor", "eal-collector", "eal-coordinate", "eal-mas-server", "mas-protocol-codec",
            "eal", "eal-core", "eal-syntax", "eal-runtime", "eal-aspic-solve"}


class InstallEalRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="engarg runtime installation ")
        self.addCleanup(self.temporary.cleanup)
        # The installer reports physical paths, including macOS's /private/var.
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root / "checkout"
        (self.repo / "scripts").mkdir(parents=True)
        self.script = self.repo / "scripts/install-eal-runtime.sh"
        shutil.copyfile(SCRIPT, self.script)
        (self.repo / "Cargo.toml").write_text("# Offline metadata fixture\n")
        self.commands = self.root / "commands"
        self.commands.mkdir()
        for name in ("bash", "python3"):
            executable = shutil.which(name)
            self.assertIsNotNone(executable)
            (self.commands / name).symlink_to(executable)
        self.trace = self.root / "commands.jsonl"
        self.metadata = self.root / "metadata.json"
        self.write_metadata()
        self.env = {"PATH": str(self.commands), "HOME": str(self.root),
                    "STUB_TRACE": str(self.trace), "STUB_METADATA": str(self.metadata)}
        self.stub(self.commands / "cargo", """#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with pathlib.Path(os.environ['STUB_TRACE']).open('a') as trace:
    trace.write(json.dumps({'program': 'cargo', 'args': args,
                            'target': os.environ.get('CARGO_TARGET_DIR')}) + '\\n')
if args[0] == 'metadata':
    print(pathlib.Path(os.environ['STUB_METADATA']).read_text())
    raise SystemExit(0)
if args[:2] != ['+1.90.0', 'build']:
    raise SystemExit(91)
packages = [args[index + 1] for index, value in enumerate(args) if value == '--package']
if os.environ.get('STUB_BLOCK_PACKAGE') in packages:
    import time
    pathlib.Path(os.environ['STUB_CHILD_PID']).write_text(str(os.getpid()))
    release = pathlib.Path(os.environ['STUB_RELEASE'])
    while not release.exists():
        time.sleep(0.02)
if os.environ.get('STUB_FAIL_PACKAGE') in packages:
    raise SystemExit(42)
directory = pathlib.Path(os.environ['CARGO_TARGET_DIR']) / 'release'
directory.mkdir(parents=True, exist_ok=True)
expected = {'eal-process', 'eal-runtime', 'eal-mcp', 'eal-collectors', 'eal-coordination', 'eal-cli'}
if set(packages) != expected or len(packages) != len(expected):
    raise SystemExit(92)
names = ['eal-process-supervisor', 'eal-method-worker', 'eal-command-supervisor',
         'eal-host', 'eal-mcp', 'eal-collector', 'eal-coordinate', 'eal-mas-server', 'mas-protocol-codec', 'eal', 'eal-core', 'eal-syntax', 'eal-runtime', 'eal-aspic-solve']
for name in names:
    if os.environ.get('STUB_MISSING_BINARY') == name:
        continue
    binary = directory / name
    binary.write_text('#!/bin/sh\\n# Pinned fixture ' + name + '\\nexit 0\\n')
    binary.chmod(0o700)
    artifact = str(binary)
    if os.environ.get('STUB_OUTSIDE_ARTIFACT') == name:
        artifact = str(directory.parent.parent.parent / name)
    print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': name, 'kind': ['bin']},
                      'executable': artifact}))
print('Synthetic compiler progress on stderr', file=sys.stderr)
""")
        self.stub(self.commands / "git", """#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with pathlib.Path(os.environ['STUB_TRACE']).open('a') as trace:
    trace.write(json.dumps({'program': 'git', 'args': args}) + '\\n')
if args[-2:] == ['rev-parse', 'HEAD']:
    print(os.environ.get('STUB_SOURCE_REVISION', 'a' * 40))
""")
        self.stub(self.commands / "rustup", """#!/usr/bin/env python3
import json, os, pathlib, sys
with pathlib.Path(os.environ['STUB_TRACE']).open('a') as trace:
    trace.write(json.dumps({'program': 'rustup', 'args': sys.argv[1:]}) + '\\n')
print('Synthetic toolchain progress on stdout')
""")

    @staticmethod
    def stub(path, source):
        path.write_text(source)
        path.chmod(0o700)

    def write_metadata(self, second_revision=None):
        dependencies = []
        for name in ("eal-collector-contracts", "eal-process", "eal-adapters"):
            revision = second_revision if name == "eal-process" and second_revision else REVISION
            dependencies.append({"name": name, "source": "git+" + URL + "?rev=" + revision})
        self.metadata.write_text(json.dumps({"packages": [{"name": "engarg", "dependencies": dependencies}]}))

    def run_script(self, *args, **environment):
        return subprocess.run(
            [str(self.commands / "bash"), str(self.script), *map(str, args)],
            cwd=self.root, env=dict(self.env, **environment), capture_output=True,
            text=True, timeout=10,
        )

    def calls(self, program=None):
        records = [json.loads(line) for line in self.trace.read_text().splitlines()]
        return [record for record in records if program is None or record["program"] == program]

    def installs(self):
        return [record for record in self.calls("cargo") if record["args"][0] == "+1.90.0"]

    def default_prefix(self):
        return self.repo / ".engarg-local" / "eal-runtime" / REVISION

    def assert_success(self, result, prefix):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, str(prefix / "bin/eal-host") + "\n")
        receipt = json.loads((prefix / ".engarg-eal-runtime.json").read_text())
        self.assertEqual(receipt["git"], URL)
        self.assertEqual(receipt["revision"], REVISION)
        self.assertEqual(set(receipt["binaries"]), BINARIES)
        for name, digest in receipt["binaries"].items():
            self.assertEqual(digest, hashlib.sha256((prefix / "bin" / name).read_bytes()).hexdigest())
        self.assertTrue((prefix / ".engarg-install.lock").is_file())

    def test_default_install_uses_metadata_pin_and_coherent_isolated_prefix(self):
        # Old Python tools remain untouched even when they precede every native tool.
        stale = "#!/bin/sh\nexit 99\n"
        for name in ("eal-host", "eal-mcp"):
            self.stub(self.commands / name, stale)
        result = self.run_script()
        prefix = self.default_prefix()
        self.assert_success(result, prefix)
        self.assertIn("Synthetic compiler progress", result.stderr)
        self.assertIn("Synthetic toolchain progress", result.stderr)
        for name in ("eal-host", "eal-mcp"):
            self.assertEqual((self.commands / name).read_text(), stale)
        calls = self.calls()
        self.assertEqual(calls[0]["args"], ["metadata", "--locked", "--no-deps", "--format-version", "1",
                                          "--manifest-path", str(self.repo / "Cargo.toml")])
        self.assertEqual(self.calls("rustup"), [{"program": "rustup", "args": [
            "toolchain", "install", "1.90.0", "--profile", "minimal"]}])
        builds = self.installs()
        self.assertEqual(len(builds), 1)
        command = builds[0]['args']
        source = Path(command[command.index('--manifest-path') + 1]).parent
        self.assertEqual(source.parent, prefix)
        self.assertFalse(source.exists(), 'temporary source checkout must be removed')
        self.assertEqual(command, [
            '+1.90.0', 'build', '--manifest-path', str(source / 'Cargo.toml'),
            '--locked', '--release', '--bins', '--message-format=json-render-diagnostics',
            '--package', 'eal-process', '--package', 'eal-runtime',
            '--package', 'eal-mcp', '--package', 'eal-collectors',
            '--package', 'eal-coordination', '--package', 'eal-cli'])
        git = [record['args'][2:] for record in self.calls('git')]
        self.assertEqual(git, [['init', '--quiet'], ['remote', 'add', 'origin', URL],
                              ['fetch', '--quiet', '--depth', '1', '--no-tags', 'origin', REVISION],
                              ['checkout', '--quiet', '--detach', 'FETCH_HEAD'], ['rev-parse', 'HEAD']])
        self.assertEqual({call["target"] for call in self.installs()}, {str(prefix / "target")})

    def test_valid_cache_checks_binaries_without_reinstalling_or_requiring_rustup(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        original_calls = self.calls()
        (self.commands / "rustup").unlink()
        (self.commands / "git").unlink()
        self.assert_success(self.run_script(), prefix)
        self.assertEqual(self.calls()[:-1], original_calls)
        self.assertEqual(self.calls()[-1]["args"][0], "metadata")
        self.assertEqual(len(self.installs()), 1)

    def test_wrong_checkout_identity_stops_before_build_or_success_receipt(self):
        result = self.run_script(STUB_SOURCE_REVISION='b' * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('differs from the admitted revision', result.stderr)
        self.assertEqual(self.installs(), [])
        self.assertFalse((self.default_prefix() / '.engarg-eal-runtime.json').exists())

    def test_build_must_report_all_six_binaries_inside_the_owned_target(self):
        for environment in [dict(STUB_MISSING_BINARY='eal-collector'),
                            dict(STUB_OUTSIDE_ARTIFACT='eal-host')]:
            with self.subTest(environment=environment):
                result = self.run_script(**environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, '')
                self.assertFalse((self.default_prefix() / '.engarg-eal-runtime.json').exists())
        self.assert_success(self.run_script(), self.default_prefix())

    def test_tampered_or_missing_companion_reinstalls_every_package(self):
        prefix = self.root / "custom EAL prefix"
        self.assert_success(self.run_script("--root", prefix), prefix)
        (prefix / "bin/eal-host").write_text("unverified replacement\n")
        (prefix / "bin/eal-command-supervisor").unlink()
        self.assert_success(self.run_script("--root", prefix), prefix)
        self.assertEqual(len(self.installs()), 2)

    def test_partial_install_has_no_success_receipt_and_can_be_retried(self):
        prefix = self.default_prefix()
        result = self.run_script(STUB_FAIL_PACKAGE="eal-runtime")
        self.assertEqual(result.returncode, 42, result.stdout + result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse((prefix / ".engarg-eal-runtime.json").exists())
        self.assertTrue((prefix / ".engarg-install.lock").is_file())
        self.assert_success(self.run_script(), prefix)

    def test_missing_collector_invalidates_runtime_cache_and_reinstalls_same_pin(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        (prefix / 'bin/eal-collector').unlink()
        self.assert_success(self.run_script(), prefix)
        self.assertEqual(len(self.installs()), 2)

    def test_failed_repair_invalidates_the_old_receipt(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        (prefix / "bin/eal-method-worker").unlink()
        result = self.run_script(STUB_FAIL_PACKAGE="eal-runtime")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((prefix / ".engarg-eal-runtime.json").exists())

    def test_nonempty_unmanaged_prefix_is_preserved(self):
        prefix = self.root / "global installation"
        (prefix / "bin").mkdir(parents=True)
        old = prefix / "bin/eal-host"
        old.write_text("preserve installed Python host\n")
        result = self.run_script("--root", prefix)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unmanaged", result.stderr)
        self.assertEqual(old.read_text(), "preserve installed Python host\n")
        self.assertEqual(self.installs(), [])
        self.assertFalse((prefix / ".engarg-eal-runtime-owner.json").exists())

    def test_inconsistent_eal_revisions_fail_before_installation(self):
        self.write_metadata(second_revision="b" * 40)
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("different Git URLs or revisions", result.stderr)
        self.assertEqual(self.installs(), [])
        self.assertFalse(self.default_prefix().exists())

    def test_missing_required_dependency_fails_even_with_other_eal_dependencies(self):
        metadata = json.loads(self.metadata.read_text())
        metadata["packages"][0]["dependencies"] = [
            dependency for dependency in metadata["packages"][0]["dependencies"]
            if dependency["name"] != "eal-collector-contracts"]
        self.metadata.write_text(json.dumps(metadata))
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing", result.stderr)
        self.assertEqual(self.installs(), [])
        self.assertFalse(self.default_prefix().exists())

    def test_interrupted_lock_holder_does_not_require_manual_lock_removal(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        (prefix / "bin/eal-host").write_text("needs replacement\n")
        process = subprocess.Popen(
            [sys.executable, "-c", "import fcntl,sys,time; "
             "lock=open(sys.argv[1],'a+b'); fcntl.flock(lock,fcntl.LOCK_EX); "
             "print('locked',flush=True); time.sleep(60)",
             str(prefix / ".engarg-install.lock")],
            stdout=subprocess.PIPE, text=True,
        )
        try:
            self.assertEqual(process.stdout.readline().strip(), "locked")
            result = self.run_script()
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Another EAL installation", result.stderr)
        finally:
            process.kill()
            process.wait(timeout=5)
            process.stdout.close()
        self.assert_success(self.run_script(), prefix)

    def test_linked_bin_directory_cannot_redirect_repair_into_another_installation(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        original = self.root / "global binaries"
        shutil.move(str(prefix / "bin"), original)
        (prefix / "bin").symlink_to(original, target_is_directory=True)
        before = {path.name: path.read_bytes() for path in original.iterdir()}
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("regular directory", result.stderr)
        self.assertEqual({path.name: path.read_bytes() for path in original.iterdir()}, before)
        self.assertEqual(len(self.installs()), 1)
        self.assertFalse((prefix / ".engarg-eal-runtime.json").exists())

    def test_linked_companion_is_replaced_without_changing_the_external_target(self):
        prefix = self.default_prefix()
        self.assert_success(self.run_script(), prefix)
        external = self.root / "old Python host"
        external.write_text("preserve external installed command\n")
        binary = prefix / "bin/eal-host"
        binary.unlink()
        binary.symlink_to(external)
        self.assert_success(self.run_script(), prefix)
        self.assertEqual(external.read_text(), "preserve external installed command\n")
        self.assertFalse(binary.is_symlink())

    def test_cargo_keeps_the_lock_when_only_its_helper_is_terminated(self):
        child_pid = self.root / "cargo child.pid"
        release = self.root / "release cargo"
        helper = subprocess.Popen(
            [str(self.commands / "bash"), str(self.script)],
            cwd=self.root, env=dict(self.env, STUB_BLOCK_PACKAGE="eal-process",
                                   STUB_CHILD_PID=str(child_pid), STUB_RELEASE=str(release)),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            deadline = time.monotonic() + 5
            while not child_pid.exists() and time.monotonic() < deadline:
                self.assertIsNone(helper.poll(), "helper exited before Cargo started")
                time.sleep(0.02)
            self.assertTrue(child_pid.exists(), "blocking Cargo did not start")
            helper.terminate()
            helper.wait(timeout=5)
            result = self.run_script()
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Another EAL installation", result.stderr)
            self.assertEqual(len(self.installs()), 1)
        finally:
            if helper.poll() is None:
                helper.terminate()
                helper.wait(timeout=5)
            release.touch()
            # Cargo inherits the captured stderr descriptor. EOF confirms the
            # orphaned child has completed and released the installation lock.
            helper.communicate(timeout=5)
        self.assert_success(self.run_script(), self.default_prefix())


if __name__ == "__main__":
    unittest.main()
