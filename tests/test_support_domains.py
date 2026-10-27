"""Check experiment accounting without invoking a model or claiming live evidence."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "support_domains", Path(__file__).resolve().parents[1] / "scripts/check-support-domains.py"
)
EXPERIMENT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPERIMENT)


class DomainExperimentTests(unittest.TestCase):
    def test_repairs_and_actual_models_are_included(self):
        report = {"transcript": [
            {"response": {"model": "actual", "input_tokens": 100, "output_tokens": 25}},
            {"response": {"model": "repair", "input_tokens": 40, "output_tokens": 10}},
        ], "evidence_planning": {"elaboration": {"requirements": [
            {"id": "r", "contract": {"support_type": "calculation"}, "gaps": [{"kind": "clarification"}]}
        ]}}}
        measured = EXPERIMENT.summarise(report, {"calculation"})
        self.assertEqual((measured["calls"], measured["input_tokens"], measured["output_tokens"]), (2, 140, 35))
        self.assertEqual(measured["models"], ["actual", "repair"])
        self.assertEqual(measured["requirements_with_gaps"], ["r"])
        self.assertTrue(measured["expected_support_type_present"])

    def test_missing_usage_is_never_reported_as_zero(self):
        for usage in [None, -1, True]:
            measured = EXPERIMENT.summarise({"transcript": [
                {"response": {"input_tokens": 100}}, {"response": {"input_tokens": usage}}
            ], "evidence_planning": None}, set())
            self.assertIsNone(measured["input_tokens"])
            self.assertIsNone(measured["output_tokens"])
        self.assertIsNone(EXPERIMENT.summarise({}, set())["input_tokens"])

    def test_only_complete_successful_pairs_enter_descriptive_ratios(self):
        rows = [
            {"case": "one", "repetition": 1, "mode": "direct", "structurally_accepted": True, "input_tokens": 100},
            {"case": "one", "repetition": 1, "mode": "graph", "structurally_accepted": True, "input_tokens": 80},
            {"case": "two", "repetition": 1, "mode": "direct", "structurally_accepted": True, "input_tokens": 200},
            {"case": "two", "repetition": 1, "mode": "graph", "structurally_accepted": False, "input_tokens": 1},
        ]
        comparison = EXPERIMENT.comparisons(rows)
        self.assertEqual(comparison["input_tokens"], {"paired_samples": 1, "median_graph_to_direct_ratio": 0.8})
        self.assertEqual(comparison["elapsed_seconds"]["paired_samples"], 0)
        self.assertIsNone(comparison["elapsed_seconds"]["median_graph_to_direct_ratio"])


# This fixture exercises orchestration and accounting. It supplies no live model
# or scientific acceptance evidence.
import copy
import json
import os
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import support_study as STUDY


FIXTURE = '''#!/usr/bin/env python3
import json, os, pathlib, sys
args=sys.argv[1:]
def arg(name, default=None):
    return args[args.index(name)+1] if name in args else default
def save(path, value):
    p=pathlib.Path(path); p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(value))
phase=args[0] if args[0] != 'eal' else args[1]
value={'phase':phase, 'model_key_present':bool(os.environ.get('STUDY_FIXTURE_KEY'))}
if phase == 'analyse':
    value.update(argument={'claim':'fixture'}, transcript=[{'response':{'input_tokens':100,'output_tokens':20,'model':'fixture'}}])
    save(arg('--output'), value)
    save(arg('--audit'), {'event':'completion','record':{'response':{'input_tokens':100,'output_tokens':20,'model':'fixture'},'prompt':{'system':'fixture'}}})
elif phase == 'implement':
    output=pathlib.Path(arg('--tool-output-dir')); output.mkdir()
    value.update(transcript=[{'response':{'input_tokens':100,'output_tokens':20}}],tooling={'model_calls':1})
    save(output/'analysis.with-tools.json',value)
    (output/'tooling.audit.jsonl').write_text(json.dumps({'event':'capability_request','call':1,'request':{'system':'implementation'}})+'\\n'+json.dumps({'event':'capability_completion','call':1,'completion':{'input_tokens':5,'output_tokens':2,'model':'fixture-code'}})+'\\n')
else:
    if phase == 'collect':
        value.update(result={'collection_id':'retained-123'}, metrics={'source_requests':1})
    if phase == 'assess':
        value.update(status='supported', collection_id=arg('--collection-id'))
    if phase == 'build':
        save(pathlib.Path(arg('--output-dir'))/'build.json', value)
    else:
        save(arg('--output'),value)
'''


class FullWorkflowStudyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.binary = self.root / 'fixture'
        self.binary.write_text(FIXTURE)
        self.binary.chmod(0o700)
        self.config = self.root / 'config.json'
        self.config.write_text('{}')
        self.tools = self.root / 'tools.toml'
        self.tools.write_text('[tools]')
        self.reference = self.root / 'qualification.json'
        self.reference.write_text('{"measurement_semantics":"fixture only"}')

    def tearDown(self):
        self.temp.cleanup()

    def recipe(self):
        phases = []
        for name in STUDY.PHASES:
            path = '{work}/' + name + '.json'
            verb = name
            argv = ['{binary}', 'eal', verb, '--output', path]
            phase = {'name': name, 'argv': argv, 'timeout_seconds': 5, 'report': path,
                     'assertions': [{'file': path, 'pointer': '/phase', 'equals': verb}]}
            if name == 'build':
                path = '{work}/package/build.json'
                phase['argv'] = ['{binary}', 'build', '--tools', '{tools}', '--output-dir', '{work}/package']
                phase['report'] = path
                phase['assertions'] = [{'file':path,'pointer':'/phase','equals':'build'}]
            if name == 'implement':
                phase.update(argv=['{binary}', 'eal', 'implement', '--input', '{analysis}',
                                   '--tool-output-dir', '{work}/tools', '--tool-max-calls', '2',
                                   '--http-retries', '0', '--tools', '{tools}', '--eal-host', '{eal_host}'],
                             report='{work}/tools/analysis.with-tools.json',
                             audit='{work}/tools/tooling.audit.jsonl',
                             assertions=[{'file':'{work}/tools/analysis.with-tools.json',
                                          'pointer':'/tooling/model_calls','equals':1}])
            if name == 'qualify':
                phase['argv'] = ['{qualifier}', 'qualify', '--output', path]
                phase['qualification_reference'] = 'qualification'
                phase['deterministic_attestation'] = {
                    'reviewer':'fixture-author', 'reason':'The hashed fixture emits local JSON without model calls.',
                    'reference':'qualification'}
            if name == 'collect':
                phase['captures'] = {'collection_id': {'file':path,'pointer':'/result/collection_id'}}
                phase['counters'] = {'source_requests': {'file':path,'pointer':'/metrics/source_requests'}}
            if name == 'assess':
                phase['argv'] += ['--collection-id', '{collection_id}']
                phase['assertions'].append({'file':path,'pointer':'/collection_id','equals':'retained-123'})
                phase['assertions'].append({'file':path,'pointer':'/status','equals':'supported'})
            phases.append(phase)
        return {'phases':phases}

    def freeze(self, mutate=None):
        config = {'schema':'engarg.support-workflow-study/2', 'purpose':'exploratory',
                  'sampling_justification':'One specified algorithm probes the generated route lifecycle.',
                  'cache_treatment':'Fresh isolated workflow stores; provider caches uncontrolled.',
                  'inference_unit':'case','eal_revision':'fixture-only', 'max_model_calls':18,
                  'assets':{'qualifier':str(self.binary),'qualification':str(self.reference)},
                  'cases':{'specified_algorithm':{'partition':'development','genuine_code_gap':True,
                                                  'direct':self.recipe(),'graph':self.recipe()}}}
        if mutate:
            mutate(config)
        path = self.root / 'workflow.json'
        path.write_text(json.dumps(config))
        args = SimpleNamespace(workflow=path, binary=self.binary, evidence_config=self.config, tools=self.tools, eal_host=self.binary,
                               case=['specified_algorithm'], repetitions=1, model='fixture')
        return STUDY.freeze_study(args, EXPERIMENT.CASES)

    def test_full_chain_counts_phase_usage_once_and_binds_collection(self):
        protocol = self.freeze()
        output = self.root / 'runs'
        output.mkdir()
        with patch.dict(os.environ, {'STUDY_FIXTURE_KEY':'fixture-value'}):
            result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        self.assertEqual(len(result['rows']), 2)
        for row in result['rows']:
            self.assertTrue(row['workflow_accepted'], row)
            self.assertEqual(row['input_tokens'], 105)
            self.assertEqual(row['output_tokens'], 22)
            self.assertEqual(row['calls'], 2)
            self.assertEqual([p['phase'] for p in row['phases']], ['analyse', *STUDY.PHASES])
            work = Path(row['analysis']).parent
            self.assertTrue(json.loads(Path(row['analysis']).read_text())['model_key_present'])
            self.assertFalse(json.loads((work/'qualify.json').read_text())['model_key_present'])
            self.assertFalse(json.loads((work/'assess.json').read_text())['model_key_present'])
            self.assertEqual(json.loads((work/'assess.json').read_text())['collection_id'], 'retained-123')
        self.assertEqual(result['comparisons']['input_tokens']['independent_cases'], 1)
        self.assertEqual(result['comparisons']['all_attempt_resources']['input_tokens'], 210)
        self.assertIsNone(result['comparisons']['all_attempt_resources']['request_bytes'])

    def test_failed_assertion_stops_later_phases_and_retains_spend(self):
        def change(config):
            for mode in ['direct','graph']:
                config['cases']['specified_algorithm'][mode]['phases'][2]['assertions'][0]['equals'] = 'wrong'
        protocol = self.freeze(change)
        output = self.root / 'runs'; output.mkdir()
        result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        for row in result['rows']:
            self.assertFalse(row['workflow_accepted'])
            self.assertEqual(row['failed_phase'], 'build')
            self.assertEqual(row['input_tokens'], 105)
            self.assertFalse((Path(row['analysis']).parent/'collect.json').exists())
        self.assertEqual(result['comparisons']['input_tokens']['paired_runs'], 0)
        self.assertEqual(result['comparisons']['all_attempt_resources']['input_tokens'], 210)

    def test_freeze_rejects_excess_budget_and_unbound_qualification(self):
        with self.assertRaisesRegex(ValueError, 'exceeds study budget'):
            self.freeze(lambda c: c.update(max_model_calls=1))
        with self.assertRaisesRegex(ValueError, 'hashed study asset'):
            self.freeze(lambda c: c['assets'].pop('qualification'))
        with self.assertRaisesRegex(ValueError, 'genuine algorithm-gap'):
            self.freeze(lambda c: c['cases']['specified_algorithm'].update(genuine_code_gap=False,
                direct={'phases': self.recipe()['phases'][2:]}, graph={'phases':self.recipe()['phases'][2:]}))

    def test_frozen_input_change_is_rejected_before_commands(self):
        protocol = self.freeze()
        self.config.write_text('{"changed":true}')
        with self.assertRaisesRegex(ValueError, 'input bytes changed'):
            STUDY.verify_protocol(protocol)
        protocol['model_requested'] = 'different'
        with self.assertRaisesRegex(ValueError, 'protocol digest mismatch'):
            STUDY.verify_protocol(protocol)

    def test_review_recomputes_protocol_digest_without_runtime_assets(self):
        protocol = self.freeze()
        output = self.root/'runs'; output.mkdir()
        result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        reviews = STUDY.review_template(result)
        reviews['reviews'] = []
        tampered = copy.deepcopy(result)
        tampered['protocol']['cases'][0]['reference']['obligations'].pop()
        with self.assertRaisesRegex(ValueError, 'protocol digest mismatch'):
            STUDY.score_reviews(tampered, reviews)
        self.binary.unlink()
        self.config.unlink()
        self.reference.unlink()
        self.assertEqual(STUDY.score_reviews(result, reviews)['attempted_runs'], 2)

    def test_captures_cannot_replace_reserved_or_pinned_bindings(self):
        for name in ['binary', 'config', 'model', 'api_key_env', 'analysis', 'work', 'now', 'qualifier']:
            with self.subTest(name=name):
                def change(config):
                    config['cases']['specified_algorithm']['graph']['phases'][0]['captures'] = {
                        name: {'file':'{work}/anything.json', 'pointer':'/value'}}
                with self.assertRaisesRegex(ValueError, 'capture name collides'):
                    self.freeze(change)
                phase = {'name':'collect','captures':{name:{'file':'unused','pointer':'/value'}}}
                bindings = {'binary':str(self.binary),'qualifier':str(self.binary),'api_key_env':'KEY'}
                with patch.object(STUDY, 'run_command') as command:
                    with self.assertRaisesRegex(ValueError, 'capture name collides'):
                        STUDY.execute_phase(phase, bindings, os.environ, self.root)
                    command.assert_not_called()

    def test_qualification_executable_must_select_a_hashed_asset(self):
        def unpin(config):
            config['cases']['specified_algorithm']['graph']['phases'][1]['argv'][0] = str(self.binary)
        with self.assertRaisesRegex(ValueError, 'qualification executable must select a hashed study asset'):
            self.freeze(unpin)
        protocol = self.freeze()
        self.assertEqual(protocol['cases'][0]['graph']['phases'][1]['argv'][0], '{qualifier}')
        self.assertEqual(protocol['assets']['qualifier']['sha256'], STUDY.file_digest(self.binary))
        self.binary.write_text(FIXTURE+'\n# changed helper\n')
        with self.assertRaisesRegex(ValueError, 'input bytes changed'):
            STUDY.verify_protocol(protocol)

    def test_qualification_usage_is_unknown_without_attestation_or_receipt(self):
        def remove(config):
            for mode in ['direct','graph']:
                config['cases']['specified_algorithm'][mode]['phases'][1].pop('deterministic_attestation')
        protocol = self.freeze(remove)
        self.assertIn('EngArg analyse and implement only', protocol['declared_model_call_bound_scope'])
        output = self.root/'runs'; output.mkdir()
        result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        for row in result['rows']:
            self.assertTrue(row['workflow_accepted'])
            self.assertIsNone(row['calls'])
            self.assertIsNone(row['input_tokens'])
            self.assertEqual(row['known_input_tokens'],105)
            qualify = row['phases'][2]
            self.assertEqual(qualify['model_usage_basis']['kind'], 'unavailable')
        receipt = self.root/'usage.json'
        receipt.write_text(json.dumps({'calls':2,'input_tokens':17,'output_tokens':9}))
        usage, basis = STUDY.qualification_usage({'model_usage_receipt':{'file':str(receipt),'pointer':''}})
        self.assertEqual((usage['calls'],usage['input_tokens'],usage['output_tokens']),(2,17,9))
        self.assertEqual(basis['kind'],'reported_usage')
        receipt.write_text(json.dumps({'calls':2,'input_tokens':17,'output_tokens':9,'unknown_usage_attempts':1}))
        usage, _ = STUDY.qualification_usage({'model_usage_receipt':{'file':str(receipt),'pointer':''}})
        self.assertIsNone(usage['input_tokens'])
        self.assertIsNone(usage['output_tokens'])
        self.assertEqual((usage['known_input_tokens'],usage['known_output_tokens']),(17,9))

    def test_frozen_asset_change_keeps_paid_phase_before_stopping(self):
        protocol = self.freeze()
        output = self.root/'runs'; output.mkdir()
        execute = STUDY.execute_phase
        def change_after_analysis(phase, *args):
            measured = execute(phase, *args)
            if phase['name'] == 'analyse':
                self.config.write_text('{"changed_after_paid_phase":true}')
            return measured
        with patch.object(STUDY, 'execute_phase', side_effect=change_after_analysis):
            result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        saved = json.loads((output/'results.json').read_text())
        self.assertEqual(saved,result)
        self.assertEqual(result['study_stopped'],'frozen phase preflight failed')
        self.assertEqual(len(result['rows']),1)
        row = result['rows'][0]
        self.assertEqual([phase['phase'] for phase in row['phases']],['analyse'])
        self.assertEqual(row['failed_phase'],'implement')
        self.assertEqual(row['input_tokens'],100)
        self.assertEqual(result['comparisons']['all_attempt_resources']['input_tokens'],100)

    def test_collection_byte_metrics_use_retained_provider_fields(self):
        example = json.loads((Path(__file__).resolve().parents[1]/'experiments/support-domains/workflow.example.json').read_text())
        collect = next(p for p in example['cases']['specified_algorithm']['graph']['phases'] if p['name']=='collect')
        value = {'result':{'records':{'one':{'details':{'metrics':{
            'reported_request_bytes':123,'reported_response_bytes':456}}}}}}
        self.assertEqual(STUDY.pointer(value,collect['counters']['request_bytes']['pointer']),[123])
        self.assertEqual(STUDY.pointer(value,collect['counters']['response_bytes']['pointer']),[456])
    def test_unrepresentable_arm_is_a_capability_difference(self):
        protocol = self.freeze(lambda c: c['cases']['specified_algorithm'].update(
            direct={'unrepresentable':True,'reason':'fixture declared capability gap'}))
        output = self.root / 'runs'; output.mkdir()
        result = STUDY.run_workflow(protocol, output, 'STUDY_FIXTURE_KEY')
        self.assertEqual(result['comparisons']['unrepresentable_runs'], 1)
        self.assertEqual(result['comparisons']['input_tokens']['paired_runs'], 0)
        self.assertTrue(result['rows'][0]['capability_difference'])

    def test_process_deadline_and_log_bound_are_observed(self):
        import sys
        value = STUDY.run_command([sys.executable,'-c','import time;time.sleep(10)'], self.root,
                                  self.root/'timeout.log', 0.05, os.environ)
        self.assertEqual(value['failure'], 'phase deadline exceeded')
        self.assertLess(value['elapsed_seconds'], 3)
        value = STUDY.run_command([sys.executable,'-c','print("x"*5000)'], self.root,
                                  self.root/'bounded.log', 5, os.environ, max_bytes=100)
        self.assertEqual(value['failure'], 'phase log limit exceeded')

    def test_implementation_reuse_never_recounts_authoring(self):
        report = {'transcript':[{'response':{'input_tokens':100,'output_tokens':20}}],
                  'tooling':{'model_calls':0}}
        measured = STUDY.model_usage([], report, implementation=True)
        self.assertEqual((measured['calls'], measured['input_tokens'], measured['output_tokens']), (0,0,0))
        report['tooling']['model_calls'] = 1
        measured = STUDY.model_usage([], report, implementation=True)
        self.assertEqual(measured['calls'], 1)
        self.assertIsNone(measured['input_tokens'])

    def test_failed_provider_usage_remains_unknown_with_known_subtotal(self):
        measured = STUDY.model_usage([
            {'event':'capability_request','call':1,'request':{}},
            {'event':'capability_completion','call':1,'completion':{'input_tokens':7,'output_tokens':2}},
            {'event':'capability_request','call':2,'request':{}},
            {'event':'capability_provider_failure','call':2}], implementation=True, failed=True)
        self.assertIsNone(measured['input_tokens'])
        self.assertEqual(measured['known_input_tokens'],7)
        self.assertEqual(measured['unknown_usage_attempts'],1)

    def test_review_rejects_changed_outputs_and_preserves_disagreement(self):
        protocol = self.freeze()
        output=self.root/'runs'; output.mkdir()
        result=STUDY.run_workflow(protocol,output,'STUDY_FIXTURE_KEY')
        template=STUDY.review_template(result)
        first=template['reviews'][0]
        first.update(reviewer='independent-a', independent_of_authoring=True, reasons='Fixture rubric test.')
        first['obligations']={key:'covered' for key in first['obligations']}
        first['errors']={key:0 for key in first['errors']}
        second=copy.deepcopy(first); second['reviewer']='independent-b'
        template['reviews']=[first,second]
        scored=STUDY.score_reviews(result,template)
        self.assertEqual(scored['outcomes'][0]['status'],'adequate')
        self.assertEqual(scored['outcomes'][1]['status'],'requires_review')
        second['obligations']['exactness']='missing'
        second['errors']['method']=1
        self.assertEqual(STUDY.score_reviews(result,template)['outcomes'][0]['status'],'disputed')
        adjudication=copy.deepcopy(first)
        adjudication.update(reviewer='adjudicator',reviewers=['independent-a','independent-b'],
                            reasons='Resolve exactness dispute against the retained operation.')
        template['adjudications']=[adjudication]
        resolved=STUDY.score_reviews(result,template)
        self.assertEqual(resolved['outcomes'][0]['status'],'adjudicated_adequate')
        self.assertTrue(resolved['outcomes'][0]['reviewers_disagree'])
        self.assertEqual(len(resolved['outcomes'][0]['reviews']),2)
        self.assertEqual(resolved['error_prevalence']['method']['flagged_runs'],1)
        second['analysis_sha256']='0'*64
        with self.assertRaisesRegex(ValueError,'retained analysis'):
            STUDY.score_reviews(result,template)
        second['analysis_sha256']=first['analysis_sha256']
        Path(result['rows'][0]['analysis']).write_text('{}')
        with self.assertRaisesRegex(ValueError,'bytes changed'):
            STUDY.score_reviews(result,template)


if __name__ == "__main__":
    unittest.main()
