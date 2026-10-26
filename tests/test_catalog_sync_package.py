"""Direct checks for the extracted operator-data and readiness contracts."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
from engarg_catalog_sync import json_io, readiness, tooling


class OperatorJsonTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="operator JSON ")
        self.addCleanup(temporary.cleanup)
        self.path = Path(temporary.name) / "inputs.json"

    def test_exact_decimal_tokens_remain_distinct_from_lookalike_strings(self):
        original = (
            '{"values":[0.12345678901234567890123456789,1.2300e+42,-0.0,'
            '"1.2300e+42",9007199254740993],"name":"café"}\n'
        ).encode("utf-8")
        self.path.write_bytes(original)

        raw, value = json_io.read(self.path)

        self.assertEqual(raw, original)
        self.assertEqual(json_io.exact_json(value).encode("utf-8"), original.rstrip(b"\n"))
        self.assertIsInstance(value["values"][0], json_io.ExactNumber)
        self.assertIs(type(value["values"][3]), str)
        self.assertEqual(value["values"][4], 9007199254740993)

    def test_limit_applies_to_bytes_and_accepts_the_exact_boundary(self):
        original = '{"name":"café"}'.encode("utf-8")
        self.path.write_bytes(original)
        self.assertEqual(json_io.read(self.path, limit=len(original))[0], original)
        with self.assertRaisesRegex(ValueError, "byte limit"):
            json_io.read(self.path, limit=len(original) - 1)

    def test_duplicate_nested_keys_and_nonfinite_constants_are_rejected(self):
        invalid = [
            '{"scope":{"name":"one","name":"two"}}',
            '{"value":NaN}',
            '{"value":Infinity}',
            '{"value":-Infinity}',
        ]
        for original in invalid:
            with self.subTest(original=original):
                self.path.write_text(original)
                with self.assertRaises(ValueError):
                    json_io.read(self.path)

    def test_top_level_arrays_and_scalars_are_rejected(self):
        for value in [[], "scope", 3, True, None]:
            with self.subTest(value=value):
                self.path.write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, "JSON objects"):
                    json_io.read(self.path)

    @unittest.skipUnless(hasattr(os, "O_NOFOLLOW"), "requires no-follow file admission")
    def test_leaf_symlinks_are_rejected_even_when_the_target_is_valid(self):
        self.path.write_text('{"scope":"valid"}')
        alias = self.path.with_name("alias.json")
        alias.symlink_to(self.path)
        with self.assertRaises(OSError):
            json_io.read(alias)

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires named pipes")
    def test_named_pipes_are_rejected_without_waiting_for_a_writer(self):
        os.mkfifo(self.path)
        with self.assertRaisesRegex(ValueError, "regular files"):
            json_io.read(self.path)


class ToolingMergeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="tool analysis ")
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        self.original = directory / "original.json"
        self.enriched = directory / "enriched.json"
        self.final = directory / "final.json"
        self.original.write_text('{"argument":{"number":1.2300e+42,"label":"number"}}\n')
        self.enriched.write_text(
            '{ "tooling": {"schema":"engarg.capability-tooling/1"}, '
            '"argument":{"label":"number","number":1.2300e+42} }\n'
        )

    def test_valid_enrichment_preserves_exact_bytes_and_original_file(self):
        before = self.original.read_bytes()
        tooling.merge_analysis(self.original, self.enriched, self.final)
        self.assertEqual(self.final.read_bytes(), self.enriched.read_bytes())
        self.assertEqual(self.original.read_bytes(), before)

    def test_numerically_equal_tokens_and_lookalike_arrays_cannot_change_analysis(self):
        for token in ["1.23e42", '"1.2300e+42"', '["number","1.2300e+42"]']:
            with self.subTest(token=token):
                self.enriched.write_text(
                    '{"tooling":{"schema":"engarg.capability-tooling/1"},'
                    '"argument":{"number":' + token + ',"label":"number"}}'
                )
                with self.assertRaisesRegex(SystemExit, "changed the original analysis"):
                    tooling.merge_analysis(self.original, self.enriched, self.final)
                self.assertFalse(self.final.exists())

    def test_existing_output_is_never_replaced(self):
        self.final.write_bytes(b"retained earlier analysis\n")
        with self.assertRaises(FileExistsError):
            tooling.merge_analysis(self.original, self.enriched, self.final)
        self.assertEqual(self.final.read_bytes(), b"retained earlier analysis\n")


class ReadinessDependencyTests(unittest.TestCase):
    def routes(self, *names):
        return {
            "routes": [
                {"acquisition_id": name, "selected": "provider_" + name,
                 "configuration_gaps": []}
                for name in names
            ],
            "discovery": {"incomplete": False},
            "execution": {"acquisitions": [], "derivations": []},
        }

    def test_ready_sources_feed_a_derivation_then_a_dependent_acquisition(self):
        routes = self.routes("details", "index")
        routes["execution"] = {
            "acquisitions": [
                {"id": "details", "bindings": [{"output": {"step_id": "identifiers"}}]},
                {"id": "index", "bindings": []},
            ],
            "derivations": [{"id": "identifiers", "inputs": [{"step_id": "index"}]}],
        }
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"index", "details"})
        routes["routes"][1]["configuration_gaps"] = ["credential"]
        self.assertEqual(readiness.ready_acquisition_ids(routes), set())

    def test_fanout_requires_both_discovery_and_witness_control_dependencies(self):
        routes = self.routes("details", "index", "expected")
        routes["execution"]["acquisitions"] = [{
            "id": "details", "for_each": {
                "output": {"step_id": "index"},
                "stop_on_witness": {"equals_output": {"step_id": "expected"}},
            },
        }]
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"details", "index", "expected"})
        routes["routes"][2]["configuration_gaps"] = ["missing_connection"]
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"index"})
        routes["routes"][2]["configuration_gaps"] = []
        routes["routes"][1]["selected"] = None
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"expected"})

    def test_cycles_and_unknown_dependencies_leave_independent_sources_available(self):
        routes = self.routes("first", "second", "unknown", "independent")
        routes["execution"]["acquisitions"] = [
            {"id": "first", "bindings": [{"output": {"step_id": "second"}}]},
            {"id": "second", "bindings": [{"output": {"step_id": "first"}}]},
            {"id": "unknown", "bindings": [{"output": {"step_id": "absent"}}]},
        ]
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"independent"})

    def test_derivation_cycle_cannot_admit_a_downstream_source(self):
        routes = self.routes("details", "independent")
        routes["execution"] = {
            "acquisitions": [
                {"id": "details", "bindings": [{"output": {"step_id": "first"}}]},
            ],
            "derivations": [
                {"id": "first", "inputs": [{"step_id": "second"}]},
                {"id": "second", "inputs": [{"step_id": "first"}]},
            ],
        }
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"independent"})

    def test_incomplete_discovery_blocks_otherwise_configured_sources(self):
        routes = self.routes("index")
        self.assertEqual(readiness.ready_acquisition_ids(routes), {"index"})
        routes["discovery"]["incomplete"] = True
        self.assertEqual(readiness.ready_acquisition_ids(routes), set())


if __name__ == "__main__":
    unittest.main()
