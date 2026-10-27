#!/usr/bin/env python3
"""Compare frozen finite-claim executions and separately inventory source change.

This is a reproducible measurement harness, not an implementation oracle or a
performance benchmark claim. All tools are local synthetic commands; no model or
external source is contacted. Historical source is read from Git, never copied
into the current executable language contract.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import pathlib
import re
import statistics
import subprocess
import sys
import time

BASELINE_ENGARG = "d181276f39f2189cbadf2654c1d0af2233e5e71e"
BASELINE_EAL = "cf9d43d42e6983c69f7c09fc72cf2881fe7deef9"
ORIGINAL_ENGARG = "79e2903"
ORIGINAL_EAL = "520abf0"
FIXTURE = "examples/executable/finite-inventory/case.eal"
NOW = "2026-10-25T12:00:00Z"
MEASURER = '''import json,sys
operation=sys.argv[1]
request=json.load(sys.stdin)
with open('requests.jsonl','a') as stream:
    stream.write(json.dumps({'operation':operation,'input':request['input']},sort_keys=True)+'\\n')
if operation=='snapshot':
    assert request['input']=={'scenario':'positive'}
    value={'snapshot':'fixture-v1','items':['Alpha','Beta','Gamma'],'complete':True,'manifest_count':3}
else:
    assert operation=='audit'
    items=request['input']['items']
    assert isinstance(items,list) and all(isinstance(item,str) for item in items)
    value={'count':len(items),'distinct':len(items)==len(set(items))}
print(json.dumps({'value':value,'observed_at':'2026-10-25T12:00:00Z'}))
'''


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(repo, *arguments):
    return subprocess.check_output(["git", "-C", str(repo), *arguments])


def frozen(repo, ref, path):
    return git(repo, "show", f"{ref}:{path}")


def source_inventory(repo, ref):
    """Only production Rust/grammar; generated parser, vendor and tests excluded."""
    if ref:
        paths = git(repo, "ls-tree", "-r", "--name-only", ref).decode().splitlines()
    else:
        paths = git(repo, "ls-files", "--cached", "--others", "--exclude-standard").decode().splitlines()
    result = {}
    for name in sorted(set(paths)):
        if not name.startswith(("src/", "crates/", "grammar/")):
            continue
        if not name.endswith((".rs", ".g4")) or any(part in name.split("/") for part in ("tests", "generated", "vendor")):
            continue
        if not ref and not (repo / name).is_file():
            continue
        result[name] = frozen(repo, ref, name) if ref else (repo / name).read_bytes()
    return result


def inventory_summary(files):
    return {
        "production_source_files": len(files),
        "production_source_bytes": sum(len(data) for data in files.values()),
        "production_source_lines": sum(len(data.splitlines()) for data in files.values()),
        "content_tree_sha256": sha(canonical({name: sha(data) for name, data in files.items()})),
    }


def structural(repo, original, baseline):
    current = source_inventory(repo, None)
    snapshots = {}
    for label, ref in (("original_plan_base", original), ("completion_baseline", baseline)):
        before = source_inventory(repo, ref)
        snapshots[label] = {
            "revision": git(repo, "rev-parse", ref).decode().strip(),
            **inventory_summary(before),
            "source_change_fan_out_to_current": {
                "modified": sum(before[p] != current[p] for p in before.keys() & current.keys()),
                "added": len(current.keys() - before.keys()),
                "removed": len(before.keys() - current.keys()),
            },
        }
    snapshots["current_working_tree"] = {
        "head_revision": git(repo, "rev-parse", "HEAD").decode().strip(),
        **inventory_summary(current),
    }
    return snapshots


def obligation_inventory(source):
    """Exact declarative evaluator identity in this bounded, line-oriented fixture.

    Bodies include source identity, typed input/bindings, predicates and age. Thus
    independent verification of an audit result is not conflated with reusing an
    identical evaluator. This intentionally does not count assessment integrity
    recomputation or report runtime evaluator invocation telemetry.
    """
    bodies = re.findall(r"^evidence ([A-Za-z_][A-Za-z_0-9]*) \{\n(.*?)^\}", source, re.M | re.S)
    if len(bodies) != 4:
        raise ValueError("frozen fixture changed: expected four evidence declarations")
    mappings = {}
    for name, body in bodies:
        normalized = "\n".join(line.strip() for line in body.splitlines() if line.strip())
        mappings[name] = {"evaluator_sha256": sha(normalized.encode()),
                          "kind": "pure_derivation" if any(line.strip().startswith("derive ") for line in body.splitlines()) else "acquisition",
                          "predicate_count": sum(line.strip().startswith("require ") for line in body.splitlines())}
    counts = collections.Counter(value["evaluator_sha256"] for value in mappings.values())
    return {"evidence_obligations": len(bodies),
            "acceptance_predicates": sum(item["predicate_count"] for item in mappings.values()),
            "static_duplicate_obligation_evaluator_mappings": sum(count - 1 for count in counts.values()),
            "mappings": mappings}


def manifest():
    blocks = []
    for name, operation in (("snapshot_source", "snapshot"), ("identifier_audit", "audit")):
        blocks.append(f'[tools.{name}]\nkind="command"\nversion="1"\nargv={json.dumps([sys.executable,"tools/measure.py",operation])}\ntimeout_seconds=10\nmax_output_bytes=65536\n')
    return "\n".join(blocks)


def run_one(host, source, directory):
    directory.mkdir(parents=True)
    (directory / "tools").mkdir()
    (directory / "case.eal").write_text(source)
    (directory / "tools.toml").write_text(manifest())
    (directory / "tools/measure.py").write_text(MEASURER)
    authored = ["case.eal", "tools.toml", "tools/measure.py"]
    command = [str(host), "run", "case.eal", "--tools", "tools.toml", "--workspace", "store",
               "--claim", "inventory_verified", "--now", NOW]
    start = time.perf_counter_ns()
    result = subprocess.run(command, cwd=directory, capture_output=True, timeout=30)
    elapsed = time.perf_counter_ns() - start
    (directory / "stdout.json").write_bytes(result.stdout)
    (directory / "stderr.txt").write_bytes(result.stderr)
    envelope = json.loads(result.stdout)
    if result.returncode or envelope.get("is_error"):
        raise RuntimeError(f"execution failed in {directory}: {result.stderr.decode()} {envelope}")
    execution = envelope["result"]
    assessment = execution["assessment"]
    claim = assessment["claims"]["inventory_verified"]
    if claim["status"] != "supported" or not assessment["valid"] or not assessment["complete"]:
        raise RuntimeError(f"finite claim was not completely supported: {claim}")
    records = execution["collection"]["records"]
    requests = [json.loads(line) for line in (directory / "requests.jsonl").read_text().splitlines()]
    unique = collections.Counter(canonical(request) for request in requests)
    if collections.Counter(request["operation"] for request in requests) != {"snapshot": 1, "audit": 1}:
        raise RuntimeError(f"expected exactly one shared snapshot and one audit: {requests}")
    values = {key: record.get("value") for key, record in records.items()}
    source_value = records["snapshot"]["value"]
    if source_value != {"snapshot":"fixture-v1","items":["Alpha","Beta","Gamma"],"complete":True,"manifest_count":3}:
        raise RuntimeError("acquired value differs from independently specified finite snapshot")
    return {
        "command": command, "exit_status": result.returncode,
        "execution_phase_model_calls": 0,
        "model_call_basis": "No model client or provider is configured; all executable bindings are the two retained local command fixtures.",
        "source_requests": len(requests), "requests_per_operation": dict(collections.Counter(request["operation"] for request in requests)),
        "duplicate_acquisition_requests": sum(count - 1 for count in unique.values()),
        "generated_package_files": len(authored),
        "generated_package_bytes": sum((directory / name).stat().st_size for name in authored),
        "logical_eal_source_bytes": (directory / "case.eal").stat().st_size,
        "logical_acquired_snapshot_bytes": len(canonical(source_value)),
        "normalized_observation_value_bytes": len(canonical(values)),
        "store_bytes": sum(path.stat().st_size for path in (directory / "store").rglob("*") if path.is_file()),
        "retained_observation_records": len(records),
        "retained_pure_derivation_records": sum(key in ("count_check", "distinct_check") for key in records),
        "latency_ns": elapsed,
        "assessment": {"valid": assessment["valid"], "complete": assessment["complete"], "claim_status": claim["status"]},
        "observation_values_sha256": sha(canonical(values)),
        "retained_output_sha256": sha(result.stdout),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-host", type=pathlib.Path, required=True)
    parser.add_argument("--current-host", type=pathlib.Path, required=True)
    parser.add_argument("--engarg-repo", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--eal-repo", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--repetitions", type=int, default=5)
    args = parser.parse_args()
    if not 1 <= args.repetitions <= 20:
        parser.error("repetitions must be between 1 and 20")
    output = args.output.absolute()
    output.mkdir(parents=True, exist_ok=False)
    (output / ".gitignore").write_text("/*/store/\n")
    before = frozen(args.engarg_repo, BASELINE_ENGARG, FIXTURE).decode()
    current = (args.engarg_repo / FIXTURE).read_text()
    strip_header = lambda source: re.sub(r'^language .*$', '', source, flags=re.M)
    if strip_header(before) != strip_header(current):
        raise ValueError("finite fixture semantics changed; author and review a common scenario before comparison")
    result = {
        "schema": "engarg.eal4-completion-measurement/1",
        "question": "Does the retrieved fixture-v1 snapshot contain exactly three pairwise distinct case-sensitive string entries?",
        "scope": "One synthetic finite claim, two local command transports, fixed observed and assessment times, fresh stores. No external catalogue/population or model authoring measurement.",
        "fixture_sha256_without_header": sha(strip_header(before).encode()),
        "tool_implementation_sha256": sha(MEASURER.encode()),
        "engarg_baseline": BASELINE_ENGARG,
        "eal_baseline": BASELINE_EAL,
        "obligation_inventory": obligation_inventory(current),
        "structural_source_inventory": {
            "engarg": structural(args.engarg_repo, ORIGINAL_ENGARG, BASELINE_ENGARG),
            "eal": structural(args.eal_repo, ORIGINAL_EAL, BASELINE_EAL),
        },
        "limits": [
            "Latency includes startup, collection, persistence and assessment; repetitions on one host do not establish performance superiority.",
            "Source inventory and source-change fan-out are static counts, not measured operational complexity or authoring cost.",
            "Duplicate evaluator count concerns identical complete authored obligation-to-evaluator mappings. Assessment integrity recomputation is a distinct lifecycle step, not a second semantic model.",
            "Original-plan revisions have source inventory only; no execution timing or historical authoring-call count is inferred.",
            "Store bytes sum logical file lengths after process exit, including SQLite metadata and lock/key files, not allocated disk blocks.",
        ],
        "variants": {},
    }
    variants = {"completion_baseline": (args.baseline_host.resolve(), before), "eal4_current": (args.current_host.resolve(), current)}
    for name, (host, source) in variants.items():
        result["variants"][name] = {"binary_sha256": sha(host.read_bytes()), "source_sha256": sha(source.encode()), "runs": []}
    for repetition in range(args.repetitions):
        order = list(variants) if repetition % 2 == 0 else list(reversed(variants))
        for name in order:
            host, source = variants[name]
            run = run_one(host, source, output / f"{name}-{repetition + 1}")
            result["variants"][name]["runs"].append(run)
    values = {run["observation_values_sha256"] for variant in result["variants"].values() for run in variant["runs"]}
    if len(values) != 1:
        raise RuntimeError("baseline and current normalized observation values differ")
    for variant in result["variants"].values():
        times = [run["latency_ns"] for run in variant["runs"]]
        variant["latency_ns"] = {"minimum": min(times), "median": statistics.median(times), "maximum": max(times)}
    (output / "measurement.json").write_bytes(json.dumps(result, indent=2, sort_keys=True).encode() + b"\n")
    lines = ["# EAL/4 completion measurement", "", result["question"], "", result["scope"], "", "| Observed metric | Frozen completion baseline | Current EAL/4 |", "|---|---:|---:|"]
    names = list(result["variants"])
    for label, key in (("Requests per fresh run", "source_requests"), ("Duplicate acquisition requests", "duplicate_acquisition_requests"), ("Execution model calls", "execution_phase_model_calls"), ("Package files", "generated_package_files"), ("EAL source bytes", "logical_eal_source_bytes"), ("Logical acquired snapshot bytes", "logical_acquired_snapshot_bytes"), ("Retained observation records", "retained_observation_records"), ("Retained pure derivation records", "retained_pure_derivation_records"), ("Store bytes", "store_bytes")):
        cells = [", ".join(map(str, sorted({run[key] for run in result["variants"][name]["runs"]}))) for name in names]
        lines.append(f"| {label} | {cells[0]} | {cells[1]} |")
    latency = [result["variants"][name]["latency_ns"]["median"] / 1_000_000 for name in names]
    lines.append(f"| Median end-to-end milliseconds | {latency[0]:.3f} | {latency[1]:.3f} |")
    lines += ["", f"Both runs use {result['obligation_inventory']['evidence_obligations']} evidence obligations and {result['obligation_inventory']['acceptance_predicates']} acceptance predicates. The independent static inventory found {result['obligation_inventory']['static_duplicate_obligation_evaluator_mappings']} duplicate complete obligation/evaluator mappings. All retained normalized observation values match.", "", "Source inventory and added/modified/removed production-file counts for both historical bases are in `measurement.json`. They are separate from the execution measurements.", "", *[f"- {limit}" for limit in result["limits"]], ""]
    (output / "measurement.md").write_text("\n".join(lines))
    print(json.dumps({"measurement": str(output / "measurement.json"), "report": str(output / "measurement.md")}))


if __name__ == "__main__":
    main()
