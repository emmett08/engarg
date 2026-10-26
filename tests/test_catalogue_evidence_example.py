"""Independent acceptance checks for the bounded demonstration's admission gates."""
import copy
import importlib.util
import io
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


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False))


def qualification_run(directory, count_unit='entries'):
    """Retain independent operator authority and a separately authored graph."""
    source_contract = {
        'schema': 'eal.support-contract/1', 'support_type': 'source_retrieval',
        'scope': {'kind': 'catalogue_snapshot', 'subject': 'One retained snapshot',
                  'selectors': {'snapshot': EXAMPLE.SNAPSHOT}, 'identity_dimensions': ['snapshot']},
        'acceptable_methods': ['catalogue.snapshot/1'],
        'quantities': [{'name': name, 'unit': unit} for name, unit in [
            ('snapshot', '1'), ('items', 'json'), ('complete', 'boolean')]],
        'logical_strength': 'complete_enumeration', 'population': 'demonstration-v1 snapshot items',
        'time': {'historical_interval': None, 'max_age_seconds': 3600,
                 'max_uncertainty_seconds': None, 'require_original_observation_time': True},
        'provenance': {'required_authorities': ['catalogue-fixture'], 'transformation_history': True},
        'expected_output_schema_sha256': None,
    }
    quantities = [{'name': name, 'unit': unit} for name, unit in [
        ('snapshot', '1'), ('count', count_unit), ('distinct', 'boolean'),
        ('complete', 'boolean'), ('typed_input', 'json')]]
    audit_contract = copy.deepcopy(source_contract)
    audit_contract.update(support_type='calculation', acceptable_methods=[EXAMPLE.METHOD],
                          quantities=quantities, logical_strength='exact_calculation', population=None,
                          provenance={'required_authorities': ['catalogue-audit'],
                                      'transformation_history': True})
    authored = copy.deepcopy(EXAMPLE.operation_specification())
    authored['definition'] = 'Count every supplied array entry and compare identifiers with exact string equality.'
    authored['input_conditions'] = ['The supplied array has no more than 32 entries.']
    authored['output_guarantees'] = ['Echo the complete formal envelope and preserve snapshot identity.']
    for name, unit in [('snapshot', '1'), ('items', 'json'), ('complete', 'boolean'), ('formal_input', 'json')]:
        authored['input_schema']['properties'][name]['x-engarg-unit'] = unit
    for quantity in quantities:
        authored['output_schema']['properties'][quantity['name']]['x-engarg-unit'] = quantity['unit']
    source = {'id': 'retained_snapshot', 'contract': source_contract,
              'arguments': {'snapshot': EXAMPLE.SNAPSHOT}, 'bindings': [], 'for_each': None}
    audit = {'id': 'audit_the_snapshot', 'contract': audit_contract, 'operation_spec': authored,
             'arguments': {'snapshot': None, 'items': None, 'complete': None,
                           'formal_input': copy.deepcopy(EXAMPLE.FORMAL_INPUT)},
             'bindings': [{'argument_pointer': '/'+name,
                           'output': {'step_id': source['id'], 'pointer': '/'+name}}
                          for name in ['snapshot', 'items', 'complete']], 'for_each': None}
    input_schema = copy.deepcopy(EXAMPLE.AUDIT_INPUT)
    for name, unit in [('snapshot', '1'), ('items', 'json'), ('complete', 'boolean'), ('formal_input', 'json')]:
        input_schema['properties'][name]['x-engarg-unit'] = unit
    for name in ['snapshot', 'items', 'complete']:
        unit = input_schema['properties'][name]['x-engarg-unit']
        input_schema['properties'][name]['x-engarg-source-contract'] = {
            'x-engarg-quantity': {'name': name, 'unit': unit}}
    output_schema = copy.deepcopy(EXAMPLE.AUDIT_OUTPUT)
    output_schema['properties']['collected'] = {'type': 'boolean'}
    output_schema['required'].append('collected')
    for quantity in quantities:
        output_schema['properties'][quantity['name']]['x-engarg-quantity'] = copy.deepcopy(quantity)
    wanted = EXAMPLE.operation_specification()
    purpose = {key: wanted[key] for key in ['schema', 'definition', 'input_conditions', 'output_guarantees']}
    purpose.update(methods=[EXAMPLE.METHOD], provenance='The executor preserves original dependency provenance.')
    parameterised = copy.deepcopy(audit_contract)
    parameterised['scope'].update(selectors={}, subject='parameterised catalogue_snapshot operation')
    parameterised['time']['max_age_seconds'] = None
    operation = {'support_contract': parameterised, 'input_schema': input_schema,
                 'output_schema': output_schema, 'operation_purpose': json.dumps(purpose),
                 'declared_limits': {}}
    package = {'package_manifest': str(directory/'package/manifest.json'),
               'manifest_sha256': 'a'*64, 'capability_sha256': 'b'*64,
               'collector': {'id': 'generated_audit', 'input_schema': input_schema,
                             'result_schema': output_schema, 'purpose': json.dumps(purpose)}}
    planning = {'schema': 'engarg.support-planning/1',
                'elaboration': {'schema': 'engarg.support-elaboration/2', 'acquisitions': [source, audit],
                                'requirements': [], 'derivations': [], 'evaluations': [], 'revisions': []},
                'routes': {'configuration': {'operation_specifications': {EXAMPLE.METHOD: wanted}},
                           'gaps': [{'kind': 'transformation_implementation', 'acquisition_id': audit['id'],
                                     'requirement_id': None, 'implementation': copy.deepcopy(operation)}]}}
    analysis = {'request': EXAMPLE.request_for({'schema': 'engarg.catalogue-evidence-example/1',
                                               'snapshot': EXAMPLE.SNAPSHOT}),
                'argument': {'retained': 'The accepted argument stays unchanged.'},
                'transcript': [{'retained': 'The original authoring response stays unchanged.'}],
                'evidence_planning': planning, 'tooling': {
                    'capabilities': [package], 'bindings': [{'acquisition_id': audit['id'],
                        'requirement_id': None, 'capability_sha256': package['capability_sha256'],
                        'collector_id': package['collector']['id']}]}}
    provider = {'capability': {'id': package['collector']['id'], 'input_schema': input_schema,
                              'output_schema': output_schema, 'semantics': None},
                'mapping': {'scope_pointers': {}}}
    write_json(directory/'analysis.json', analysis)
    write_json(directory/'evidence-config.json', planning['routes']['configuration'])
    write_json(directory/'package/contract.json', {
        'request_context': json.dumps(operation), 'draft': {'fixtures': complete_fixtures()}})
    write_json(directory/'tooling/generated-native-providers.json', {'native_providers': [provider]})
    write_json(directory/'catalogue-probes.snapshot.json', EXAMPLE.frozen_probe_plan())
    raw = (directory/'catalogue-probes.snapshot.json').read_bytes()
    write_json(directory/'input-identity.json', {
        'simple_example': True, 'example_qualification_probes_sha256': EXAMPLE.hashlib.sha256(raw).hexdigest()})
    return analysis


class CatalogueExampleTests(unittest.TestCase):
    def test_configuration_pins_the_independent_operator_specification(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            EXAMPLE.configuration(directory, directory/'read-only-mcp')
            planning = EXAMPLE.read(directory/'planning.json')
            self.assertEqual(planning['operation_specifications'],
                             {EXAMPLE.METHOD: EXAMPLE.operation_specification()})

    def test_operator_contract_qualifies_despite_model_paraphrase_and_unit_annotations(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis = qualification_run(directory)
            retained = (directory/'analysis.json').read_bytes()
            self.assertNotEqual(analysis['evidence_planning']['elaboration']['acquisitions'][1]['operation_spec'],
                                EXAMPLE.operation_specification())
            with mock.patch.object(EXAMPLE, 'run_probes') as probes:
                destination = EXAMPLE.qualify(directory, '/unused-engarg')
            probes.assert_called_once()
            self.assertEqual((directory/'analysis.json').read_bytes(), retained)
            qualified = EXAMPLE.read(destination)['native_providers'][0]['capability']['semantics']
            quantities = {value['name']: value['unit'] for value in qualified['quantities']}
            self.assertEqual(quantities['count'], 'entries')
            self.assertEqual(quantities['snapshot'], '1')
            self.assertEqual(qualified['provenance']['authorities'], ['catalogue-audit'])

    def test_pre_generation_admission_preserves_the_complete_retained_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis = qualification_run(directory)
            analysis.pop('tooling')
            write_json(directory/'analysis.json', analysis)
            retained = (directory/'analysis.json').read_bytes()
            with mock.patch.object(EXAMPLE, 'run_probes') as probes:
                selected = EXAMPLE.admit_operation(directory, directory/'analysis.json')
            self.assertEqual(selected['id'], analysis['evidence_planning']['elaboration']['acquisitions'][1]['id'])
            self.assertEqual((directory/'analysis.json').read_bytes(), retained)
            probes.assert_not_called()

    def test_pre_generation_admission_rejects_unpinned_or_ambiguous_requests_before_receipt(self):
        for defect in ['changed_purpose', 'second_implementation', 'wrong_acquisition', 'changed_schema']:
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                analysis = qualification_run(directory)
                analysis.pop('tooling')
                gaps = analysis['evidence_planning']['routes']['gaps']
                if defect == 'changed_purpose':
                    purpose = json.loads(gaps[0]['implementation']['operation_purpose'])
                    purpose['definition'] = 'Always report three distinct entries.'
                    gaps[0]['implementation']['operation_purpose'] = json.dumps(purpose)
                elif defect == 'second_implementation':
                    extra = copy.deepcopy(gaps[0])
                    extra['acquisition_id'] = 'another_operation'
                    gaps.append(extra)
                elif defect == 'wrong_acquisition':
                    gaps[0]['acquisition_id'] = 'retained_snapshot'
                elif defect == 'changed_schema':
                    gaps[0]['implementation']['input_schema']['properties']['items']['maxItems'] = 31
                write_json(directory/'analysis.json', analysis)
                with mock.patch.object(EXAMPLE, 'run_probes') as probes, self.assertRaises(ValueError):
                    EXAMPLE.admit_operation(directory, directory/'analysis.json')
                probes.assert_not_called()
                self.assertFalse((directory/'tooling-operation-admission.json').exists())

    def test_admission_retains_scope_and_upstream_authority_obligations_without_qualifying_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            analysis = qualification_run(directory)
            source, audit = analysis['evidence_planning']['elaboration']['acquisitions']
            for acquisition in [source, audit]:
                acquisition['contract']['scope']['selectors']['sample_sources'] = [source['id']]
            audit['contract']['provenance']['required_authorities'].append('catalogue-fixture')
            implementation = analysis['evidence_planning']['routes']['gaps'][0]['implementation']
            implementation['support_contract']['provenance']['required_authorities'].append('catalogue-fixture')
            contract = EXAMPLE.read(directory/'package/contract.json')
            contract['request_context'] = json.dumps(implementation)
            write_json(directory/'analysis.json', analysis)
            write_json(directory/'package/contract.json', contract)
            retained = (directory/'analysis.json').read_bytes()
            with mock.patch.object(EXAMPLE.sys, 'stderr', io.StringIO()) as stderr:
                EXAMPLE.admit_operation(directory, directory/'analysis.json')
            receipt = EXAMPLE.read(directory/'tooling-operation-admission.json')
            self.assertCountEqual(receipt['unresolved_route_conditions'], [
                {'acquisition_id': source['id'], 'kind': 'scope_binding_unavailable',
                 'selector': 'sample_sources'},
                {'acquisition_id': audit['id'], 'kind': 'scope_binding_unavailable',
                 'selector': 'sample_sources'},
                {'acquisition_id': audit['id'], 'kind': 'source_authority_unqualified',
                 'authority': 'catalogue-fixture'},
            ])
            self.assertIn('3 retained route conditions', stderr.getvalue())
            self.assertEqual((directory/'analysis.json').read_bytes(), retained)
            with mock.patch.object(EXAMPLE, 'run_probes'):
                destination = EXAMPLE.qualify(directory, '/unused-engarg')
            semantics = EXAMPLE.read(destination)['native_providers'][0]['capability']['semantics']
            self.assertEqual(semantics['provenance']['authorities'], ['catalogue-audit'])
            self.assertEqual(semantics['scope_constraints'], {'snapshot': EXAMPLE.SNAPSHOT})
            self.assertEqual((directory/'analysis.json').read_bytes(), retained)

    def test_cardinality_units_remain_exact_and_source_authority_is_not_inferred(self):
        for count_unit in ['entries', 'count', '1']:
            with self.subTest(unit=count_unit), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                analysis = qualification_run(directory, count_unit)
                audit = analysis['evidence_planning']['elaboration']['acquisitions'][1]
                audit['contract']['provenance']['required_authorities'].append('catalogue-fixture')
                contract = EXAMPLE.read(directory/'package/contract.json')
                operation = json.loads(contract['request_context'])
                operation['support_contract']['provenance']['required_authorities'].append('catalogue-fixture')
                contract['request_context'] = json.dumps(operation)
                analysis['evidence_planning']['routes']['gaps'][0]['implementation'] = copy.deepcopy(operation)
                write_json(directory/'analysis.json', analysis)
                write_json(directory/'package/contract.json', contract)
                with mock.patch.object(EXAMPLE, 'run_probes'):
                    destination = EXAMPLE.qualify(directory, '/unused-engarg')
                semantics = EXAMPLE.read(destination)['native_providers'][0]['capability']['semantics']
                quantities = {quantity['name']: quantity['unit'] for quantity in semantics['quantities']}
                self.assertEqual(quantities['count'], count_unit)
                self.assertEqual(semantics['provenance']['authorities'], ['catalogue-audit'])

    def test_qualification_rejects_changed_authority_binding_method_and_schemas_before_probes(self):
        cases = ['operator_specification', 'frozen_definition', 'frozen_method', 'graph_method',
                 'graph_schema', 'output_schema', 'quantity_unit', 'package_binding',
                 'collector_binding', 'acquisition_binding', 'ambiguous_acquisition', 'unknown_annotation',
                 'frozen_fixture', 'changed_probe_plan', 'unsupported_cardinality_unit', 'support_contract']
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                analysis = qualification_run(directory)
                audit = analysis['evidence_planning']['elaboration']['acquisitions'][1]
                providers = EXAMPLE.read(directory/'tooling/generated-native-providers.json')
                contract = EXAMPLE.read(directory/'package/contract.json')
                operation = json.loads(contract['request_context'])
                purpose = json.loads(operation['operation_purpose'])
                if case == 'operator_specification':
                    analysis['evidence_planning']['routes']['configuration']['operation_specifications'][
                        EXAMPLE.METHOD]['definition'] = 'Always report three distinct entries.'
                elif case == 'frozen_definition':
                    purpose['definition'] = 'Always report three distinct entries.'
                elif case == 'frozen_method':
                    purpose['methods'] = ['unreviewed.audit/1']
                elif case == 'graph_method':
                    audit['contract']['acceptable_methods'] = ['unreviewed.audit/1']
                elif case == 'graph_schema':
                    audit['operation_spec']['input_schema']['properties']['items']['maxItems'] = 31
                elif case == 'output_schema':
                    providers['native_providers'][0]['capability']['output_schema']['properties']['count']['maximum'] = 31
                elif case == 'quantity_unit':
                    providers['native_providers'][0]['capability']['output_schema']['properties']['count'][
                        'x-engarg-quantity']['unit'] = 'milliseconds'
                elif case == 'package_binding':
                    analysis['tooling']['bindings'][0]['capability_sha256'] = 'c'*64
                elif case == 'collector_binding':
                    analysis['tooling']['bindings'][0]['collector_id'] = 'another_collector'
                elif case == 'acquisition_binding':
                    analysis['tooling']['bindings'][0]['acquisition_id'] = 'retained_snapshot'
                elif case == 'ambiguous_acquisition':
                    duplicate = copy.deepcopy(audit)
                    duplicate['id'] = 'another_audit'
                    analysis['evidence_planning']['elaboration']['acquisitions'].append(duplicate)
                elif case == 'unknown_annotation':
                    providers['native_providers'][0]['capability']['input_schema']['properties']['items'][
                        'x-engarg-unreviewed-qualification'] = True
                elif case == 'frozen_fixture':
                    contract['draft']['fixtures'][0]['expected_value']['count'] = 3
                elif case == 'changed_probe_plan':
                    plan = EXAMPLE.read(directory/'catalogue-probes.snapshot.json')
                    plan['cases'][0]['expected_value']['count'] = 3
                    write_json(directory/'catalogue-probes.snapshot.json', plan)
                elif case == 'unsupported_cardinality_unit':
                    audit['contract']['quantities'][1]['unit'] = 'milliseconds'
                elif case == 'support_contract':
                    operation['support_contract']['provenance']['required_authorities'] = ['unreviewed-authority']
                operation['operation_purpose'] = json.dumps(purpose)
                contract['request_context'] = json.dumps(operation)
                write_json(directory/'analysis.json', analysis)
                write_json(directory/'tooling/generated-native-providers.json', providers)
                write_json(directory/'package/contract.json', contract)
                with mock.patch.object(EXAMPLE, 'run_probes') as probes, self.assertRaises(ValueError):
                    EXAMPLE.qualify(directory, '/unused-engarg')
                probes.assert_not_called()
                self.assertFalse((directory/'tooling/demo-qualified-providers.json').exists())

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
            self.assertEqual(command[command.index('--topology-config') + 1], str(config.resolve()))
            self.assertIn('--check-only', command)
            self.assertEqual(command[command.index('--tool-language') + 1], 'rust')
            self.assertEqual(command[command.index('--eal-coordinate') + 1], str((repo / 'eal-coordinate').resolve()))
            self.assertEqual(command[command.index('--coordination-run') + 1], str((repo / 'journal').resolve()))
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
