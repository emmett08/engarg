"""Compatibility admission for the bounded local catalogue demonstration."""

import json
from pathlib import Path
import sys
import tempfile
import time
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from engarg_catalog_sync.runtime import validate_collector_capabilities, validate_composed_collector


class CollectorCapabilitiesTests(unittest.TestCase):
    def test_matching_marker_is_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            collector = directory / 'collector'
            marker = {'schema': 'eal.collector-capabilities/1',
                      'coverage_proofs': ['identifier_audit_v1'],
                      'qualified_population_quantity': True}
            collector.write_text('#!' + sys.executable + '\nimport sys\n'
                                 'assert sys.argv[1:] == ["capabilities"]\n'
                                 'print(' + repr(json.dumps(marker)) + ')\n')
            collector.chmod(0o700)
            receipt = directory / 'collector-capabilities.json'
            validate_collector_capabilities(str(collector), str(receipt))
            self.assertEqual(json.loads(receipt.read_text()), marker)

    def test_old_malformed_and_oversized_markers_fail_closed(self):
        examples = [
            'import sys; sys.exit(2)',
            'print("{}")',
            'print("{\\"schema\\":\\"eal.collector-capabilities/1\\",\\"schema\\":\\"eal.collector-capabilities/1\\"}")',
            'print("x" * 65537)',
        ]
        for code in examples:
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                collector = directory / 'collector'
                collector.write_text('#!' + sys.executable + '\n' + code + '\n')
                collector.chmod(0o700)
                receipt = directory / 'collector-capabilities.json'
                with self.assertRaises(SystemExit) as error:
                    validate_collector_capabilities(str(collector), str(receipt))
                self.assertIn('does not support the identifier-audit composition', str(error.exception))
                self.assertFalse(receipt.exists())

    def test_timeout_stops_child_holding_stdout_after_parent_exits(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            collector, sentinel = directory / 'collector', directory / 'child-survived'
            child_code = ('import pathlib,time; time.sleep(1); '
                          'pathlib.Path(' + repr(str(sentinel)) + ').write_text("alive")')
            collector.write_text('#!' + sys.executable + '\nimport subprocess, sys\n'
                                 'subprocess.Popen([sys.executable, "-c", ' + repr(child_code) + '])\n')
            collector.chmod(0o700)
            with self.assertRaises(SystemExit):
                validate_collector_capabilities(str(collector), str(directory / 'receipt.json'),
                                                timeout_seconds=0.2)
            time.sleep(1.1)
            self.assertFalse(sentinel.exists(), 'a collector descendant survived the deadline')

    def test_graph_requires_marker_only_for_compiled_coverage_witness(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            collector, invoked = directory / 'collector', directory / 'invoked'
            marker = {'schema': 'eal.collector-capabilities/1',
                      'coverage_proofs': ['identifier_audit_v1'],
                      'qualified_population_quantity': True}
            collector.write_text('#!' + sys.executable + '\nimport pathlib\n'
                                 'pathlib.Path(' + repr(str(invoked)) + ').write_text("yes")\n'
                                 'print(' + repr(json.dumps(marker)) + ')\n')
            collector.chmod(0o700)
            report, receipt = directory / 'analysis.json', directory / 'capabilities.json'
            analysis = {'evidence_planning': {'routes': {'execution': {'acquisitions': [
                {'id': 'plain', 'coverage_proof': None}]}}}}
            report.write_text(json.dumps(analysis))
            validate_composed_collector(str(report), str(collector), str(receipt))
            self.assertFalse(invoked.exists(), 'legacy graphs must not start a capability probe')
            analysis['evidence_planning']['routes']['execution']['acquisitions'].append(
                {'id': 'composed', 'coverage_proof': {'kind': 'identifier_audit_v1'}})
            report.write_text(json.dumps(analysis))
            validate_composed_collector(str(report), str(collector), str(receipt))
            self.assertTrue(invoked.exists())
            self.assertEqual(json.loads(receipt.read_text()), marker)
            analysis['evidence_planning']['routes']['execution']['acquisitions'][-1]['coverage_proof']['kind'] = 'unknown'
            report.write_text(json.dumps(analysis))
            with self.assertRaises(SystemExit):
                validate_composed_collector(str(report), str(collector), str(receipt))

    def test_composed_graph_rejects_incompatible_collector(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            collector = directory / 'old-collector'
            collector.write_text('#!' + sys.executable + '\nimport sys; sys.exit(2)\n')
            collector.chmod(0o700)
            report = directory / 'analysis.json'
            report.write_text(json.dumps({'evidence_planning': {'routes': {'execution': {'acquisitions': [
                {'id': 'composed', 'coverage_proof': {'kind': 'identifier_audit_v1'}}]}}}}))
            with self.assertRaises(SystemExit) as error:
                validate_composed_collector(str(report), str(collector), str(directory / 'receipt.json'))
            self.assertIn('does not support the identifier-audit composition', str(error.exception))


if __name__ == '__main__':
    unittest.main()
