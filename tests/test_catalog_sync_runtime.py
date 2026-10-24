"""Offline checks that host admission precedes paid argument generation."""

import json
import shutil
import unittest

import test_catalog_sync_authoring as authoring


class CatalogSyncRuntimeTests(unittest.TestCase):
    def setUp(self):
        # Compose the existing fixture without inheriting its unrelated tests.
        self.fixture = authoring.CatalogSyncAuthoringTests()
        self.addCleanup(self.fixture.doCleanups)
        self.fixture.setUp()
        self.root = self.fixture.root
        self.commands = self.fixture.commands
        self.target = self.root / "build target"
        self.release = self.target / "release"
        self.release.mkdir(parents=True)
        self.prefix = self.root / "EAL installation"
        self.prefix.mkdir()
        self.trace = self.root / "engarg invocations.jsonl"
        dirname = shutil.which("dirname")
        self.assertIsNotNone(dirname)
        (self.commands / "dirname").symlink_to(dirname)
        self.env = dict(
            self.fixture.env,
            ENGARG_EAL_HOST=str(self.prefix / "eal-host"),
            OPENAI_API_KEY="synthetic-key-must-not-be-reported",
            STUB_TRACE=str(self.trace),
        )
        self.executable(self.commands / "cargo", """#!/usr/bin/env python3
import json, sys
if sys.argv[1] == 'build':
    raise SystemExit(0)
if sys.argv[1] == 'metadata':
    print(json.dumps({'target_directory': %r, 'packages': []}))
    raise SystemExit(0)
raise SystemExit(90)
""" % str(self.target))
        self.executable(self.commands / "rustc", "#!/usr/bin/env python3\nprint('offline test compiler')\n")
        self.executable(self.release / "engarg-process-supervisor")
        self.executable(self.release / "engarg", """#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
operation = 'eal describe' if args[:2] == ['eal', 'describe'] else args[0]
with pathlib.Path(os.environ['STUB_TRACE']).open('a') as stream:
    stream.write(json.dumps(operation) + '\\n')
if operation == 'eal describe':
    destination = pathlib.Path(args[args.index('--output') + 1])
    failed = os.environ.get('STUB_DESCRIBE_FAIL') == '1'
    result = {'is_error': True, 'error': 'synthetic MCP initialization failure'} if failed else {
        'operation': 'describe', 'protocol_version': '2025-11-25', 'is_error': False,
        'result': {'source_syntax': os.environ.get('STUB_SOURCE_SYNTAX', 'EAL/3'),
                   'languages': ['EAL/3'],
                   'methods': {} if os.environ.get('STUB_NO_STRUCTURED') == '1' else {'structured/1': {}},
                   'implementation_version': 'test'},
        'content': []}
    if not failed and os.environ.get('STUB_NULL_METHODS') == '1':
        result['result']['methods'] = None
    destination.write_text(json.dumps(result))
    raise SystemExit(1 if failed else 0)
if operation == 'analyse':
    # This sentinel deliberately stops the script before any model response.
    raise SystemExit(97)
raise SystemExit(91)
""")
        for name in ["eal-host", "eal-mcp", "eal-method-worker", "eal-process-supervisor"]:
            self.executable(self.prefix / name)

    @staticmethod
    def executable(path, source="#!/usr/bin/env bash\nexit 88\n"):
        path.write_text(source)
        path.chmod(0o700)

    def run_script(self, **overrides):
        return self.fixture.run_script(self.fixture.inputs, env=dict(self.env, **overrides))

    def invocations(self):
        if not self.trace.exists():
            return []
        return [json.loads(line) for line in self.trace.read_text().splitlines()]

    def run_directory(self):
        directories = list((self.fixture.repo / "engarg-runs").iterdir())
        self.assertEqual(len(directories), 1)
        return directories[0]

    def assert_rejected_before_inference(self, result, diagnostic):
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(diagnostic, result.stderr)
        self.assertNotIn("analyse", self.invocations())
        self.assertNotIn("synthetic-key", result.stdout + result.stderr)

    def assert_probe_precedes_inference(self, result):
        self.assertEqual(result.returncode, 97, result.stdout + result.stderr)
        self.assertEqual(self.invocations(), ["eal describe", "analyse"])
        response = json.loads((self.run_directory() / "eal-preflight.json").read_text())
        self.assertIs(response["is_error"], False)

    def test_missing_method_worker_fails_before_model_call(self):
        (self.prefix / "eal-method-worker").unlink()
        self.assert_rejected_before_inference(self.run_script(), "eal-method-worker")

    def test_missing_supervisor_fails_before_model_call(self):
        (self.prefix / "eal-process-supervisor").unlink()
        self.assert_rejected_before_inference(self.run_script(), "supervisor")

    def test_nonexecutable_legacy_supervisor_cannot_fall_back_to_canonical(self):
        legacy = self.prefix / "eal-command-supervisor"
        self.executable(legacy)
        legacy.chmod(0o600)
        self.assert_rejected_before_inference(self.run_script(), "eal-command-supervisor")

    def test_canonical_supervisor_and_complete_host_are_probed_before_model_call(self):
        self.assert_probe_precedes_inference(self.run_script())

    def test_legacy_supervisor_is_supported_without_canonical_companion(self):
        (self.prefix / "eal-process-supervisor").unlink()
        self.executable(self.prefix / "eal-command-supervisor")
        self.assert_probe_precedes_inference(self.run_script())

    def test_directory_named_like_legacy_supervisor_does_not_hide_canonical(self):
        (self.prefix / "eal-command-supervisor").mkdir()
        self.assert_probe_precedes_inference(self.run_script())

    def test_server_alias_uses_companions_beside_its_physical_executable(self):
        server_prefix = self.root / "physical server installation"
        server_prefix.mkdir()
        for name in ["eal-mcp", "eal-method-worker", "eal-process-supervisor"]:
            (self.prefix / name).unlink()
            self.executable(server_prefix / name)
        (self.prefix / "eal-mcp").symlink_to(server_prefix / "eal-mcp")
        self.assert_probe_precedes_inference(self.run_script())

    def test_failed_host_probe_retains_phase_and_response_path_without_model_call(self):
        result = self.run_script(STUB_DESCRIBE_FAIL="1")
        self.assert_rejected_before_inference(result, "preflight")
        self.assertEqual(self.invocations(), ["eal describe"])
        report = self.run_directory() / "eal-preflight.json"
        self.assertIn(str(report.resolve()), result.stderr)
        self.assertEqual(json.loads(report.read_text()), {
            "is_error": True, "error": "synthetic MCP initialization failure",
        })

    def test_incompatible_source_syntax_stops_before_model_call(self):
        result = self.run_script(STUB_SOURCE_SYNTAX="EAL/2")
        self.assert_rejected_before_inference(result, "EAL/3")
        self.assertEqual(self.invocations(), ["eal describe"])
        self.assertIn(str((self.run_directory() / "eal-preflight.json").resolve()), result.stderr)

    def test_missing_structured_method_stops_before_model_call(self):
        result = self.run_script(STUB_NO_STRUCTURED="1")
        self.assert_rejected_before_inference(result, "structured/1")
        self.assertEqual(self.invocations(), ["eal describe"])
        self.assertIn(str((self.run_directory() / "eal-preflight.json").resolve()), result.stderr)

    def test_null_method_catalogue_is_rejected_without_traceback_or_model_call(self):
        result = self.run_script(STUB_NULL_METHODS="1")
        self.assert_rejected_before_inference(result, "structured/1")
        self.assertEqual(self.invocations(), ["eal describe"])
        self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
