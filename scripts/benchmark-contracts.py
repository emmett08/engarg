#!/usr/bin/env python3
"""Compare immutable baseline/candidate contract probes in alternating process runs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1_048_576), b""):
            digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binaries", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--primary", choices=("admit", "verify"), default="verify")
    parser.add_argument("--baseline-revision", default="HEAD",
                        help="Git revision used to compile the baseline contract implementation")
    parser.add_argument("--build-manifest", type=Path, default=Path("Cargo.toml"),
                        help="manifest snapshot used to compile both arms")
    options = parser.parse_args()
    if not 3 <= options.repetitions <= 20:
        parser.error("--repetitions requires 3..20 independent runs per arm")
    binaries = {
        (arm, profile): options.binaries / f"{arm}-contracts-{profile}"
        for arm in ("baseline", "candidate") for profile in ("debug", "release")
    }
    identities = {f"{arm}_{profile}": sha256(path)
                  for (arm, profile), path in binaries.items()}
    observations = []
    expected = {}
    for profile in ("debug", "release"):
        for mode in ("admit", "verify"):
            for parameters, fixtures in ((1, 3), (64, 16)):
                iterations = {("debug", "admit", 1): 200, ("debug", "admit", 64): 20,
                              ("debug", "verify", 1): 50, ("debug", "verify", 64): 5,
                              ("release", "admit", 1): 1000, ("release", "admit", 64): 100,
                              ("release", "verify", 1): 300, ("release", "verify", 64): 20}[(profile, mode, parameters)]
                key = profile, mode, parameters, fixtures
                for repetition in range(options.repetitions):
                    arms = ("baseline", "candidate") if repetition % 2 == 0 else ("candidate", "baseline")
                    for arm in arms:
                        command = [str(binaries[arm, profile]), mode, str(iterations),
                                   str(parameters), str(fixtures)]
                        started = time.perf_counter_ns()
                        result = subprocess.run(command, check=True, stdout=subprocess.PIPE,
                                                stderr=subprocess.PIPE, text=True, timeout=120)
                        process_ns = time.perf_counter_ns() - started
                        value = json.loads(result.stdout)
                        identity = {name: item for name, item in value.items()
                                    if name not in ("elapsed_ns", "peak_rss_kib")}
                        if identity != expected.setdefault(key, identity):
                            raise ValueError(f"admission/verification outputs differ: {key}, {arm}")
                        observations.append(dict(value, arm=arm, profile=profile,
                                                 repetition=repetition, command=command,
                                                 process_ns=process_ns))
    summary = []
    for profile, mode, parameters, fixtures in expected:
        selected = [row for row in observations if (row["profile"], row["workload"],
                    row["parameters"], row["fixtures"]) == (profile, mode, parameters, fixtures)]
        row = dict(profile=profile, workload=mode, parameters=parameters, fixtures=fixtures)
        for arm in ("baseline", "candidate"):
            arm_rows = [entry for entry in selected if entry["arm"] == arm]
            milliseconds = [entry["elapsed_ns"] / entry["iterations"] / 1e6 for entry in arm_rows]
            process_ms = [entry["process_ns"] / 1e6 for entry in arm_rows]
            rss = [entry["peak_rss_kib"] for entry in arm_rows if entry["peak_rss_kib"] is not None]
            row[arm] = dict(n=len(milliseconds), median_ms=statistics.median(milliseconds),
                            minimum_ms=min(milliseconds), maximum_ms=max(milliseconds),
                            process_median_ms=statistics.median(process_ms),
                            peak_rss_median_kib=statistics.median(rss) if rss else None)
        baseline, candidate = row["baseline"]["median_ms"], row["candidate"]["median_ms"]
        row["absolute_reduction_ms"] = baseline - candidate
        row["relative_reduction_percent"] = 100 * (baseline - candidate) / baseline
        row["meets_15_percent_threshold"] = row["relative_reduction_percent"] >= 15
        baseline_rss = row["baseline"]["peak_rss_median_kib"]
        candidate_rss = row["candidate"]["peak_rss_median_kib"]
        row["peak_rss_change_kib"] = candidate_rss - baseline_rss if baseline_rss is not None else None
        row["meets_memory_threshold"] = (candidate_rss - baseline_rss <= max(16_384, baseline_rss * 0.1)
                                         if baseline_rss is not None else None)
        summary.append(row)
    output = dict(
        schema="engarg.contract-admission-measurement/1",
        sampling=dict(unit="independent process", repetitions=options.repetitions,
                      ordering="AB/BA alternating", warmup="one admission plus four adverse cases before timed loop in every process",
                      primary_workload=options.primary,
                      threshold=f"at least 15% lower median complete {options.primary} time; equal success, input, rejection messages and receipt identities",
                      loop_policy="longer small-contract loops selected after stage-one 10–25 ms samples showed scheduling variability",
                      secondary_threshold="median peak RSS increase at most the greater of 10% and 16 MiB"),
        environment=dict(platform=platform.platform(), cpu_count=os.cpu_count(),
                         cpu_quota=Path("/sys/fs/cgroup/cpu.max").read_text().strip()
                         if Path("/sys/fs/cgroup/cpu.max").exists() else None),
        binary_sha256=identities,
        benchmark_source_sha256=sha256(Path("examples/benchmark_contracts.rs")),
        runner_source_sha256=sha256(Path(__file__)),
        build_options=dict(debug="opt-level=0, debug assertions enabled; sha2 package uses repository opt-level=3",
                           release="opt-level=3, LTO off; no profile changes between arms"),
        compiler=subprocess.check_output(["rustc", "--version"], text=True).strip(),
        revision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        manifest_sha256=sha256(options.build_manifest),
        baseline_revision=subprocess.check_output(
            ["git", "rev-parse", "--verify", f"{options.baseline_revision}^{{commit}}"], text=True).strip(),
        baseline_contract_source_sha256=hashlib.sha256(subprocess.check_output(
            ["git", "show", f"{options.baseline_revision}:src/tooling/contracts.rs"])).hexdigest(),
        candidate_contract_source_sha256=sha256(Path("src/tooling/contracts.rs")),
        baseline_verification_source_sha256=hashlib.sha256(subprocess.check_output(
            ["git", "show", f"{options.baseline_revision}:src/tooling/verification.rs"])).hexdigest(),
        candidate_verification_source_sha256=sha256(Path("src/tooling/verification.rs")),
        candidate_patch=subprocess.check_output(
            ["git", "diff", "--", "src/tooling/contracts.rs", "src/tooling/verification.rs"], text=True),
        summary=summary, observations=observations,
        limitations=["Synthetic bounded contracts; fixture verification uses an in-process runner and performs real durable receipt writes.",
                     "The timed admission boundary excludes input construction and process startup; process_ns records complete probe elapsed time.",
                     "No claim about provider latency, Docker execution or hosted CI duration."],
    )
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
