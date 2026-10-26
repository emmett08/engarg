"""Independent acceptance checks for the bounded demonstration's admission gates."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import textwrap
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1]/'scripts/catalogue_evidence_example.py'
SPEC = importlib.util.spec_from_file_location('catalogue_evidence_example', SOURCE)
EXAMPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXAMPLE)


def fixture(identifiers, count, distinct, complete=True):
    return {'kind': 'positive' if distinct else 'negative', 'input': {
        'snapshot': 'demonstration-v1', 'items': [{'id': value} for value in identifiers],
        'complete': complete, 'formal_input': EXAMPLE.FORMAL_INPUT,
    }, 'expected_value': {
        'collected': True, 'snapshot': 'demonstration-v1', 'count': count, 'distinct': distinct,
        'complete': complete, 'typed_input': EXAMPLE.FORMAL_INPUT,
    }}


def complete_fixtures():
    return [fixture([], 0, True), fixture(['only'], 1, True),
            fixture(['one', 'two', 'three'], 3, True), fixture(['same', 'same'], 2, False),
            fixture(['a', 'A'], 2, True), fixture(['one', 'two'], 2, True, complete=False)]


class CatalogueExampleTests(unittest.TestCase):
    def test_simple_launcher_forwards_topology_options_without_model_or_tool_calls(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            (repo / 'scripts').mkdir()
            source = repo / 'scripts/catalogue_evidence_example.py'
            source.write_text('# retained launcher location')
            config = repo / 'selected-topology.json'
            config.write_text('{"topology":{"kind":"ring"}}')
            with mock.patch.object(EXAMPLE, '__file__', str(source)), \
                    mock.patch.object(EXAMPLE.subprocess, 'run', return_value=mock.Mock(returncode=0)) as run:
                self.assertEqual(EXAMPLE.launch(['--check-only', '--topology-config', str(config),
                    '--eal-coordinate', str(repo / 'eal-coordinate'),
                    '--coordination-run', str(repo / 'journal')]), 0)
            command = run.call_args.args[0]
            self.assertEqual(command[command.index('--topology-config') + 1], str(config))
            self.assertIn('--check-only', command)
            self.assertEqual(command[command.index('--tool-language') + 1], 'rust')
            self.assertIn('--eal-coordinate', command)
            self.assertIn('--coordination-run', command)
            self.assertEqual(run.call_count, 1)

    def test_simple_claim_has_scoped_contracts_and_no_supplied_observations(self):
        request = EXAMPLE.request_for({'schema': 'engarg.catalogue-evidence-example/1', 'snapshot': 'demonstration-v1'})
        self.assertEqual(request['evidence'], [])
        context = json.loads(request['context'])
        self.assertFalse(context['observations_supplied'])
        self.assertEqual(context['subject_scope']['selectors'], {'snapshot': 'demonstration-v1'})
        self.assertEqual(context['formal_input']['payload']['premises'], ['count_is_three', 'identifiers_distinct'])
        self.assertEqual(context['formal_scope'], EXAMPLE.METHOD_SCOPE)
        self.assertEqual(context['formal_input']['scope'], EXAMPLE.method_scope_identity(context['formal_scope']))
        self.assertEqual(context['formal_input']['scope'],
                         'engarg.scope:e4573ab176d50fdfebebee9913448452898c9ad953abd8b22ae3ba2e9efb02a8')
        self.assertIn('separately evaluated support requirements', context['required_workflow'])
        with self.assertRaises(ValueError):
            EXAMPLE.request_for({'schema': 'engarg.catalogue-evidence-example/1', 'snapshot': 'demonstration-v1', 'claim': 'unrelated'})

    def test_independent_fixture_gate_covers_negatives_case_sensitivity_and_incompleteness(self):
        self.assertEqual(EXAMPLE.verify_fixtures(complete_fixtures()), [
            'case_sensitive', 'duplicates', 'empty', 'incomplete', 'singleton', 'three_distinct'])

    def test_constant_true_or_constant_count_fixtures_cannot_qualify(self):
        for field, value in [('distinct', True), ('count', 3)]:
            fixtures = copy.deepcopy(complete_fixtures())
            for item in fixtures:
                item['expected_value'][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'independent reference'):
                EXAMPLE.verify_fixtures(fixtures)

    def test_missing_adverse_examples_cannot_qualify(self):
        with self.assertRaisesRegex(ValueError, 'omit independently required cases'):
            EXAMPLE.verify_fixtures([fixture(['one', 'two', 'three'], 3, True)])

    def test_host_probe_plan_is_complete_when_model_fixture_selection_is_partial(self):
        selected = [fixture(['one', 'two', 'three'], 3, True)]
        self.assertEqual(EXAMPLE.verify_fixtures(selected, require_coverage=False), ['three_distinct'])
        self.assertEqual(EXAMPLE.verify_fixtures(EXAMPLE.frozen_probe_plan()['cases']), [
            'case_sensitive', 'duplicates', 'empty', 'incomplete', 'singleton', 'three_distinct'])

    def test_probe_launcher_retains_observed_negative_failure_before_registration(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory/'tooling').mkdir()
            package = directory/'package'
            package.mkdir()
            (package/'profile.json').write_text(json.dumps({'timeout_seconds': 1}))
            launcher = directory/'fake-engarg'
            launcher.write_text('#!/usr/bin/env python3\n'+textwrap.dedent('''\
                import json, sys
                request = json.load(sys.stdin)
                values = {'empty': (0, True), 'singleton': (1, True), 'three-distinct': (3, True),
                          'duplicates': (2, True), 'case-sensitive': (2, True), 'incomplete': (2, True)}
                count, distinct = values[request['context']['probe']]
                value = {'collected': True, 'snapshot': request['input']['snapshot'], 'count': count,
                         'distinct': distinct, 'complete': request['input']['complete'],
                         'typed_input': request['input']['formal_input']}
                print(json.dumps({'value': value, 'observed_at': '2026-10-25T00:00:00Z',
                    'context': request['context'],
                    'request': {key: request[key] for key in ['tool', 'tool_version', 'input', 'context']}}))
                '''))
            launcher.chmod(0o700)
            metadata = {'package_manifest': str(package/'manifest.json'), 'manifest_sha256': 'a'*64,
                        'collector': {'id': 'fixture-only'}}
            with self.assertRaisesRegex(ValueError, 'Independent frozen custom-tool probes failed'):
                EXAMPLE.run_probes(directory, launcher, metadata, EXAMPLE.frozen_probe_plan())
            receipt = json.loads((directory/'tooling/demo-probes.receipt.json').read_text())
            self.assertFalse(receipt['passed'])
            self.assertEqual(receipt['results'][-1]['name'], 'duplicates')
            self.assertFalse(receipt['results'][-1]['passed'])
            self.assertTrue((directory/'tooling/demo-probes.audit.jsonl').read_text().endswith('\n'))
            self.assertFalse((directory/'tooling/demo-qualified-providers.json').exists())

    def test_reference_retains_negative_and_incomplete_results(self):
        duplicate = fixture(['same', 'same'], 2, False, complete=False)
        self.assertEqual(EXAMPLE.audit_reference(duplicate['input']), duplicate['expected_value'])
        changed = copy.deepcopy(duplicate['input'])
        changed['formal_input']['scope'] = 'another-snapshot'
        with self.assertRaises(ValueError):
            EXAMPLE.audit_reference(changed)

    def test_actual_audit_must_use_the_acquired_snapshot_and_exact_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            store = directory/'sources'
            (store/'observations').mkdir(parents=True)
            source = {'observation_id': 'source-observation', 'source': {'capability_id': 'catalogue_snapshot'},
                      'arguments': {'snapshot': 'demonstration-v1'}, 'value': {
                          'snapshot': 'demonstration-v1', 'items': [{'id': 'a'}, {'id': 'A'}], 'complete': True}}
            computation = {'observation_id': 'audit-observation', 'source': {'capability_id': 'fixture_audit'},
                           'arguments': fixture(['a', 'A'], 2, True)['input'],
                           'value': fixture(['a', 'A'], 2, True)['expected_value']}
            references = []
            for record in [source, computation]:
                record['value_sha256'] = EXAMPLE.digest(record['value'])
                name = record['observation_id']+'.json'
                (store/'observations'/name).write_text(json.dumps(record))
                references.append({'observation_id': record['observation_id'], 'value_sha256': record['value_sha256'],
                                   'snapshot': 'observations/'+name})
            (directory/'argument.bundle.json').write_text(json.dumps({'locations': {'store': str(store)}}))
            (directory/'evidence-collection.json').write_text(json.dumps({'result': {'records': {
                'audit': {'details': {'observations': references}}}}}))
            self.assertTrue(EXAMPLE.verify_observations(directory)['passed'])
            computation['arguments']['items'] = [{'id': 'unrelated'}, {'id': 'snapshot'}]
            (store/'observations/audit-observation.json').write_text(json.dumps(computation))
            with self.assertRaisesRegex(ValueError, 'differs from the acquired MCP snapshot'):
                EXAMPLE.verify_observations(directory)


if __name__ == '__main__':
    unittest.main()
