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
                description = {'source_syntax': 'EAL/3', 'methods': {'structured/1': {}}}
                if os.environ.get('HOST_COLLECTION_LIMIT'):
                    description[os.environ.get('HOST_COLLECTION_PATH', 'execution_limits')] = {
                        'collection_evidence': json.loads(os.environ['HOST_COLLECTION_LIMIT'])}
                write(option('--output'), {'is_error': False, 'result': description})
            elif args[:2] == ['eal', 'replan']:
                if os.environ.get('FAIL_REPLAN'):
                    raise SystemExit('Synthetic replan failure')
                report = json.loads(pathlib.Path(option('--input')).read_text())
                report['evidence_planning']['routes']['configuration'] = json.loads(
                    pathlib.Path(option('--evidence-config')).read_text())
                report['replanning'] = {'schema': 'engarg.replanning/1', 'model_calls': 0}
                write(option('--output'), report)
            elif args[0] == 'analyse':
                request = json.loads(pathlib.Path(option('--input')).read_text())
                report = {'request': request, 'argument': {'status': 'supported'}, 'transcript': []}
                graph = '--evidence-planning' in args
                if graph:
                    elaboration = {'schema': 'engarg.support-elaboration/1', 'requirements': [{'id': 'R1'}], 'acquisitions': [{'id': 'query'}]}
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
                    if os.environ.get('GRAPH_READY_ROUTES'):
                        routes['gaps'] = []
                        routes['routes'][0]['configuration_gaps'] = []
                    if os.environ.get('GRAPH_PENDING_JUDGEMENT'):
                        routes['gaps'].append({'kind': 'judgement', 'requirement_id': 'R1',
                                               'description': 'Independent assessment remains pending.'})
                    if os.environ.get('GRAPH_COLLAPSED_DESIGN'):
                        elaboration['acquisitions'] = []
                        routes['routes'] = []
                        routes['gaps'] = [{'kind': 'source_unavailable', 'requirement_id': 'R1',
                                          'description': 'Contract was withheld after validation.'}]
                        report['evidence_planning']['design_review'] = {
                            'schema': 'engarg.acquisition-design-review/1', 'status': 'pending_repair',
                            'active_acquisitions': 0, 'active_derivations': 0,
                            'pending_designs': [{'missing_acquisition_ids': ['query']}]}
                    if os.environ.get('GRAPH_BLOCKED_DEPENDENCY'):
                        elaboration['acquisitions'].append({'id': 'discovery'})
                        routes['routes'].append({'acquisition_id': 'discovery', 'selected': None,
                                                  'configuration_gaps': ['missing_connection']})
                        routes['execution'] = {'acquisitions': [
                            {'id': 'query', 'bindings': [{'output': {'step_id': 'derive_version'}}]},
                            {'id': 'discovery', 'bindings': []}],
                            'derivations': [{'id': 'derive_version', 'inputs': [{'step_id': 'discovery'}]}]}

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
                    emitted = int(os.environ.get('GRAPH_EMITTED_EVIDENCE', '1'))
                    budget = {'schema': 'engarg.collection-budget/1', 'assumed_default_limit': 128,
                              'limit_source': 'EAL default collection_evidence; the selected host may advertise an override',
                              'total_emitted_evidence': emitted, 'claim_evidence_counts': {'current_claim': 1}}
                    if emitted > 128:
                        pending.append({'kind': 'collection_budget_exceeded', 'description': 'Assumed default exceeded.'})
                    bundle = {'schema_version': 'engarg.support-export/1', 'authored': {'revision': revision},
                              'claim_names': {'current_claim': 'current_claim'},
                              'execution_plan_sha256': plan_digest, 'pending': pending}
                    write(option('--manifest'), bundle)
                    stem.write_text('offline graph EAL fixture\n')
                    pathlib.Path(option('--tools-output')).write_text('offline graph registry fixture\n')
                    write(stem.with_suffix('.execution-plan.json'), {'schema': 'eal.evidence-execution-plan/1'})
                    exported = {'schema': 'engarg.support-evidence-plan/1', 'case_digest': revision,
                                'execution_plan_sha256': 'd' * 64 if os.environ.get('GRAPH_STALE_PLAN') else plan_digest,
                                'support_elaboration': report['evidence_planning']['elaboration'],
                                'routes': report['evidence_planning']['routes'], 'pending': pending,
                                'collection_budget': budget,
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
                write(option('--manifest'), {'revision': revision, 'acquisition_plan': {'entries': entries}, 'obligations': [],
                                            'claim_names': {'current_claim': 'current_claim'}})
            elif args[:2] == ['eal', 'validate']:
                write(option('--output'), {'is_error': False, 'result': {'valid': True}})
            elif args[:2] == ['eal', 'plan']:
                write(option('--output'), {'is_error': False, 'result': {}})
            elif args[:2] == ['eal', 'collect']:
                write(option('--output'), {'is_error': False, 'result': {
                    'collection_id': 'offline-script-collection',
                    'records': {'source': {'status': 'ok'}}}})
            elif args[:2] == ['eal', 'assess']:
                assert option('--collection-id') == 'offline-script-collection'
                write(option('--output'), {'is_error': False, 'result': {
                    'assessment_id': 'offline-script-assessment', 'valid': True, 'complete': True,
                    'claims': {'current_claim': {'status': os.environ.get('ASSESSMENT_STATUS', 'supported')}}}})
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
        self.assertIn("rendering, export and EAL planning passed", result.stdout)
        return Path(result.stdout.split("Artifacts: ", 1)[1].splitlines()[0])

    def invocations(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_topology_preflight_records_all_presets_without_runtime_dependencies(self):
        for preset in ['fully-connected', 'grouped', 'chain', 'ring', 'star', 'tree', 'layered', 'sparse', 'bipartite']:
            with self.subTest(preset=preset):
                directory = self.successful_run_directory(self.run_script(
                    '--check-only', '--topology', preset, '--debate-participants', '4',
                    '--debate-rounds', '2', self.inputs))
                identity = json.loads((directory / 'input-identity.json').read_text())
                self.assertEqual(identity['debate']['preset'], preset)
                self.assertEqual(identity['debate']['participants'], 4)
                self.assertEqual(identity['debate']['rounds'], 2)
                self.assertFalse((directory / 'run-settings.json').exists())

    def test_topology_file_is_snapshotted_exactly_and_conflicting_flags_are_rejected(self):
        config = self.inputs.with_name('topology.json')
        raw = b'{ "participants":4, "rounds":2, "topology":{"kind":"ring"} }\n'
        config.write_bytes(raw)
        directory = self.successful_run_directory(self.run_script(
            '--check-only', '--topology-config', config, self.inputs))
        self.assertEqual((directory / 'topology.snapshot.json').read_bytes(), raw)
        identity = json.loads((directory / 'input-identity.json').read_text())
        self.assertEqual(identity['debate']['configuration_sha256'], hashlib.sha256(raw).hexdigest())
        for options in [('--topology', 'ring', '--topology-config', config),
                        ('--debate-rounds', '2'), ('--eal-coordinate', 'unselected')]:
            rejected = self.run_script('--check-only', *options, self.inputs)
            self.assertEqual(rejected.returncode, 2)

    def test_topology_arguments_and_automatic_call_budget_reach_actual_analysis_boundary(self):
        self.install_pipeline_stubs()
        coordinate = self.commands / 'eal-coordinate'
        coordinate.write_text('#!/usr/bin/env bash\nexit 0\n')
        coordinate.chmod(0o700)
        directory = self.pipeline_directory(self.run_script(
            '--topology', 'ring', '--debate-participants', '4', '--debate-rounds', '2',
            self.inputs))
        analysis = next(call for call in self.invocations() if call[0] == 'analyse')
        value = lambda name: analysis[analysis.index(name) + 1]
        self.assertEqual(value('--topology'), 'ring')
        self.assertEqual(value('--debate-participants'), '4')
        self.assertEqual(value('--debate-rounds'), '2')
        self.assertEqual(value('--max-calls'), '16')
        self.assertEqual(value('--eal-coordinate'), str(coordinate.resolve()))
        self.assertEqual(value('--coordination-run'), str((directory / 'coordination').resolve()))
        self.assertEqual(json.loads((directory / 'run-settings.json').read_text())['debate']['preset'], 'ring')

    def test_explicit_topology_budget_and_configuration_snapshot_are_forwarded(self):
        self.install_pipeline_stubs()
        coordinate = self.commands / 'eal-coordinate'
        coordinate.write_text('#!/usr/bin/env bash\nexit 0\n')
        coordinate.chmod(0o700)
        config = self.inputs.with_name('selected.json')
        raw = b'{"participants":4,"rounds":2,"topology":{"kind":"ring"}}\n'
        config.write_bytes(raw)
        directory = self.pipeline_directory(self.run_script(
            '--topology-config', config, self.inputs,
            env=dict(self.env, ENGARG_MAX_CALLS='30', ENGARG_EAL_COORDINATE=str(coordinate))))
        analysis = next(call for call in self.invocations() if call[0] == 'analyse')
        value = lambda name: analysis[analysis.index(name) + 1]
        self.assertEqual(value('--max-calls'), '30')
        self.assertEqual(value('--topology-config'), str((directory / 'topology.snapshot.json').resolve()))
        self.assertEqual((directory / 'topology.snapshot.json').read_bytes(), raw)

    def test_topology_explicit_underbudget_stops_before_host_and_model_calls(self):
        self.install_pipeline_stubs()
        result = self.run_script('--topology', 'ring', '--debate-participants', '4',
            self.inputs, env=dict(self.env, ENGARG_MAX_CALLS='7'))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('at least 12 calls', result.stderr)
        self.assertFalse(self.log.exists())

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
        self.assertEqual(summary['presentations'], {
            name: str(directory / filename) for name, filename in {
                'json': 'analysis.json', 'markdown': 'argument.md', 'text': 'argument.txt',
                'html': 'argument.html', 'prose': 'prose.md',
            }.items()
        })
        for path in summary['presentations'].values():
            self.assertTrue(Path(path).is_file())
        self.assertIn('Evidence truth and inferential soundness remain unverified', result.stdout)

    def test_replanning_reuses_saved_argument_without_authoring_or_api_key(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        first = self.pipeline_directory(self.run_script('--evidence-config', config, self.inputs, graph=True))
        retained = first / 'analysis.json'
        saved = json.loads(retained.read_text())
        saved['transcript'] = [{'retained_fixture_call': number} for number in range(6)]
        retained.write_text(json.dumps(saved))
        original_bytes = retained.read_bytes()
        original = json.loads(original_bytes)
        updated = json.loads(config.read_text())
        updated['ranking']['dimensions'] = ['execution_latency_ms']
        updated['prefer_configured'] = True
        config.write_text(json.dumps(updated))
        self.log.write_text('')
        environment = dict(self.env)
        environment.pop('OPENAI_API_KEY')
        second = self.pipeline_directory(self.run_script(
            '--replan-analysis', retained, '--evidence-config', config, self.inputs,
            graph=True, env=environment))
        calls = self.invocations()
        self.assertFalse(any(call[0] == 'analyse' for call in calls))
        self.assertEqual(sum(call[:2] == ['eal', 'replan'] for call in calls), 1)
        replanned = json.loads((second / 'analysis.json').read_text())
        for key in ['request', 'argument', 'transcript']:
            self.assertEqual(replanned[key], original[key])
        self.assertEqual(replanned['evidence_planning']['elaboration'], original['evidence_planning']['elaboration'])
        self.assertEqual(replanned['evidence_planning']['routes']['configuration']['ranking'], updated['ranking'])
        self.assertIs(replanned['evidence_planning']['routes']['configuration']['prefer_configured'], True)
        self.assertEqual(retained.read_bytes(), original_bytes)
        self.assertEqual((second / 'retained-analysis.snapshot.json').read_bytes(), original_bytes)
        summary = json.loads((second / 'run-summary.json').read_text())
        self.assertEqual(summary['model_calls_this_run'], 0)
        self.assertEqual(summary['recorded_model_calls'], 6)
        self.assertEqual(summary['analysis_reuse']['sha256'], hashlib.sha256(original_bytes).hexdigest())
        self.assertEqual(len(summary['presentations']), 5)

    def test_graph_collection_readiness_uses_actual_host_cap_for_all_emitted_evidence(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        for limit, emitted, expected, field in [
            (256, 129, True, 'execution_limits'),
            (64, 65, False, 'execution_limits'),
            (64, 64, True, 'execution_limits'),
            (None, 129, False, 'execution_limits'),
            (None, 128, True, 'execution_limits'),
            (256, 129, True, 'budgets'),
        ]:
            with self.subTest(limit=limit, emitted=emitted, field=field):
                environment = dict(self.env, GRAPH_READY_ROUTES='1', GRAPH_EMITTED_EVIDENCE=str(emitted))
                if limit is not None:
                    environment.update(HOST_COLLECTION_LIMIT=str(limit), HOST_COLLECTION_PATH=field)
                directory = self.pipeline_directory(self.run_script(
                    '--evidence-config', config, self.inputs, graph=True, env=environment))
                exported = json.loads((directory / 'argument.evidence-plan.json').read_text())
                readiness = json.loads((directory / 'graph-readiness.json').read_text())
                self.assertEqual(readiness['collection_budget'], exported['collection_budget'])
                self.assertEqual(readiness['pending'], exported['pending'])
                self.assertTrue(any(item['kind'] == 'judgement' for item in readiness['pending']))
                self.assertEqual(any(item['kind'] == 'collection_budget_exceeded' for item in readiness['pending']), emitted > 128)
                admission = readiness['collection_admission']
                self.assertEqual(admission['selection'], 'all_emitted_evidence')
                self.assertEqual(admission['evidence_count'], emitted)
                self.assertEqual(readiness['collection_budget']['claim_evidence_counts'], {'current_claim': 1})
                self.assertEqual(admission['limit'], 128 if limit is None else limit)
                self.assertEqual(admission['limit_is_assumed'], limit is None)
                self.assertEqual(admission['within_limit'], expected)
                self.assertEqual(readiness['ready_for_collection'], expected)
                self.assertEqual(admission['limit_source'], 'exported.collection_budget.assumed_default_limit'
                                 if limit is None else 'eal-preflight.result.' + field + '.collection_evidence')
                summary = json.loads((directory / 'run-summary.json').read_text())
                self.assertEqual(summary['graph_readiness'], readiness)
                if not expected:
                    self.assertIn('Default collect-all selects', summary['next_step'])

    def test_graph_readiness_distinguishes_pending_reviews_from_configured_source_routes(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True,
            env=dict(self.env, GRAPH_READY_ROUTES='1', GRAPH_PENDING_JUDGEMENT='1'))
        directory = self.pipeline_directory(result)
        readiness = json.loads((directory / 'graph-readiness.json').read_text())
        self.assertEqual(readiness['authored_logical_acquisitions'], 1)
        self.assertEqual(readiness['compiled_acquisition_routes'], 1)
        self.assertEqual(readiness['logical_acquisitions'], 1)
        self.assertEqual(readiness['source_collection']['status'], 'ready')
        self.assertEqual(readiness['source_collection']['ready_acquisition_ids'], ['query'])
        self.assertTrue(readiness['ready_for_collection'])
        self.assertFalse(readiness['evidence_truth_verified'])
        self.assertEqual(readiness['independent_review_obligations'], 1)
        self.assertEqual(readiness['gaps'][0]['kind'], 'judgement')

    def test_graph_readiness_checks_dependencies_through_derivations(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True,
            env=dict(self.env, GRAPH_READY_ROUTES='1', GRAPH_BLOCKED_DEPENDENCY='1'))
        directory = self.pipeline_directory(result)
        readiness = json.loads((directory / 'graph-readiness.json').read_text())
        self.assertEqual(readiness['source_collection']['status'], 'blocked')
        self.assertEqual(readiness['source_collection']['ready_acquisition_ids'], [])
        self.assertEqual(readiness['source_collection']['blocked_acquisition_ids'], ['discovery', 'query'])
        self.assertFalse(readiness['ready_for_collection'])

    def test_graph_collapse_warns_and_retains_completed_artifacts_with_explicit_stage_status(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--evidence-config', config, self.inputs, graph=True,
            env=dict(self.env, GRAPH_COLLAPSED_DESIGN='1'))
        directory = self.pipeline_directory(result)
        readiness = json.loads((directory / 'graph-readiness.json').read_text())
        summary = json.loads((directory / 'run-summary.json').read_text())
        self.assertIn('warning: acquisition planning is incomplete', result.stderr)
        self.assertEqual(readiness['acquisition_design']['status'], 'pending_repair')
        self.assertEqual(readiness['acquisition_design']['pending_designs'], 1)
        self.assertEqual(readiness['authored_logical_acquisitions'], 0)
        self.assertEqual(readiness['compiled_acquisition_routes'], 0)
        self.assertEqual(readiness['source_collection']['status'], 'no_acquisitions')
        self.assertFalse(readiness['ready_for_collection'])
        self.assertEqual(summary['stage_status']['argument_validation'], 'passed')
        self.assertEqual(summary['stage_status']['artifact_generation'], 'passed')
        self.assertEqual(summary['stage_status']['acquisition_design'], 'pending_repair')
        self.assertEqual(summary['stage_status']['tool_implementation'], 'not_requested')
        self.assertIn('targeted repair', result.stderr)
        for filename in ['analysis.json', 'argument.md', 'argument.execution-plan.json', 'argument.bundle.json']:
            self.assertTrue((directory / filename).is_file(), filename)

    def test_graph_collection_readiness_rejects_malformed_advertised_cap(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        for limit in [0, True, '256']:
            with self.subTest(limit=limit):
                self.log.write_text('')
                result = self.run_script('--evidence-config', config, self.inputs, graph=True,
                    env=dict(self.env, GRAPH_READY_ROUTES='1', HOST_COLLECTION_LIMIT=json.dumps(limit)))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('collection_evidence budget must be a positive integer', result.stderr)
                self.assertFalse(any(call[0] == 'analyse' for call in self.invocations()))

    def test_replanning_rejects_changed_requests_and_failure_stops_before_export(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        first = self.pipeline_directory(self.run_script('--evidence-config', config, self.inputs, graph=True))
        retained = first / 'analysis.json'
        original = retained.read_bytes()
        changed = json.loads(original)
        changed['request']['claim'] = 'Different engineering claim'
        retained.write_text(json.dumps(changed))
        self.log.write_text('')
        result = self.run_script('--replan-analysis', retained, '--evidence-config', config, self.inputs, graph=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.invocations(), [])
        retained.write_bytes(original)
        result = self.run_script('--replan-analysis', retained, '--evidence-config', config, self.inputs,
                                 graph=True, env=dict(self.env, FAIL_REPLAN='1'))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[:2] == ['eal', 'export'] for call in self.invocations()))

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
        self.assertIn('--progress', analysis)
        self.assertIn('--progress', implementation)
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

    def test_default_provider_maximum_and_automatic_argument_depth(self):
        self.install_pipeline_stubs()
        directory = self.pipeline_directory(self.run_script(self.inputs))
        analysis = next(call for call in self.invocations() if call[0] == 'analyse')
        self.assertEqual(analysis[analysis.index('--max-output-tokens') + 1], '128000')
        self.assertEqual(analysis[analysis.index('--max-response-bytes') + 1], '16777216')
        self.assertNotIn('--argument-depth', analysis)
        self.assertIsNone(json.loads((directory/'run-settings.json').read_text())['argument_depth'])

    def test_explicit_model_budget_and_depth_are_forwarded(self):
        self.install_pipeline_stubs()
        self.pipeline_directory(self.run_script(self.inputs, env=dict(
            self.env, ENGARG_MODEL='operator-deployment', ENGARG_MAX_OUTPUT_TOKENS='32000', ENGARG_ARGUMENT_DEPTH='5')))
        analysis = next(call for call in self.invocations() if call[0] == 'analyse')
        self.assertEqual(analysis[analysis.index('--max-output-tokens') + 1], '32000')
        self.assertEqual(analysis[analysis.index('--argument-depth') + 1], '5')

    def test_unknown_model_and_unsupported_known_budget_stop_before_inference(self):
        self.install_pipeline_stubs()
        for overrides in [{'ENGARG_MODEL': 'operator-deployment'}, {'ENGARG_MAX_OUTPUT_TOKENS': '128001'},
                          {'ENGARG_ARGUMENT_DEPTH': '17'}]:
            with self.subTest(overrides=overrides):
                result = self.run_script(self.inputs, env=dict(self.env, **overrides))
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.log.exists())

    def test_collection_and_assessment_share_actual_collection_and_database(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        directory = self.pipeline_directory(self.run_script(
            '--collect-and-assess', '--evidence-config', config, self.inputs, graph=True))
        calls = self.invocations()
        collect = next(call for call in calls if call[:2] == ['eal', 'collect'])
        assess = next(call for call in calls if call[:2] == ['eal', 'assess'])
        for call in [collect, assess]:
            self.assertEqual(call[call.index('--database') + 1], str(directory/'eal.sqlite'))
            self.assertEqual(call[call.index('--tools') + 1], str(directory/'argument.tools.toml'))
            self.assertEqual(call[call.index('--timeout-secs') + 1], '600')
        self.assertEqual(assess[assess.index('--collection-id') + 1], 'offline-script-collection')
        summary = json.loads((directory/'run-summary.json').read_text())
        self.assertTrue(summary['evidence_collected'])
        self.assertEqual(summary['eal_execution']['status'], 'supported')
        self.assertEqual(summary['stage_status']['evidence_assessment'], 'supported')

    def test_unresolved_assessment_preserves_every_artifact_and_returns_nonzero(self):
        config = self.write_evidence_config()
        self.install_pipeline_stubs()
        result = self.run_script('--collect-and-assess', '--evidence-config', config, self.inputs, graph=True,
                                 env=dict(self.env, ASSESSMENT_STATUS='unsupported'))
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        directory = Path(result.stdout.split('Artifacts: ', 1)[1].splitlines()[0])
        self.assertEqual(json.loads((directory/'execution-result.json').read_text())['status'], 'unresolved')
        self.assertTrue((directory/'evidence-collection.json').is_file())
        self.assertTrue((directory/'eal-assessment.json').is_file())
        self.assertTrue((directory/'run-summary.json').is_file())
        self.assertIn('current claim: unsupported', result.stdout)

    def test_simple_catalogue_preflight_needs_no_runtime_or_provider(self):
        shutil.copyfile(SOURCE_SCRIPT.parent/'catalogue_evidence_example.py', self.script.parent/'catalogue_evidence_example.py')
        shutil.copyfile(SOURCE_SCRIPT.parents[1]/'examples/toolchains/python.json', self.repo/'examples/toolchains/python.json')
        directory = self.successful_run_directory(self.run_script('--simple-example', '--check-only', graph=True))
        request = json.loads((directory/'request.json').read_text())
        self.assertEqual(request['claim'], 'The demonstration-v1 catalogue snapshot contains three entries with distinct identifiers.')
        self.assertEqual(request['evidence'], [])
        context = json.loads(request['context'])
        self.assertEqual(context['required_custom_operation']['schema'], 'engarg.operation-specification/1')
        self.assertEqual(context['formal_input']['method'], 'deductive/1')
        self.assertFalse((self.commands/'cargo').exists())


if __name__ == "__main__":
    unittest.main()
