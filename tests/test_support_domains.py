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
            {"case": "one", "repetition": 1, "mode": "flat", "structurally_accepted": True, "input_tokens": 100},
            {"case": "one", "repetition": 1, "mode": "graph", "structurally_accepted": True, "input_tokens": 80},
            {"case": "two", "repetition": 1, "mode": "flat", "structurally_accepted": True, "input_tokens": 200},
            {"case": "two", "repetition": 1, "mode": "graph", "structurally_accepted": False, "input_tokens": 1},
        ]
        comparison = EXPERIMENT.comparisons(rows)
        self.assertEqual(comparison["input_tokens"], {"paired_samples": 1, "median_graph_to_flat_ratio": 0.8})
        self.assertEqual(comparison["elapsed_seconds"]["paired_samples"], 0)
        self.assertIsNone(comparison["elapsed_seconds"]["median_graph_to_flat_ratio"])


if __name__ == "__main__":
    unittest.main()
