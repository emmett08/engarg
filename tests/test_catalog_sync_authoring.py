"""Offline preflight and script orchestration regressions; no build or model calls."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import textwrap
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
        toolchains = self.repo / "examples/toolchains"
        toolchains.mkdir(parents=True)
        shutil.copyfile(SOURCE_SCRIPT.parents[1] / "examples/toolchains/rust.json",
                        toolchains / "rust.json")
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

    def write_toolchain(self, language="rust"):
        path = self.inputs.with_name("operator toolchain.json")
        profile = {
            "schema": "engarg.toolchain/1", "language": language,
            "docker": "must-not-run-docker", "image": "operator-fixture:local",
            "build_argv": ["rustc", "/src/collector.rs", "-o", "/build/collector"],
            "run_argv": ["/build/collector"], "inherit_env": [],
            "runtime_network": "none", "timeout_seconds": 30,
            "memory_mb": 128, "output_bytes": 1_048_576,
        }
        path.write_text(json.dumps(profile, indent=4) + "\n\n")
        return path

    def install_pipeline_stubs(self):
        """Replace CLI boundaries only; script control flow and artifacts stay real."""
        (self.commands / 'dirname').symlink_to(shutil.which('dirname'))
        target = self.root / "stub target"
        release = target / "release"
        release.mkdir(parents=True)
        self.log = self.root / "engarg invocations.jsonl"
        self.env.update(FAKE_ENGARG_LOG=str(self.log), FAKE_TARGET=str(target),
                        OPENAI_API_KEY="synthetic-key-must-not-be-used",
                        ENGARG_EAL_HOST=str(self.commands / "eal-host"))
        for path in [self.commands / name for name in [
            "rustc", "eal-host", "eal-mcp", "eal-method-worker", "eal-command-supervisor"
        ]] + [release / "engarg-process-supervisor"]:
            path.write_text("#!/usr/bin/env bash\nprintf 'offline fixture\\n'\n")
            path.chmod(0o700)
        cargo = self.commands / "cargo"
        cargo.write_text(textwrap.dedent("""\
            #!/usr/bin/env python3
            import json, os, sys
            if sys.argv[1] == 'metadata':
                print(json.dumps({'target_directory': os.environ['FAKE_TARGET'],
                                  'packages': [{'name': 'engarg', 'dependencies': []}]}))
            elif sys.argv[1] != 'build':
                raise SystemExit('Unexpected cargo operation')
            """))
        cargo.chmod(0o700)
        binary = release / "engarg"
        binary.write_text(textwrap.dedent(r"""
            #!/usr/bin/env python3
            import json, os, pathlib, sys
            args = sys.argv[1:]
            with open(os.environ['FAKE_ENGARG_LOG'], 'a') as log:
                log.write(json.dumps(args) + '\n')
            def option(name):
                return args[args.index(name) + 1]
            def write(path, value):
                pathlib.Path(path).write_text(json.dumps(value) + '\n')
            if args[:2] == ['eal', 'describe']:
                write(option('--output'), {'is_error': False, 'result': {
                    'source_syntax': 'EAL/3', 'methods': {'structured/1': {}}}})
            elif args[0] == 'analyse':
                request = json.loads(pathlib.Path(option('--input')).read_text())
                report = {'request': request, 'argument': {'status': 'supported'}, 'transcript': []}
                if '--implement-tools' in args:
                    directory = pathlib.Path(option('--tool-output-dir'))
                    assert not directory.exists(), 'Tool output must be a new directory'
                    directory.mkdir()
                    write(directory / 'argument.analysis.json', report)
                    failed = bool(os.environ.get('FAIL_TOOLING'))
                    write(directory / 'tooling.audit.jsonl', {'event': 'finished', 'passed': not failed})
                    if not failed:
                        write(directory / 'collectors.json', {'schema': 'eal.collector-catalogue/1'})
                        report['tooling'] = {
                            'schema': 'engarg.tooling/1', 'language': option('--tool-language'),
                            'model_calls': 2, 'packages': [{'need_id': 'E1', 'reused': False}],
                            'audit_path': str(directory / 'tooling.audit.jsonl')}
                    write(option('--output'), report)
                    if failed:
                        raise SystemExit(3)
                else:
                    write(option('--output'), report)
            elif args[0] == 'validate':
                pass
            elif args[0] == 'render':
                pathlib.Path(option('--output')).write_text('offline rendered fixture\n')
            elif args[:2] == ['eal', 'export']:
                report = json.loads(pathlib.Path(option('--input')).read_text())
                tool_id = 'generated_fixture' if report.get('tooling') else 'http_json_get'
                pathlib.Path(option('--output')).write_text('offline EAL fixture\n')
                pathlib.Path(option('--tools-output')).write_text('offline registry fixture\n')
                write(option('--manifest'), {'acquisition_plan': {'entries': [
                    {'tool_id': tool_id, 'unresolved_arguments': []}]}, 'obligations': []})
            elif args[:2] == ['eal', 'validate']:
                write(option('--output'), {'is_error': False, 'result': {'valid': True}})
            elif args[:2] == ['eal', 'plan']:
                write(option('--output'), {'is_error': False, 'result': {}})
            else:
                raise SystemExit('Unexpected EngArg operation')
            """).lstrip())
        binary.chmod(0o700)

    def pipeline_directory(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Authoring, rendering, export and EAL planning passed", result.stdout)
        return Path(result.stdout.split("Artifacts: ", 1)[1].splitlines()[0])

    def invocations(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

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

    def test_tool_generation_check_only_keeps_input_only_dependencies(self):
        self.make_dirty()
        before = self.tracked_state()
        result = self.run_script("--implement-tools", "--check-only", self.inputs)
        directory = self.successful_run_directory(result)
        self.assertEqual(self.tracked_state(), before)
        profile = self.repo / "examples/toolchains/rust.json"
        self.assertEqual((directory / "toolchain.snapshot.json").read_bytes(), profile.read_bytes())
        implementation = json.loads((directory / "input-identity.json").read_text())["tool_implementation"]
        self.assertTrue(implementation["enabled"])
        self.assertEqual(implementation["language"], "rust")
        self.assertFalse((directory / "run-settings.json").exists())
        self.assertFalse((directory / "tooling").exists())

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

    def test_implementation_preflight_snapshots_profile_without_docker_or_model(self):
        profile = self.write_toolchain()
        original = profile.read_bytes()
        directory = self.successful_run_directory(self.run_script(
            "--implement-tools", "--check-only", "--tool-language", "rust",
            "--toolchain", profile, self.inputs,
        ))
        identity = json.loads((directory / "input-identity.json").read_text())['tool_implementation']
        self.assertTrue(identity['enabled'])
        self.assertEqual(identity['language'], 'rust')
        self.assertEqual(identity['max_calls'], 16)
        self.assertEqual(identity['timeout_seconds'], 3600)
        self.assertEqual(identity['toolchain_sha256'], hashlib.sha256(original).hexdigest())
        self.assertEqual((directory / 'toolchain.snapshot.json').read_bytes(), original)
        self.assertEqual(stat.S_IMODE((directory / 'toolchain.snapshot.json').stat().st_mode), 0o600)
        self.assertEqual(Path(identity['store']).resolve(), (self.repo / '.engarg-local/tool-store').resolve())
        self.assertFalse((self.repo / '.engarg-local/tool-store').exists())
        self.assertFalse((directory / 'tooling').exists())

    def test_inactive_implementation_options_are_rejected(self):
        for options in [
            ['--tool-language', 'rust'],
            ['--toolchain', 'unused.json'],
            ['--tool-store', 'unused-store'],
            ['--tool-max-calls', '4'],
            ['--tool-timeout-secs', '60'],
        ]:
            with self.subTest(options=options):
                result = self.run_script('--check-only', *options, self.inputs)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('require', result.stderr)
                self.assertFalse((self.repo / 'engarg-runs').exists())

    def test_explicit_tool_settings_override_environment_defaults(self):
        profile = self.write_toolchain()
        store = self.root / 'explicit store'
        env = dict(self.env, ENGARG_TOOLCHAIN='missing-profile.json',
                   ENGARG_TOOL_STORE='unused-store', ENGARG_TOOL_MAX_CALLS='invalid',
                   ENGARG_TOOL_TIMEOUT_SECS='invalid')
        directory = self.successful_run_directory(self.run_script(
            '--check-only', '--implement-tools', '--toolchain', profile,
            '--tool-store', store, '--tool-max-calls', '4', '--tool-timeout-secs', '60',
            self.inputs, env=env,
        ))
        implementation = json.loads((directory / 'input-identity.json').read_text())['tool_implementation']
        self.assertTrue(profile.samefile(implementation['operator_toolchain']))
        self.assertEqual(Path(implementation['store']).resolve(), store.resolve())
        self.assertEqual(implementation['max_calls'], 4)
        self.assertEqual(implementation['timeout_seconds'], 60)

    def test_check_only_rejects_invalid_tooling_environment_without_runtime_tools(self):
        for overrides in [
            {'ENGARG_TOOLCHAIN': 'missing-profile.json'},
            {'ENGARG_TOOL_MAX_CALLS': 'invalid'},
            {'ENGARG_TOOL_TIMEOUT_SECS': '0'},
        ]:
            with self.subTest(overrides=overrides):
                result = self.run_script('--check-only', '--implement-tools', self.inputs,
                                         env=dict(self.env, **overrides))
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('Traceback', result.stderr)
        self.assertFalse(list((self.repo / 'engarg-runs').glob('*/request.json')))

    def test_invalid_implementation_settings_fail_input_preflight(self):
        profile = self.write_toolchain()
        for extra in [
            ['--tool-language', 'python'], ['--tool-max-calls', '0'],
            ['--tool-max-calls', '257'], ['--tool-timeout-secs', '0'],
            ['--tool-timeout-secs', '86401'], ['--tool-max-calls', 'invalid'],
        ]:
            with self.subTest(extra=extra):
                result = self.run_script(
                    '--check-only', '--implement-tools', '--tool-language', 'rust',
                    '--toolchain', profile, *extra, self.inputs,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Input preflight failed', result.stderr)
                self.assertNotIn('Traceback', result.stderr)
        self.assertFalse(list((self.repo / 'engarg-runs').glob('*/request.json')))

    def test_relative_toolchain_and_store_use_invocation_directory(self):
        profile = self.write_toolchain()
        directory = self.successful_run_directory(self.run_script(
            '--check-only', '--implement-tools', '--tool-language', 'rust',
            '--toolchain', '../.engarg-local/' + profile.name,
            '--tool-store', '../shared tool store', '../.engarg-local/collector-inputs.json',
            cwd=self.repo / 'scripts',
        ))
        identity = json.loads((directory / 'input-identity.json').read_text())['tool_implementation']
        self.assertTrue(profile.samefile(identity['operator_toolchain']))
        self.assertEqual(Path(identity['store']).resolve(), (self.repo / 'shared tool store').resolve())

    def test_full_implementation_exports_enriched_analysis_and_retains_separate_budget(self):
        profile = self.write_toolchain()
        self.install_pipeline_stubs()
        store = self.root / 'shared verified store'
        directory = self.pipeline_directory(self.run_script(
            '--implement-tools', '--tool-language', 'rust', '--toolchain', profile,
            '--tool-store', store, '--tool-max-calls', '4', '--tool-timeout-secs', '7200',
            self.inputs,
        ))
        calls = self.invocations()
        analysis = next(call for call in calls if call[0] == 'analyse')
        def argument(name):
            return analysis[analysis.index(name) + 1]
        self.assertIn('--implement-tools', analysis)
        self.assertEqual(argument('--tool-language'), 'rust')
        self.assertEqual(argument('--tool-store'), str(store))
        self.assertEqual(argument('--toolchain'), str(directory / 'toolchain.snapshot.json'))
        self.assertEqual(argument('--tool-output-dir'), str(directory / 'tooling'))
        self.assertEqual(argument('--tool-max-calls'), '4')
        self.assertEqual(argument('--tool-timeout-secs'), '7200')
        self.assertEqual(argument('--max-calls'), '7')
        self.assertEqual(argument('--timeout-secs'), '10800')
        export = next(call for call in calls if call[:2] == ['eal', 'export'])
        self.assertEqual(export[export.index('--input') + 1], str(directory / 'analysis.json'))
        report = json.loads((directory / 'analysis.json').read_text())
        self.assertEqual(report['tooling']['schema'], 'engarg.tooling/1')
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertFalse(summary['evidence_collected'])
        self.assertEqual(summary['selected_collectors'], ['generated_fixture'])
        self.assertEqual(summary['tool_implementation']['model_calls'], 2)
        self.assertEqual(summary['tool_implementation']['verified_packages'], 1)
        self.assertEqual(summary['tool_implementation']['reused_packages'], 0)
        settings = json.loads((directory / 'run-settings.json').read_text())
        self.assertEqual(settings['tool_implementation']['max_calls'], 4)
        self.assertEqual(settings['tool_implementation']['toolchain_sha256'],
                         hashlib.sha256(profile.read_bytes()).hexdigest())
        self.assertNotIn('synthetic-key', json.dumps(calls))

    def test_argument_only_pipeline_does_not_enable_implementation(self):
        self.install_pipeline_stubs()
        directory = self.pipeline_directory(self.run_script(self.inputs))
        analysis = next(call for call in self.invocations() if call[0] == 'analyse')
        self.assertNotIn('--implement-tools', analysis)
        self.assertNotIn('--toolchain', analysis)
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertEqual(summary['tool_implementation'], {'enabled': False})
        self.assertFalse((directory / 'tooling').exists())

    def test_implementation_failure_preserves_analysis_and_stops_before_export(self):
        profile = self.write_toolchain()
        self.install_pipeline_stubs()
        result = self.run_script(
            '--implement-tools', '--tool-language', 'rust', '--toolchain', profile, self.inputs,
            env=dict(self.env, FAIL_TOOLING='1'),
        )
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertIn('during model analysis and tool implementation', result.stderr)
        directory = next((self.repo / 'engarg-runs').iterdir())
        self.assertTrue((directory / 'analysis.json').is_file())
        audit = json.loads((directory / 'tooling/tooling.audit.jsonl').read_text())
        self.assertFalse(audit['passed'])
        self.assertFalse((directory / 'argument.bundle.json').exists())
        self.assertFalse((directory / 'run-summary.json').exists())
        self.assertFalse(any(call[:2] == ['eal', 'export'] for call in self.invocations()))

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
