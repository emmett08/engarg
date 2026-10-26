#!/usr/bin/env python3
"""Compare real ticket-stage input validation in independent warmed child processes.

Both programs receive a deliberately mismatched ticket identity. They must finish
configuration::load and reject that identity before any authoring or provider call.
The caller preserves a baseline executable before rebuilding the candidate.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import tempfile
import time


def identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while data := source.read(65536):
            digest.update(data)
            size += len(data)
    return {"path": str(path), "sha256": digest.hexdigest(), "bytes": size}


def sample(program, request, cwd):
    started = time.perf_counter()
    def expired(_signal, _frame):
        raise TimeoutError("Ticket input-validation child exceeded 30 seconds")
    previous_handler = signal.signal(signal.SIGALRM, expired)
    signal.alarm(30)
    child = None
    try:
        child = subprocess.Popen(
            [str(program), "ticket-stage"], cwd=cwd, start_new_session=True,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        child.stdin.write(request)
        child.stdin.close()
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
        stdout, stderr = child.stdout.read(), child.stderr.read()
        elapsed = time.perf_counter() - started
        if (child.returncode != 1 or stdout
                or b"ticket stage identity changed" not in stderr):
            raise RuntimeError("Programme did not complete the expected input-validation path")
    except BaseException:
        if child is not None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            if child.returncode is None:
                _, status, _ = os.wait4(child.pid, 0)
                child.returncode = os.waitstatus_to_exitcode(status)
        raise
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, previous_handler)
        if child is not None:
            for stream in [child.stdin, child.stdout, child.stderr]:
                stream.close()
    # getrusage reports bytes on macOS and KiB on Linux.
    peak_kib = usage.ru_maxrss / 1024 if os.uname().sysname == "Darwin" else usage.ru_maxrss
    return {"elapsed_s": elapsed, "peak_rss_kib": peak_kib,
            "cpu_s": usage.ru_utime + usage.ru_stime, "exit": child.returncode}


def summary(rows):
    return {metric: {"median": statistics.median(row[metric] for row in rows),
                     "minimum": min(row[metric] for row in rows),
                     "maximum": max(row[metric] for row in rows)}
            for metric in ["elapsed_s", "peak_rss_kib", "cpu_s"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 3 <= args.pairs <= 50:
        parser.error("--pairs requires 3 through 50 independent process pairs")
    baseline, candidate, source = [path.resolve(strict=True) for path in
                                   [args.baseline, args.candidate, args.input]]
    programs = {"baseline": baseline, "candidate": candidate}
    pin = identity(source)
    rows = []
    with tempfile.TemporaryDirectory(prefix="engarg-ticket-input-profile-") as temporary:
        workspace = Path(temporary).resolve()
        config = {
            "schema_version": "engarg.ticket/1", "ticket_id": "hash_profile",
            "request": {"claim": "A bounded hashing profile.", "context": "", "evidence": []},
            "debate": {"participants": 3, "rounds": 1, "topology": {"kind": "ring"}},
            "evidence_config": str(workspace / "unused.json"), "authoring_args": [],
            "eal_host": str(source), "environment": [],
        }
        snapshot = workspace / "snapshot.json"
        snapshot.write_text(json.dumps({"configuration": config,
                                       "pins": [{**pin, "retained": None}],
                                       "coordinator": str(source)}))
        request = json.dumps({
            "schema_version": "eal.ticket-stage/1",
            "ticket_id": "deliberate_mismatch_after_load", "stage": "author",
            "workspace": str(workspace),
            "context": {"spec_path": str(snapshot), "spec_sha256": identity(snapshot)["sha256"]},
            "previous": [], "call_id": "profile:load",
        }).encode()
        warmup = {arm: sample(program, request, workspace) for arm, program in programs.items()}
        for pair in range(args.pairs):
            order = ["baseline", "candidate"] if pair % 2 == 0 else ["candidate", "baseline"]
            for arm in order:
                rows.append({"pair": pair, "arm": arm, **sample(programs[arm], request, workspace)})
    if identity(source) != pin:
        raise RuntimeError("Profile input changed during measurement")
    arms = {arm: summary([row for row in rows if row["arm"] == arm]) for arm in programs}
    baseline_rss = arms["baseline"]["peak_rss_kib"]["median"]
    candidate_rss = arms["candidate"]["peak_rss_kib"]["median"]
    baseline_time = arms["baseline"]["elapsed_s"]["median"]
    candidate_time = arms["candidate"]["elapsed_s"]["median"]
    reduction = 100 * (baseline_rss - candidate_rss) / baseline_rss
    regression = 100 * (candidate_time - baseline_time) / baseline_time
    repo = Path(__file__).resolve().parents[1]
    result = {
        "schema": "engarg.ticket-input-profile/1", "input": pin,
        "programs": {arm: identity(program) for arm, program in programs.items()},
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "source_files": [identity(repo / name) for name in
                         ["src/bounded_file.rs", "src/ticket_commands/configuration.rs"]],
        "source_patch_sha256": hashlib.sha256(subprocess.check_output(
            ["git", "diff", "HEAD", "--", "src/bounded_file.rs", "src/ticket_commands/configuration.rs"], cwd=repo)).hexdigest(),
        "toolchain": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "platform": list(os.uname()), "cache": "one warmup per arm, warm filesystem cache",
        "independent_unit": "fresh ticket-stage child process with a 30-second deadline", "warmup": warmup,
        "threshold": {"peak_rss_reduction_min_percent": 50, "median_elapsed_regression_max_percent": 10},
        "samples": rows, "summary": arms, "peak_rss_reduction_percent": reduction,
        "elapsed_regression_percent": regression,
        "decision": "retain" if reduction >= 50 and regression <= 10 else "threshold_not_met",
    }
    with args.output.open("x") as target:
        json.dump(result, target, indent=2)
        target.write("\n")
    print(json.dumps({key: result[key] for key in
                      ["decision", "peak_rss_reduction_percent", "elapsed_regression_percent", "summary"]}))


if __name__ == "__main__":
    main()
