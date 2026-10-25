#!/usr/bin/env python3
"""Retain paired live flat/graph runs for domain review and measured model usage.

This is an opt-in acceptance experiment. It never substitutes a replay for live
inference or treats structural validity as scientific qualification.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import statistics
import time


CASES = {
    "deployment_identity": (
        'The service deployment at revision abc123 in development cluster dev serves image digest sha256:' + 'a' * 64,
        {"observation", "source_retrieval"},
    ),
    "performance": (
        'For revision abc123 on an eight-core reference host, the arithmetic mean of request latency over 1000 completed requests at concurrency 8 is below 50 ms.',
        {"experiment", "observation", "calculation"},
    ),
    "statistical_reliability": (
        'For revision abc123 under the specified independent Bernoulli workload, the one-sided 95 percent lower confidence bound on request success probability exceeds 0.999.',
        {"experiment", "calculation"},
    ),
    "api_conformance": (
        'Every documented operation in API contract revision abc123 has a passing request and response conformance test against deployment revision def456.',
        {"experiment", "observation", "proof_check"},
    ),
    "numerical_result": (
        'The exact rational sum of 1/3 and 1/6 equals 1/2.',
        {"calculation", "proof_check"},
    ),
    "design_judgement": (
        'For the bounded design review at revision abc123, an independent reviewer judges that a durable queue is preferable to synchronous calls under the stated recovery and latency requirements.',
        {"judgement"},
    ),
}


def summarise(report, expected):
    calls = report.get("transcript", [])
    responses = [call.get("response", {}) for call in calls]
    requirements = (report.get("evidence_planning") or {}).get("elaboration", {}).get("requirements", [])
    types = sorted({r["contract"]["support_type"] for r in requirements if r.get("contract")})
    def total(name):
        values = [r.get(name) for r in responses]
        return sum(values) if values and all(type(v) is int and v >= 0 for v in values) else None
    return {
        "calls": len(calls),
        "input_tokens": total("input_tokens"),
        "output_tokens": total("output_tokens"),
        "models": sorted({r["model"] for r in responses if r.get("model")}),
        "support_types": types,
        "expected_support_type_present": bool(expected.intersection(types)),
        "requirements_with_gaps": [r["id"] for r in requirements if r.get("gaps")],
        "domain_adequacy": "requires independent review of retained requirements and criteria",
    }


def comparisons(rows):
    """Pair complete measurements without hiding failures or claiming adequacy."""
    pairs = {}
    for row in rows:
        pairs.setdefault((row["case"], row["repetition"]), {})[row["mode"]] = row
    result = {}
    for metric in ("elapsed_seconds", "calls", "input_tokens", "output_tokens"):
        ratios = []
        for pair in pairs.values():
            if set(pair) != {"flat", "graph"} or not all(row["structurally_accepted"] for row in pair.values()):
                continue
            baseline, candidate = pair["flat"].get(metric), pair["graph"].get(metric)
            if baseline is not None and candidate is not None and baseline > 0:
                ratios.append(candidate / baseline)
        result[metric] = {"paired_samples": len(ratios), "median_graph_to_flat_ratio": statistics.median(ratios) if ratios else None}
    result["qualification"] = "descriptive only; compare domain adequacy independently before choosing a topology"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--evidence-config", type=Path, required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--api-key-env", default="OPENAI_API_KEY")
    parser.add_argument("--case", choices=sorted(CASES), action="append")
    args = parser.parse_args()
    if not 1 <= args.repetitions <= 10:
        parser.error("repetitions must be between 1 and 10")
    if not os.environ.get(args.api_key_env):
        parser.error(f"{args.api_key_env} must be configured for live inference")
    binary = args.binary.resolve(strict=True)
    config = args.evidence_config.resolve(strict=True)
    args.output_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    rows = []
    identity = {
        "schema": "engarg.support-domain-experiment/1",
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
        "model_requested": args.model,
        "measurement": "whole CLI elapsed time and provider-reported input/output tokens, including repairs",
        "cache_treatment": "provider caching uncontrolled and billing classes unavailable",
        "rows": rows,
    }
    for name in args.case or CASES:
        claim, expected = CASES[name]
        for repeat in range(args.repetitions):
            # Alternate order to reduce systematic temporal drift between arms.
            for mode in (["flat", "graph"] if repeat % 2 == 0 else ["graph", "flat"]):
                stem = args.output_dir / f"{name}-{repeat + 1}-{mode}"
                command = [str(binary), "analyse", "--claim", claim, "--model", args.model,
                           "--api-key-env", args.api_key_env, "--api", "responses", "--mode", "hybrid",
                           "--evidence-planning", mode, "--max-calls", "7", "--max-repairs", "1",
                           "--timeout-secs", "10800", "--request-timeout-secs", "1800",
                           "--output", str(stem.with_suffix(".json")), "--audit", str(stem.with_suffix(".audit.jsonl"))]
                if mode == "graph":
                    command.extend(["--evidence-config", str(config)])
                started = time.monotonic()
                try:
                    with stem.with_suffix(".log").open("xb") as log:
                        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=10830)
                    row = {"case": name, "repetition": repeat + 1, "mode": mode, "returncode": result.returncode,
                           "elapsed_seconds": time.monotonic() - started, "structurally_accepted": result.returncode == 0}
                    if result.returncode == 0:
                        row.update(summarise(json.loads(stem.with_suffix(".json").read_text()), expected))
                except subprocess.TimeoutExpired:
                    row = {"case": name, "repetition": repeat + 1, "mode": mode, "returncode": None,
                           "elapsed_seconds": time.monotonic() - started, "structurally_accepted": False, "failure": "experiment timeout"}
                if not row["structurally_accepted"]:
                    audit = stem.with_suffix(".audit.jsonl")
                    completed = []
                    if audit.exists():
                        for line in audit.read_text().splitlines():
                            try:
                                event = json.loads(line)
                            except ValueError:
                                continue  # A killed process may leave a final partial record.
                            if event.get("event") == "completion":
                                completed.append(event["record"])
                    row.update(summarise({"transcript": completed}, expected))
                    row["failed_attempt_usage"] = "unknown when the provider returned no usage"
                rows.append(row)
                identity["comparisons"] = comparisons(rows)
                (args.output_dir / "results.json").write_text(json.dumps(identity, indent=2) + "\n")
                print(json.dumps(row), flush=True)
    return int(any(not r["structurally_accepted"] or (r["mode"] == "graph" and not r.get("expected_support_type_present")) for r in rows))


if __name__ == "__main__":
    raise SystemExit(main())
