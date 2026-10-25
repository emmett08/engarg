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

    def run_script(self, *arguments, cwd=None, env=None, graph=False):
        # These existing regressions explicitly exercise the legacy workflow.
        # Graph migration tests opt into the real script default below.
        arguments = arguments if graph else ('--legacy-flat', *arguments)
        return subprocess.run(
            [str(self.commands / "bash"), str(self.script), *map(str, arguments)],
            cwd=cwd or self.repo,
            env=env or self.env,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def write_evidence_config(self, collector="managed"):
        connections = self.inputs.with_name("evidence-connections.json")
        connections.write_text(json.dumps({"schema": "eal.evidence-config/1", "connections": [],
                                           "private_setting": "DO_NOT_SEND_CONNECTION_CONFIGURATION"}))
        path = self.inputs.with_name("evidence-planning.json")
        config = {"schema": "engarg.capability-planning-config/1", "collector": collector,
                  "connections": connections.name, "store": "source-store", "environment": [],
                  "ranking": {"dimensions": ["execution_requests"]},
                  "scheduling": {"maximum_concurrent_sources": 4, "per_provider": {}}, "bindings": []}
        path.write_text(json.dumps(config, indent=2) + "\n")
        return path

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
            "rustc", "eal-host", "eal-mcp", "eal-method-worker", "eal-command-supervisor", "eal-collector"
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
                graph = '--evidence-planning' in args
                if graph:
                    elaboration = {'schema': 'engarg.support-elaboration/1', 'requirements': [{'id': 'R1'}]}
                    routes = {'schema': 'engarg.evidence-routes/1', 'routes': [{'acquisition_id': 'query', 'selected': 'qualified_trace_search', 'configuration_gaps': ['credential_reference']}],
                              'gaps': [{'kind': 'configuration', 'description': 'Operator credential reference remains missing.'}],
                              'discovery': {'incomplete': False}, 'configuration': json.loads(pathlib.Path(option('--evidence-config')).read_text())}
                    report['evidence_planning'] = {'schema': 'engarg.support-planning/1', 'elaboration': elaboration, 'routes': routes}
                    if os.environ.get('GRAPH_IMPLEMENTATION_GAP'):
                        routes['gaps'] = [{'kind': os.environ.get('GRAPH_GAP_KIND', 'operation_implementation'),
                            'acquisition_id': 'query', 'requirement_id': None,
                            'description': 'A producing operation is missing.', 'implementation': {'operation_purpose': 'fixture operation'}}]
                    if os.environ.get('GRAPH_DISCOVERY_INCOMPLETE'):
                        routes['discovery']['incomplete'] = True
                if '--implement-tools' in args:
                    directory = pathlib.Path(option('--tool-output-dir'))
                    assert not directory.exists(), 'Tool output must be a new directory'
                    directory.mkdir()
                    write(directory / 'argument.analysis.json', report)
                    failed = bool(os.environ.get('FAIL_TOOLING'))
                    write(directory / 'tooling.audit.jsonl', {'event': 'finished', 'passed': not failed})
                    if not failed:
                        write(directory / 'collectors.json', {'schema': 'eal.collector-catalogue/1'})
                        no_calls = os.environ.get('TOOLING_NO_CALLS') == '1'
                        report['tooling'] = {
                            'schema': 'engarg.tooling/1', 'language': option('--tool-language'),
                            'model_calls': 0 if no_calls else 2,
                            'packages': [] if no_calls else [{'need_id': 'E1', 'reused': False}],
                            'overrides': [], 'audit_path': str(directory / 'tooling.audit.jsonl')}
                        if graph:
                            report['tooling'].update(schema='engarg.capability-tooling/1', capabilities=[] if no_calls else [{'reused': False}],
                                skipped_non_implementation_gaps=1, deferred_implementation_gaps=0,
                                note='Function fixtures do not qualify domain measurement semantics.')
                        if no_calls:
                            report['tooling']['coverage'] = {
                                'implementation_gap_needs': 0, 'existing_adapter_needs': 42,
                                'existing_adapters_with_unresolved_inputs': 41,
                                'artifact_reader_needs': 34, 'outcome': 'no_implementation_required',
                                'note': 'Adapter selection and zero coding calls do not establish configured inputs or available evidence.'}
                    write(option('--output'), report)
                    if failed:
                        raise SystemExit(3)
                else:
                    write(option('--output'), report)
            elif args[:2] == ['eal', 'implement']:
                report = json.loads(pathlib.Path(option('--input')).read_text())
                assert 'evidence_planning' in report, 'Graph implementation keeps its typed routes'
                directory = pathlib.Path(option('--tool-output-dir'))
                assert not directory.exists(), 'Tool output must be a new directory'
                directory.mkdir()
                write(directory / 'argument.analysis.json', report)
                failed = bool(os.environ.get('FAIL_TOOLING'))
                write(directory / 'tooling.audit.jsonl', {'event': 'finished', 'passed': not failed})
                if failed:
                    raise SystemExit(3)
                routes = report['evidence_planning']['routes']
                eligible = [gap for gap in routes['gaps'] if gap['kind'] in ['operation_implementation', 'transformation_implementation']
                            and isinstance(gap.get('implementation'), dict)]
                no_calls = not eligible or routes['discovery']['incomplete'] or os.environ.get('TOOLING_NO_CALLS') == '1'
                report['tooling'] = {
                    'schema': 'engarg.capability-tooling/1', 'language': option('--tool-language'),
                    'model_calls': 0 if no_calls else 2,
                    'capabilities': [] if no_calls else [{'reused': False}],
                    'audit_path': str(directory / 'tooling.audit.jsonl'),
                    'skipped_non_implementation_gaps': len(routes['gaps']) - len(eligible),
                    'deferred_implementation_gaps': len(eligible) if routes['discovery']['incomplete'] else 0,
                    'note': 'Function fixtures do not qualify domain measurement semantics.'}
                if os.environ.get('CHANGE_TOOLING_ANALYSIS'):
                    report['evidence_planning']['routes']['gaps'][0]['description'] = 'Silently weakened'
                write(directory / 'analysis.with-tools.json', report)
            elif args[0] == 'validate':
                pass
            elif args[0] == 'render':
                pathlib.Path(option('--output')).write_text('offline rendered fixture\n')
            elif args[:2] == ['eal', 'export']:
                report = json.loads(pathlib.Path(option('--input')).read_text())
                if 'evidence_planning' in report:
                    stem = pathlib.Path(option('--output'))
                    revision, plan_digest = 'a' * 64, 'c' * 64
                    pending = [{'kind': 'judgement', 'description': 'Independent review remains pending.'}]
                    bundle = {'schema_version': 'engarg.support-export/1', 'authored': {'revision': revision},
                              'execution_plan_sha256': plan_digest, 'pending': pending}
                    write(option('--manifest'), bundle)
                    stem.write_text('offline graph EAL fixture\n')
                    pathlib.Path(option('--tools-output')).write_text('offline graph registry fixture\n')
                    write(stem.with_suffix('.execution-plan.json'), {'schema': 'eal.evidence-execution-plan/1'})
                    exported = {'schema': 'engarg.support-evidence-plan/1', 'case_digest': revision,
                                'execution_plan_sha256': 'd' * 64 if os.environ.get('GRAPH_STALE_PLAN') else plan_digest,
                                'support_elaboration': report['evidence_planning']['elaboration'],
                                'routes': report['evidence_planning']['routes'], 'pending': pending,
                                'independent_review_obligations': [{'id': 'independent-review'}]}
                    if not os.environ.get('GRAPH_MISSING_PLAN'):
                        write(stem.with_suffix('.evidence-plan.json'), exported)
                    for suffix in ['acquisition-requests', 'review-requests']:
                        write(stem.with_suffix('.' + suffix + '.json'), [])
                    raise SystemExit(0)
                tool_id = 'generated_fixture' if report.get('tooling', {}).get('packages') else 'http_json_get'
                revision = 'a' * 64
                entries = [{'need_id': 'E1', 'tool_id': tool_id, 'unresolved_arguments': []}]
                readiness = [{
                    'need_id': 'E1', 'tool_id': tool_id, 'implementation': 'available',
                    'configuration': 'bound', 'evidence': 'not_collected', 'gaps': []}]
                if os.environ.get('PREPARATION_GAPS') == '1':
                    entries = [{'need_id': 'K1', 'tool_id': 'kubectl_get', 'unresolved_arguments': []}]
                    readiness = [{
                        'need_id': 'K1', 'tool_id': 'kubectl_get', 'implementation': 'available',
                        'configuration': 'bound', 'evidence': 'not_collected', 'gaps': []}]
                    for adapter, count, evidence, gap in [
                        ('scoped_json', 7, 'not_collected', 'trace_ids_missing'),
                        ('artifact_json', 34, 'unavailable', 'artifact_producer_unidentified')]:
                        for number in range(count):
                            need = adapter + str(number)
                            entries.append({'need_id': need, 'tool_id': adapter, 'unresolved_arguments': ['inputs']})
                            readiness.append({
                                'need_id': need, 'tool_id': adapter, 'implementation': 'available',
                                'configuration': 'unresolved', 'evidence': evidence, 'gaps': [gap]})
                preparation = pathlib.Path(option('--preparation-output-dir'))
                preparation.mkdir()
                operator = json.loads(pathlib.Path(option('--operator-inputs')).read_text())
                prepared = {
                    'schema': 'engarg.collector-preparation/1', 'case_digest': revision,
                    'inputs_prepared': True, 'evidence_collected': False,
                    'ignored_legacy_binding_fields': [name for name in ['case_digest', 'needs'] if name in operator],
                    'entries': readiness,
                    'summary': {
                        'bound': sum(entry['configuration'] == 'bound' for entry in readiness),
                        'unresolved': sum(entry['configuration'] == 'unresolved' for entry in readiness),
                        'retained_artifacts_verified': 0}}
                if os.environ.get('PREPARATION_STALE_DIGEST') == '1':
                    prepared['case_digest'] = 'b' * 64
                if os.environ.get('PREPARATION_CLAIMS_EVIDENCE') == '1':
                    prepared['evidence_collected'] = True
                if os.environ.get('PREPARATION_MISSING') != '1':
                    write(preparation / 'collector-preparation.json', prepared)
                pathlib.Path(option('--output')).write_text('offline EAL fixture\n')
                pathlib.Path(option('--tools-output')).write_text('offline registry fixture\n')
                write(option('--manifest'), {'revision': revision, 'acquisition_plan': {'entries': entries}, 'obligations': []})
            elif args[:2] == ['eal', 'validate']:
                write(option('--output'), {'is_error': False, 'result': {'valid': True}})
            elif args[:2] == ['eal', 'plan']:
                write(option('--output'), {'is_error': False, 'result': {}})
            else:
                raise SystemExit('Unexpected EngArg operation')
            """).lstrip())
        binary.chmod(0o700)

    def install_docker_stub(self):
        docker = self.commands / 'docker'
        docker.write_text(textwrap.dedent(r"""
            #!/usr/bin/env python3
            import json, os, sys
            with open(os.environ['FAKE_ENGARG_LOG'], 'a') as log:
                log.write(json.dumps(['docker', *sys.argv[1:]]) + '\n')
            if sys.argv[1] not in ['info', 'build']:
                raise SystemExit('Unexpected Docker operation')
            """).lstrip())
        docker.chmod(0o700)

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

    def test_preflight_exposes_artifact_aliases_without_private_pins_or_stale_bindings(self):
        self.configuration.update(
            case_digest='OLD_CASE_DIGEST_MUST_NOT_ENTER_REQUEST',
            needs={'OLD_NEED_MUST_NOT_ENTER_REQUEST': 'grafana_search'},
            artifacts={
                'trace_capture': {'artifacts': [{
                    'path': '/private/DO_NOT_SEND_ARTIFACT_PATH.json',
                    'sha256': 'b' * 64, 'observed_at': '2026-01-01T00:00:00Z',
                    'source': 'DO_NOT_SEND_ARTIFACT_PROVENANCE'}]},
                'inventory': {'artifacts': []},
            },
        )
        self.write_inputs()
        directory = self.successful_run_directory(self.run_script('--check-only', self.inputs))
        request_text = (directory / 'request.json').read_text()
        context = json.loads(json.loads(request_text)['context'])
        self.assertEqual(context['retained_artifact_references'], ['inventory', 'trace_capture'])
        self.assertIn('input_preparation.kind scoped', context['collector_preparation'])
        for private in ['DO_NOT_SEND_ARTIFACT_PATH', 'DO_NOT_SEND_ARTIFACT_PROVENANCE',
                        'OLD_CASE_DIGEST_MUST_NOT_ENTER_REQUEST', 'OLD_NEED_MUST_NOT_ENTER_REQUEST']:
            self.assertNotIn(private, request_text)
        self.assertEqual((directory / 'collector-inputs.snapshot.json').read_bytes(), self.inputs.read_bytes())

    def test_invalid_artifact_reference_fails_input_preflight(self):
        for value in [{'invalid/reference': {}}, ['not-a-reference-map']]:
            with self.subTest(value=value):
                self.configuration['artifacts'] = value
                self.write_inputs()
                result = self.run_script('--check-only', self.inputs)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Input preflight failed', result.stderr)
                self.assertNotIn('Traceback', result.stderr)

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
        self.assertEqual(export[export.index('--operator-inputs') + 1],
                         str(directory / 'collector-inputs.snapshot.json'))
        self.assertTrue(Path(export[export.index('--operator-input-base') + 1]).samefile(self.inputs.parent))
        self.assertEqual(export[export.index('--preparation-output-dir') + 1],
                         str(directory / 'collector-inputs'))
        report = json.loads((directory / 'analysis.json').read_text())
        self.assertEqual(report['tooling']['schema'], 'engarg.tooling/1')
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertFalse(summary['evidence_collected'])
        self.assertEqual(summary['selected_collectors'], ['generated_fixture'])
        self.assertEqual(summary['tool_implementation']['model_calls'], 2)
        self.assertEqual(summary['tool_implementation']['verified_packages'], 1)
        self.assertEqual(summary['tool_implementation']['reused_packages'], 0)
        preparation = summary['collector_preparation']
        bundle = json.loads((directory / 'argument.bundle.json').read_text())
        self.assertEqual(preparation['case_digest'], bundle['revision'])
        self.assertFalse(preparation['evidence_collected'])
        self.assertEqual(preparation['entries'][0]['evidence'], 'not_collected')
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
        self.assertIn('collector_preparation', summary)

    def test_zero_coding_calls_keep_existing_adapter_configuration_and_evidence_gaps(self):
        self.configuration.update(case_digest='old-export', needs={'OLD_E1': 'grafana_search'})
        self.write_inputs()
        profile = self.write_toolchain()
        self.install_pipeline_stubs()
        result = self.run_script(
            '--implement-tools', '--toolchain', profile, self.inputs,
            env=dict(self.env, TOOLING_NO_CALLS='1', PREPARATION_GAPS='1'),
        )
        directory = self.pipeline_directory(result)
        summary = json.loads((directory / 'run-summary.json').read_text())
        report = json.loads((directory / 'analysis.json').read_text())
        self.assertEqual(report['tooling']['model_calls'], 0)
        self.assertEqual(report['tooling']['packages'], [])
        self.assertEqual(report['tooling']['overrides'], [])
        coverage = summary['tool_implementation']['coverage']
        self.assertEqual(coverage['outcome'], 'no_implementation_required')
        self.assertEqual(coverage['existing_adapters_with_unresolved_inputs'], 41)
        self.assertEqual(summary['tool_implementation']['verified_packages'], 0)
        self.assertEqual(summary['tool_implementation']['reused_packages'], 0)
        preparation = summary['collector_preparation']
        self.assertEqual(preparation['summary'], {
            'bound': 1, 'unresolved': 41, 'retained_artifacts_verified': 0})
        self.assertEqual(preparation['ignored_legacy_binding_fields'], ['case_digest', 'needs'])
        self.assertEqual(len(summary['unresolved_arguments']), 41)
        self.assertEqual(sum(entry['evidence'] == 'unavailable' for entry in preparation['entries']), 34)
        self.assertFalse(summary['evidence_collected'])
        self.assertIn('1 bound; 41 unresolved; 0 retained-artifact inputs verified', result.stdout)
        self.assertIn('Resolve each reported configuration or evidence gap', summary['next_step'])

    def test_preparation_report_must_match_export_and_separate_collection(self):
        self.install_pipeline_stubs()
        for variable in ['PREPARATION_STALE_DIGEST', 'PREPARATION_CLAIMS_EVIDENCE']:
            with self.subTest(variable=variable):
                before = set((self.repo / 'engarg-runs').glob('*'))
                result = self.run_script(self.inputs, env=dict(self.env, **{variable: '1'}))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Collector preparation must describe this exact export without claiming collection', result.stderr)
                self.assertNotIn('Authoring, rendering, export and EAL planning passed', result.stdout)
                directory, = set((self.repo / 'engarg-runs').glob('*')) - before
                self.assertFalse((directory / 'run-summary.json').exists())

    def test_pipeline_retains_all_presentations_from_one_analysis(self):
        self.install_pipeline_stubs()
        directory = self.pipeline_directory(self.run_script(self.inputs))
        calls = self.invocations()
        self.assertEqual(sum(call[0] == 'analyse' for call in calls), 1)
        presentations = {
            'json': 'analysis.json', 'markdown': 'argument.md', 'text': 'argument.txt',
            'html': 'argument.html', 'prose': 'prose.md',
        }
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertEqual(summary['presentations'], {
            name: str(directory / filename) for name, filename in presentations.items()
        })
        for filename in presentations.values():
            self.assertTrue((directory / filename).is_file(), filename)
        renders = [call for call in calls if call[0] == 'render']
        by_format = {call[call.index('--format') + 1]: call for call in renders}
        self.assertEqual(set(by_format), {'markdown', 'argument', 'html', 'prose'})
        for format_name, filename in [
            ('markdown', 'argument.md'), ('argument', 'argument.txt'),
            ('html', 'argument.html'), ('prose', 'prose.md'),
        ]:
            call = by_format[format_name]
            self.assertEqual(call[call.index('--input') + 1], str(directory / 'analysis.json'))
            self.assertEqual(call[call.index('--output') + 1], str(directory / filename))
        markdown = by_format['markdown']
        self.assertIn('--no-audit-appendix', markdown)
        self.assertNotIn('--display-depth', markdown)

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

    def test_graph_default_requires_explicit_planning_configuration(self):
        result = self.run_script('--check-only', self.inputs, graph=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('Graph planning requires --evidence-config', result.stderr)
        self.assertFalse((self.repo / 'engarg-runs').exists())

    def test_graph_preflight_snapshots_config_preserves_base_and_keeps_catalogues_out_of_claim(self):
        config = self.write_evidence_config()
        original = config.read_bytes()
        self.make_dirty()
        before = self.tracked_state()
        directory = self.successful_run_directory(self.run_script('--check-only', '--evidence-config', config, self.inputs, graph=True))
        self.assertEqual(self.tracked_state(), before)
        self.assertEqual((directory / 'evidence-config.snapshot.json').read_bytes(), original)
        retained = json.loads((directory / 'evidence-config.json').read_text())
        self.assertEqual(retained['collector'], 'managed')
        self.assertEqual(retained['connections'], str(config.with_name('evidence-connections.json')))
        self.assertEqual(retained['store'], str(config.parent / 'source-store'))
        request = (directory / 'request.json').read_text()
        self.assertNotIn('DO_NOT_SEND_CONNECTION_CONFIGURATION', request)
        self.assertNotIn('engarg.capability-planning-config', request)
        identity = json.loads((directory / 'input-identity.json').read_text())
        self.assertEqual(identity['evidence_planning']['mode'], 'graph')
        self.assertEqual(identity['evidence_planning']['config_sha256'], hashlib.sha256(original).hexdigest())

    def test_graph_configuration_normalization_preserves_exact_decimal_arguments(self):
        config = self.write_evidence_config()
        exact = '0.123456789012345678901234567890123456789'
        config.write_text(config.read_text().replace('"bindings": []',
            '"bindings": [{"capability_id":"selected","arguments":{"value":' + exact + '},"input_bindings":{},"configured_prerequisites":[]}]'))
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True)
        directory = self.pipeline_directory(result)
        self.assertIn('"value":' + exact, (directory / 'evidence-config.json').read_text())

    def test_graph_rejects_flat_catalogue_and_conflicting_switches(self):
        config = self.write_evidence_config()
        catalogue = self.inputs.with_name('catalogue.json')
        catalogue.write_text('{}')
        for options, expected in [
            (['--evidence-config', config, self.inputs, catalogue], 'positional collector catalogue requires --legacy-flat'),
            (['--legacy-flat', '--evidence-config', config, self.inputs], 'cannot be combined with --legacy-flat'),
        ]:
            result = self.run_script('--check-only', *options, graph=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn(expected, result.stderr)

    def test_graph_config_duplicate_keys_and_wrong_connection_schema_fail_without_runtime(self):
        config = self.write_evidence_config()
        config.write_text('{"schema":"one","schema":"two"}')
        result = self.run_script('--check-only', '--evidence-config', config, self.inputs, graph=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('Traceback', result.stderr)
        config = self.write_evidence_config()
        config.with_name('evidence-connections.json').write_text('{"schema":"wrong"}')
        result = self.run_script('--check-only', '--evidence-config', config, self.inputs, graph=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Input preflight failed', result.stderr)
        self.assertNotIn('Traceback', result.stderr)

    def test_graph_pipeline_preserves_plan_and_reports_typed_gaps_without_flat_repair(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True)
        directory = self.pipeline_directory(result)
        calls = self.invocations()
        analysis = next(call for call in calls if call[0] == 'analyse')
        self.assertEqual(analysis[analysis.index('--evidence-planning') + 1], 'graph')
        retained = json.loads((directory / 'evidence-config.json').read_text())
        self.assertEqual(retained['collector'], str((self.commands / 'eal-collector').resolve()))
        self.assertEqual(json.loads(config.read_text())['collector'], 'managed')
        export = next(call for call in calls if call[:2] == ['eal', 'export'])
        for forbidden in ['--operator-inputs', '--collector-catalogue', '--preparation-output-dir']:
            self.assertNotIn(forbidden, export)
        self.assertFalse((directory / 'collector-inputs').exists())
        for name in ['evidence-plan.json', 'argument.evidence-plan.json', 'argument.execution-plan.json', 'graph-readiness.json']:
            self.assertTrue((directory / name).is_file(), name)
        readiness = json.loads((directory / 'graph-readiness.json').read_text())
        self.assertFalse(readiness['ready_for_collection'])
        self.assertFalse(readiness['evidence_collected'])
        self.assertFalse(readiness['evidence_truth_verified'])
        self.assertEqual(readiness['gaps'][0]['kind'], 'configuration')
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertEqual(summary['evidence_planning'], 'graph')
        self.assertIn('Evidence truth and inferential soundness remain unverified', result.stdout)

    def test_graph_implementation_zero_calls_does_not_close_configuration_or_judgement_gaps(self):
        config = self.write_evidence_config()
        profile = self.write_toolchain()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, '--implement-tools', '--toolchain', profile, self.inputs,
                                 graph=True, env=dict(self.env, TOOLING_NO_CALLS='1'))
        directory = self.pipeline_directory(result)
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertEqual(summary['tool_implementation']['model_calls'], 0)
        self.assertEqual(summary['tool_implementation']['verified_packages'], 0)
        self.assertEqual(summary['graph_readiness']['independent_review_obligations'], 1)
        self.assertFalse(summary['graph_readiness']['ready_for_collection'])
        self.assertIn('Function fixtures do not qualify', summary['tool_implementation']['note'])

    def test_graph_default_rust_implementation_needs_no_docker_for_configuration_only(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, '--implement-tools', self.inputs, graph=True)
        directory = self.pipeline_directory(result)
        self.assertFalse((self.commands / 'docker').exists())
        calls = self.invocations()
        analysis = next(call for call in calls if call[0] == 'analyse')
        implementation = next(call for call in calls if call[:2] == ['eal', 'implement'])
        self.assertNotIn('--implement-tools', analysis)
        self.assertNotIn('--toolchain', analysis)
        self.assertEqual(analysis[analysis.index('--output') + 1], str(directory / 'analysis.original.json'))
        self.assertEqual(implementation[implementation.index('--input') + 1], str(directory / 'analysis.original.json'))
        original = json.loads((directory / 'analysis.original.json').read_text())
        enriched = json.loads((directory / 'analysis.json').read_text())
        tooling = enriched.pop('tooling')
        self.assertEqual(enriched, original)
        self.assertEqual(tooling['model_calls'], 0)
        self.assertEqual(tooling['capabilities'], [])
        selection = json.loads((directory / 'tooling-selection.json').read_text())
        self.assertEqual(selection['eligible_count'], 0)

    def test_graph_builds_default_rust_image_only_after_identifying_a_genuine_gap(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        self.install_docker_stub()
        result = self.run_script('--evidence-config', config, '--implement-tools',
                                 '--tool-max-calls', '4', '--tool-timeout-secs', '7200', self.inputs,
                                 graph=True, env=dict(self.env, GRAPH_IMPLEMENTATION_GAP='1'))
        directory = self.pipeline_directory(result)
        calls = self.invocations()
        analysis_index = next(index for index, call in enumerate(calls) if call[0] == 'analyse')
        build_index = next(index for index, call in enumerate(calls) if call[:2] == ['docker', 'build'])
        implementation_index = next(index for index, call in enumerate(calls) if call[:2] == ['eal', 'implement'])
        self.assertLess(analysis_index, build_index)
        self.assertLess(build_index, implementation_index)
        implementation = calls[implementation_index]
        self.assertEqual(implementation[implementation.index('--tool-max-calls') + 1], '4')
        self.assertEqual(implementation[implementation.index('--tool-timeout-secs') + 1], '7200')
        self.assertNotIn('--max-calls', implementation)
        self.assertEqual(implementation[implementation.index('--model') + 1], 'gpt-6.1-sol')
        self.assertEqual(json.loads((directory / 'tooling-selection.json').read_text())['eligible_count'], 1)
        self.assertEqual(json.loads((directory / 'analysis.json').read_text())['tooling']['model_calls'], 2)

    def test_graph_defers_incomplete_discovery_and_qualification_without_docker(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        for extra in [{'GRAPH_DISCOVERY_INCOMPLETE': '1'}, {'GRAPH_GAP_KIND': 'qualification'}]:
            with self.subTest(extra=extra):
                result = self.run_script('--evidence-config', config, '--implement-tools', self.inputs,
                                         graph=True, env=dict(self.env, GRAPH_IMPLEMENTATION_GAP='1', **extra))
                directory = self.pipeline_directory(result)
                self.assertEqual(json.loads((directory / 'tooling-selection.json').read_text())['eligible_count'], 0)
                self.assertEqual(json.loads((directory / 'analysis.json').read_text())['tooling']['model_calls'], 0)
        self.assertFalse((self.commands / 'docker').exists())

    def test_graph_missing_docker_preserves_original_after_identifying_implementation_gap(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, '--implement-tools', self.inputs,
                                 graph=True, env=dict(self.env, GRAPH_IMPLEMENTATION_GAP='1'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('identified missing operations', result.stderr)
        directory, = (self.repo / 'engarg-runs').glob('*')
        self.assertTrue((directory / 'analysis.original.json').is_file())
        self.assertFalse((directory / 'analysis.json').exists())
        self.assertFalse(any(call[:2] == ['eal', 'implement'] for call in self.invocations()))
        self.assertFalse(any(call[:2] == ['eal', 'export'] for call in self.invocations()))

    def test_graph_implementation_failure_or_changed_analysis_stops_before_export(self):
        config = self.write_evidence_config()
        profile = self.write_toolchain()
        self.install_pipeline_stubs()
        for variable in ['FAIL_TOOLING', 'CHANGE_TOOLING_ANALYSIS']:
            with self.subTest(variable=variable):
                before = set((self.repo / 'engarg-runs').glob('*'))
                old_calls = len(self.invocations()) if self.log.exists() else 0
                result = self.run_script('--evidence-config', config, '--implement-tools', '--toolchain', profile,
                                         self.inputs, graph=True, env=dict(self.env, **{variable: '1'}))
                self.assertNotEqual(result.returncode, 0)
                directory, = set((self.repo / 'engarg-runs').glob('*')) - before
                self.assertTrue((directory / 'analysis.original.json').is_file())
                self.assertTrue((directory / 'tooling/argument.analysis.json').is_file())
                self.assertFalse((directory / 'analysis.json').exists())
                self.assertFalse(any(call[:2] == ['eal', 'export'] for call in self.invocations()[old_calls:]))

    def test_graph_collector_mismatch_stops_before_inference_and_preserves_config(self):
        other = self.inputs.with_name('other-collector')
        other.write_text('#!/bin/sh\nexit 91\n')
        other.chmod(0o700)
        config = self.write_evidence_config(str(other))
        original = config.read_bytes()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Configured collector differs from the selected EAL runtime', result.stderr)
        self.assertEqual(config.read_bytes(), original)
        self.assertFalse(self.log.exists())

    def test_graph_export_identity_mismatch_cannot_claim_success(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True,
                                 env=dict(self.env, GRAPH_STALE_PLAN='1'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('retain the exact argument', result.stderr)
        self.assertNotIn('Authoring, rendering, export and EAL planning passed', result.stdout)

    def test_graph_export_must_produce_retained_evidence_plan(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True,
                                 env=dict(self.env, GRAPH_MISSING_PLAN='1'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Graph evidence plan is missing or invalid', result.stderr)
        self.assertNotIn('Traceback', result.stderr)
        self.assertNotIn('Authoring, rendering, export and EAL planning passed', result.stdout)

    def test_graph_missing_collector_companion_fails_before_model_call(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        (self.commands / 'eal-collector').unlink()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Missing executable EAL companion', result.stderr)
        self.assertIn('eal-collector', result.stderr)
        self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
