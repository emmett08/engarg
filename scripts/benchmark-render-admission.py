#!/usr/bin/env python3
"""Compare bounded JSON admission in independent CLI processes, without models."""

import argparse
import hashlib
import json
import platform
from pathlib import Path
import resource
import statistics
import subprocess
import tempfile
import time


def identity(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(65536), b""):
            digest.update(chunk)
    return {"bytes": path.stat().st_size, "sha256": digest.hexdigest()}


def sample(program, source, expected):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.perf_counter()
    result = subprocess.run([str(program), "render", "--input", str(source)],
                            capture_output=True, timeout=15)
    elapsed = time.perf_counter() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    if result.returncode != 1 or result.stdout or expected not in result.stderr:
        raise RuntimeError("CLI did not preserve the expected admission failure")
    return {"elapsed_s": elapsed,
            "cpu_user_s": after.ru_utime - before.ru_utime,
            "cpu_system_s": after.ru_stime - before.ru_stime,
            "exit": result.returncode,
            "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
            "stderr_sha256": hashlib.sha256(result.stderr).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=5)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 3 <= args.pairs <= 30:
        parser.error("--pairs must be 3 through 30")
    programs = {arm: path.resolve(strict=True) for arm, path in
                [("baseline", args.baseline), ("candidate", args.candidate)]}
    pins = {arm: identity(path) for arm, path in programs.items()}
    samples, warmups = [], []
    with tempfile.TemporaryDirectory(prefix="engarg-render-profile-") as temporary:
        root = Path(temporary)
        workloads = []
        for size in [1024, 1_048_576, 16 * 1_048_576, 16 * 1_048_576 + 1]:
            path = root / str(size)
            path.write_bytes(b"{}" + b" " * (size - 2))
            expected = (b"saved report requires request" if size <= 16 * 1_048_576
                        else b"exceeds")
            workloads.append((size, path, expected))
        for size, path, expected in workloads:
            for arm, program in programs.items():
                warmups.append({"arm": arm, "input_bytes": size,
                                **sample(program, path, expected)})
            for pair in range(args.pairs):
                order = ["baseline", "candidate"] if pair % 2 == 0 else ["candidate", "baseline"]
                for arm in order:
                    samples.append({"pair": pair, "arm": arm, "input_bytes": size,
                                    **sample(programs[arm], path, expected)})
    summary = []
    for size in [1024, 1_048_576, 16 * 1_048_576, 16 * 1_048_576 + 1]:
        arms = {}
        outcomes = set()
        for arm in programs:
            rows = [row for row in samples if row["arm"] == arm and row["input_bytes"] == size]
            values = [row["elapsed_s"] for row in rows]
            arms[arm] = {"median_s": statistics.median(values),
                         "minimum_s": min(values), "maximum_s": max(values),
                         "median_cpu_user_s": statistics.median(row["cpu_user_s"] for row in rows),
                         "median_cpu_system_s": statistics.median(row["cpu_system_s"] for row in rows)}
            outcomes.update((row["exit"], row["stdout_sha256"], row["stderr_sha256"]) for row in rows)
        if len(outcomes) != 1:
            raise RuntimeError("Baseline and candidate admission outcomes differ")
        baseline, candidate = arms["baseline"]["median_s"], arms["candidate"]["median_s"]
        summary.append({"input_bytes": size, **arms,
                        "reduction_percent": 100 * (baseline - candidate) / baseline})
    for arm, path in programs.items():
        if identity(path) != pins[arm]:
            raise RuntimeError("A measured executable changed")
    retain = summary[2]["reduction_percent"] >= 50 and (
        summary[0]["candidate"]["median_s"] - summary[0]["baseline"]["median_s"] <= 0.015)
    result = {"schema": "engarg.render-admission-profile/1", "programs": pins,
              "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "toolchain": subprocess.check_output(["rustc", "--version"], text=True).strip(),
              "cargo_manifest_sha256": identity(Path("Cargo.toml"))["sha256"],
              "source_patch_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD", "--", "Cargo.toml"])).hexdigest(),
              "platform": platform.platform(), "independent_unit": "one fresh CLI process",
              "cache": "one process warmup per arm and input; warm filesystem cache",
              "scope": "debug CLI render JSON admission at valid size boundaries and oversized rejection; no inference or EAL execution",
              "threshold": {"large_input_reduction_min_percent": 50,
                            "small_input_regression_max_seconds": 0.015},
              "decision": "retain" if retain else "threshold_not_met",
              "warmups": warmups, "samples": samples, "summary": summary,
              "limitations": "One host, five paired processes per input by default. No release runtime, cold-build, whole-CI or invoice savings claim."}
    with args.output.open("x") as target:
        json.dump(result, target, indent=2)
        target.write("\n")
    print(json.dumps({"decision": result["decision"], "summary": summary}))


if __name__ == "__main__":
    main()
