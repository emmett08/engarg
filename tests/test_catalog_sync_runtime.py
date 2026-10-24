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
        self.analyse_args = self.root / "analyse arguments.json"
        self.docker_trace = self.root / "docker arguments.json"
        dirname = shutil.which("dirname")
        self.assertIsNotNone(dirname)
        (self.commands / "dirname").symlink_to(dirname)
        self.env = dict(
            self.fixture.env,
            ENGARG_EAL_HOST=str(self.prefix / "eal-host"),
            OPENAI_API_KEY="synthetic-key-must-not-be-reported",
            STUB_TRACE=str(self.trace),
            STUB_ANALYSE_ARGS=str(self.analyse_args),
            STUB_DOCKER_TRACE=str(self.docker_trace),
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
    expected = os.environ.get('STUB_EXPECT_HOST')
    if expected and args[args.index('--eal-host') + 1] != expected:
        raise SystemExit('selected the wrong host installation')
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
    pathlib.Path(os.environ['STUB_ANALYSE_ARGS']).write_text(json.dumps(args))
    request = json.loads(pathlib.Path(args[args.index('--input') + 1]).read_text())
    report = {'request': request, 'argument': {'status': 'supported'}, 'transcript': []}
    if '--implement-tools' in args:
        directory = pathlib.Path(args[args.index('--tool-output-dir') + 1])
        assert not directory.exists()
        directory.mkdir()
        report['tooling'] = {
            'schema': 'engarg.tooling/1', 'language': args[args.index('--tool-language') + 1],
            'model_calls': 2, 'packages': [{'need_id': 'E1', 'reused': False}],
            'audit_path': str(directory / 'tooling.audit.jsonl')}
        (directory / 'tooling.audit.jsonl').write_text(json.dumps({'event': 'finished', 'passed': True}))
        (directory / 'collectors.json').write_text(json.dumps({'schema': 'eal.collector-catalogue/1'}))
    pathlib.Path(args[args.index('--output') + 1]).write_text(json.dumps(report))
    # This sentinel stops the script after the synthetic response, before export.
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

    def installer(self, source):
        path = self.fixture.repo / "scripts/install-eal-runtime.sh"
        self.executable(path, source)
        self.fixture.git("add", str(path))
        self.fixture.git("commit", "-qm", "Add managed runtime fixture")

    def test_default_runtime_ignores_incompatible_python_tools_on_path(self):
        for name in ["eal-host", "eal-mcp", "eal-method-worker", "eal-process-supervisor"]:
            self.executable(self.commands / name, """#!/usr/bin/env python3
import json
print(json.dumps({'is_error': True, 'error': 'methods must have the form package.module:function'}))
raise SystemExit(2)
""")
        self.installer('#!/usr/bin/env bash\nprintf \'%s\\n\' "$STUB_MANAGED_HOST"\n')
        host = str((self.prefix / "eal-host").resolve())
        result = self.run_script(
            ENGARG_EAL_HOST="", STUB_MANAGED_HOST=host, STUB_EXPECT_HOST=host,
        )
        self.assert_probe_precedes_inference(result)
        settings = json.loads((self.run_directory() / "run-settings.json").read_text())
        self.assertEqual(settings["eal_host_selection"], "managed")
        self.assertEqual(settings["eal_host_path"], host)
        self.assertEqual(settings["model"], "gpt-6.1-sol")
        self.assertIn("Selected EAL host (managed): " + host, result.stderr)

    def test_explicit_host_bypasses_managed_installation(self):
        self.installer("#!/usr/bin/env bash\nexit 89\n")
        result = self.run_script()
        self.assert_probe_precedes_inference(result)
        settings = json.loads((self.run_directory() / "run-settings.json").read_text())
        self.assertEqual(settings["eal_host_selection"], "operator")
        self.assertIn(str((self.prefix / "eal-host").resolve()), result.stderr)

    def test_failed_managed_installation_stops_before_host_or_model_call(self):
        self.installer("#!/usr/bin/env bash\nexit 89\n")
        result = self.run_script(ENGARG_EAL_HOST="")
        self.assertEqual(result.returncode, 89, result.stdout + result.stderr)
        self.assertEqual(self.invocations(), [])
        self.assertIn("during EAL runtime installation", result.stderr)

    def prepare_tooling(self, docker=True):
        examples = self.fixture.repo / "examples/toolchains"
        (examples / "rust").mkdir()
        (examples / "rust/Dockerfile").write_text("# Trusted build context fixture\n")
        self.fixture.git("add", "examples")
        self.fixture.git("commit", "-qm", "Add Rust tooling fixture")
        if docker:
            self.executable(self.commands / "docker", """#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ['STUB_DOCKER_TRACE']).write_text(json.dumps(sys.argv[1:]))
failed_build = sys.argv[1] == 'build' and os.environ.get('STUB_DOCKER_FAIL') == '1'
failed_daemon = sys.argv[1] == 'info' and os.environ.get('STUB_DOCKER_OFFLINE') == '1'
raise SystemExit(86 if failed_build or failed_daemon else 0)
""")

    def run_with_tooling(self, *options, **overrides):
        return self.fixture.run_script("--implement-tools", *options, self.fixture.inputs,
                                       env=dict(self.env, **overrides))

    def test_default_full_run_has_no_tooling_flags_or_docker_requirement(self):
        result = self.run_script(ENGARG_TOOLCHAIN="missing-profile.json")
        self.assert_probe_precedes_inference(result)
        arguments = json.loads(self.analyse_args.read_text())
        self.assertNotIn("--implement-tools", arguments)
        self.assertNotIn("--tool-language", arguments)
        self.assertFalse(self.docker_trace.exists())
        settings = json.loads((self.run_directory() / "run-settings.json").read_text())
        self.assertFalse(settings["tool_implementation"]["enabled"])

    def test_tool_generation_prepares_rust_and_passes_separate_budgets_and_store(self):
        self.prepare_tooling()
        result = self.run_with_tooling(ENGARG_TOOL_MAX_CALLS="23", ENGARG_TOOL_TIMEOUT_SECS="90")
        self.assert_probe_precedes_inference(result)
        arguments = json.loads(self.analyse_args.read_text())
        value = lambda name: arguments[arguments.index(name) + 1]
        self.assertIn("--implement-tools", arguments)
        self.assertEqual(value("--tool-language"), "rust")
        self.assertEqual(value("--tool-max-calls"), "23")
        self.assertEqual(value("--tool-timeout-secs"), "90")
        directory = self.run_directory().resolve()
        self.assertEqual(value("--tool-output-dir"), str(directory / "tooling"))
        self.assertEqual(value("--tool-store"), str(self.fixture.repo.resolve() / ".engarg-local/tool-store"))
        self.assertEqual(value("--toolchain"), str(directory / "toolchain.snapshot.json"))
        snapshot = json.loads((directory / "toolchain.snapshot.json").read_text())
        self.assertEqual(snapshot["language"], "rust")
        self.assertEqual(json.loads(self.docker_trace.read_text()), [
            "build", "--tag", "engarg-rust-tools:1.99.1", str(self.fixture.repo.resolve() / "examples/toolchains/rust"),
        ])
        settings = json.loads((directory / "run-settings.json").read_text())
        self.assertEqual(settings["tool_implementation"]["language"], "rust")
        self.assertEqual(settings["tool_implementation"]["max_calls"], 23)
        self.assertEqual(settings["model"], "gpt-6.1-sol")

    def test_custom_rust_profile_and_store_bypass_default_image_build(self):
        self.prepare_tooling(docker=False)
        profile = self.root / "custom rust.json"
        profile.write_text((self.fixture.repo / "examples/toolchains/rust.json").read_text())
        store = self.root / "reused tool packages"
        result = self.run_with_tooling(ENGARG_TOOLCHAIN=str(profile), ENGARG_TOOL_STORE=str(store))
        self.assert_probe_precedes_inference(result)
        arguments = json.loads(self.analyse_args.read_text())
        self.assertEqual(arguments[arguments.index("--tool-store") + 1], str(store))
        self.assertFalse(self.docker_trace.exists())
        self.assertEqual((self.run_directory() / "toolchain.snapshot.json").read_bytes(), profile.read_bytes())

    def test_explicit_cli_profile_bypasses_default_image_build(self):
        self.prepare_tooling(docker=False)
        profile = self.root / "CLI Rust profile.json"
        profile.write_bytes((self.fixture.repo / "examples/toolchains/rust.json").read_bytes())
        result = self.run_with_tooling("--toolchain", profile, ENGARG_TOOLCHAIN="missing-env-profile.json")
        self.assert_probe_precedes_inference(result)
        self.assertFalse(self.docker_trace.exists())
        self.assertEqual((self.run_directory() / "toolchain.snapshot.json").read_bytes(), profile.read_bytes())

    def test_missing_docker_for_default_rust_tools_stops_before_inference(self):
        self.prepare_tooling(docker=False)
        result = self.run_with_tooling()
        self.assert_rejected_before_inference(result, "Docker is required")
        self.assertEqual(self.invocations(), [])
        self.assertIn("during Rust tooling prerequisites", result.stderr)

    def test_stopped_docker_daemon_stops_before_host_or_model_call(self):
        self.prepare_tooling()
        result = self.run_with_tooling(STUB_DOCKER_OFFLINE="1")
        self.assert_rejected_before_inference(result, "Docker must be running")
        self.assertEqual(self.invocations(), [])
        self.assertIn("during Rust tooling prerequisites", result.stderr)

    def test_failed_rust_image_build_stops_before_inference(self):
        self.prepare_tooling()
        result = self.run_with_tooling(STUB_DOCKER_FAIL="1")
        self.assertEqual(result.returncode, 86)
        self.assertEqual(self.invocations(), ["eal describe"])
        self.assertIn("during Rust tooling image preparation", result.stderr)

    def test_invalid_tooling_budget_is_rejected_before_inference(self):
        self.prepare_tooling()
        result = self.run_with_tooling(ENGARG_TOOL_MAX_CALLS="257")
        self.assert_rejected_before_inference(result, "Input preflight failed")
        self.assertEqual(self.invocations(), [])
        self.assertFalse(self.docker_trace.exists())

    def test_default_rust_language_rejects_a_nonmatching_operator_profile(self):
        self.prepare_tooling()
        profile = self.root / "other language.json"
        value = json.loads((self.fixture.repo / "examples/toolchains/rust.json").read_text())
        value["language"] = "python"
        profile.write_text(json.dumps(value))
        result = self.run_with_tooling(ENGARG_TOOLCHAIN=str(profile))
        self.assert_rejected_before_inference(result, "Input preflight failed")
        self.assertEqual(self.invocations(), [])
        self.assertFalse(self.docker_trace.exists())

    def test_matching_explicit_language_and_profile_remain_available(self):
        self.prepare_tooling(docker=False)
        profile = self.fixture.write_toolchain(language="python")
        result = self.run_with_tooling("--tool-language", "python", "--toolchain", profile)
        self.assert_probe_precedes_inference(result)
        arguments = json.loads(self.analyse_args.read_text())
        self.assertEqual(arguments[arguments.index("--tool-language") + 1], "python")
        self.assertFalse(self.docker_trace.exists())

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
