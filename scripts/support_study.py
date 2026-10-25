"""Frozen, bounded support-workflow experiments and output-bound human reviews.

Recipes contain argv arrays, never shell strings. They specify real operator
configuration and independently qualified measurements. This module does not
infer qualification from a generated package's passing protocol fixtures.
"""
import hashlib
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time

REFERENCE = Path(__file__).resolve().parents[1] / "experiments/support-domains/reference.json"
PHASES = ["implement", "qualify", "register", "replan", "export", "collect", "assess"]
COUNTERS = ("source_requests", "discovery_requests", "request_bytes", "response_bytes",
            "generated_code_count", "reused_sources")
ERRORS = ("critical_omission", "false_readiness", "unit", "method", "scope", "time",
          "lost_challenge", "unjustified_generation")
RESERVED_BINDINGS = {"binary", "config", "model", "api_key_env", "analysis", "work", "now"}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=False).encode()).hexdigest()


def file_digest(path):
    hasher = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w") as target:
        os.chmod(temporary, 0o600)
        json.dump(value, target, indent=2, ensure_ascii=False)
        target.write("\n")
    temporary.replace(path)


def integer(value):
    return type(value) is int and value >= 0


def total_or_unknown(values):
    values = list(values)
    return sum(values) if values and all(integer(v) for v in values) else None


def pointer(value, path):
    """Exact JSON pointer with an explicit * aggregate over array/object children."""
    if not path:
        return [value]
    if not path.startswith("/"):
        raise ValueError("JSON pointers must be empty or begin with /")
    values = [value]
    for part in path[1:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        following = []
        for item in values:
            if part == "*":
                if isinstance(item, dict):
                    following.extend(item.values())
                elif isinstance(item, list):
                    following.extend(item)
                else:
                    raise ValueError("wildcard pointer reached a scalar")
            elif isinstance(item, dict) and part in item:
                following.append(item[part])
            elif isinstance(item, list) and part.isdigit() and int(part) < len(item):
                following.append(item[int(part)])
            else:
                raise ValueError(f"missing pointer component {part}")
        values = following
    return values


def substitute(value, bindings):
    # Use literal replacement: JSON braces and Python-looking strings are inert.
    if isinstance(value, str):
        for key, replacement in bindings.items():
            value = value.replace("{" + key + "}", str(replacement))
        return value
    if isinstance(value, list):
        return [substitute(item, bindings) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, bindings) for key, item in value.items()}
    return value


def git_identity(directory):
    def git(*args):
        result = subprocess.run(["git", "-C", str(directory), *args], capture_output=True,
                                timeout=10, check=False)
        return result.stdout if result.returncode == 0 else None
    head = git("rev-parse", "HEAD")
    diff = git("diff", "HEAD", "--", "src", "scripts", "Cargo.toml", "Cargo.lock")
    return {"revision": head.decode().strip() if head else None,
            "tracked_diff_sha256": hashlib.sha256(diff).hexdigest() if diff is not None else None}


def validate_recipe(recipe, generated):
    phases = recipe.get("phases", [])
    required = PHASES if generated else PHASES[3:]
    if [phase.get("name") for phase in phases] != required:
        raise ValueError(f"workflow phases must be {required}")
    for phase in phases:
        argv = phase.get("argv")
        if not isinstance(argv, list) or not argv or not all(isinstance(v, str) for v in argv):
            raise ValueError("every phase requires a nonempty argv string array")
        if type(phase.get("timeout_seconds")) is not int or not 1 <= phase["timeout_seconds"] <= 10800:
            raise ValueError("every phase requires a timeout in 1..10800 seconds")
        if not phase.get("assertions"):
            raise ValueError("every phase requires explicit outcome assertions")
        if phase["name"] == "implement":
            if argv[:3] != ["{binary}", "eal", "implement"]:
                raise ValueError("implementation must invoke the selected EngArg binary")
            for flag in ("--tool-max-calls", "--http-retries"):
                if argv.count(flag) != 1:
                    raise ValueError(f"implementation requires exactly one {flag}")
            try:
                calls = int(argv[argv.index("--tool-max-calls") + 1])
                retries = int(argv[argv.index("--http-retries") + 1])
            except (IndexError, ValueError) as error:
                raise ValueError("invalid implementation call budget") from error
            if not 1 <= calls <= 16 or retries != 0:
                raise ValueError("study implementation permits 1..16 calls and zero HTTP retries")
        elif phase["name"] != "qualify":
            verb = "register-tools" if phase["name"] == "register" else phase["name"]
            if argv[:3] != ["{binary}", "eal", verb]:
                raise ValueError(f"{phase['name']} must invoke the selected EngArg command")
        if phase["name"] == "qualify" and not phase.get("qualification_reference"):
            raise ValueError("qualification requires a frozen independent measurement reference")
    return phases


def validate_captures(phase, existing):
    names = set(phase.get("captures", {}))
    if any(not isinstance(name, str) or not name.isidentifier() for name in names):
        raise ValueError("capture names must be identifiers")
    if names.intersection(RESERVED_BINDINGS | set(existing)):
        raise ValueError("capture name collides with a reserved or existing binding")
    return names


def freeze_study(args, cases):
    config = json.loads(args.workflow.read_text())
    if config.get("schema") != "engarg.support-workflow-study/1":
        raise ValueError("unsupported workflow study schema")
    reference = json.loads(REFERENCE.read_text())
    selected = args.case or list(cases)
    if config.get("purpose") not in ("exploratory", "confirmatory"):
        raise ValueError("declare exploratory or confirmatory study purpose")
    for key in ("sampling_justification", "cache_treatment", "inference_unit", "eal_revision"):
        if not isinstance(config.get(key), str) or not config[key].strip():
            raise ValueError(f"study requires {key}")
    if config["inference_unit"] != "case":
        raise ValueError("case is the independent inference unit; repetitions are nested measurements")
    if config["purpose"] == "confirmatory" and not config.get("decision_rule"):
        raise ValueError("confirmatory studies require a frozen decision rule and precision justification")
    maximum = config.get("max_model_calls")
    if not integer(maximum) or maximum == 0:
        raise ValueError("declare a finite positive aggregate max_model_calls budget")
    assets = {}
    for name, filename in config.get("assets", {}).items():
        if name in RESERVED_BINDINGS:
            raise ValueError("study asset name collides with a reserved binding")
        path = (args.workflow.parent / filename).resolve(strict=True)
        assets[name] = {"path": str(path), "sha256": file_digest(path)}
    for name, path in (("binary", args.binary), ("config", args.evidence_config)):
        assets[name] = {"path": str(path.resolve(strict=True)), "sha256": file_digest(path)}
    # Freeze direct planning dependencies as well as the root configuration.
    planning = json.loads(args.evidence_config.read_text())
    for field in ("collector", "connections"):
        if isinstance(planning.get(field), str):
            path = (args.evidence_config.parent / planning[field]).resolve(strict=True)
            assets["planning_" + field] = {"path": str(path), "sha256": file_digest(path)}
    rows = []
    bound = 0
    for name in selected:
        entry = config.get("cases", {}).get(name)
        if not entry or name not in reference["cases"]:
            raise ValueError(f"missing frozen workflow/reference for {name}")
        if entry.get("partition") not in ("development", "holdout"):
            raise ValueError(f"declare development/holdout partition for {name}")
        generated = bool(entry.get("genuine_code_gap"))
        for mode in ("flat", "graph"):
            recipe = entry.get(mode)
            if not recipe:
                raise ValueError(f"missing {mode} recipe for {name}")
            if recipe.get("unrepresentable"):
                if not recipe.get("reason"):
                    raise ValueError("capability differences require a frozen reason")
                continue
            phases = validate_recipe(recipe, generated)
            bindings = set(assets)
            for phase in phases:
                bindings.update(validate_captures(phase, bindings))
                if phase["name"] == "qualify" and phase["qualification_reference"] not in assets:
                    raise ValueError("qualification reference must name a hashed study asset")
                if phase["name"] == "qualify":
                    if phase["argv"][0] not in {"{" + name + "}" for name in assets}:
                        raise ValueError("qualification executable must select a hashed study asset")
                    attestation = phase.get("deterministic_attestation")
                    if attestation is not None and (
                        not isinstance(attestation, dict) or not attestation.get("reviewer")
                        or not attestation.get("reason") or attestation.get("reference") not in assets
                    ):
                        raise ValueError("deterministic qualification attestation requires a reviewer, reason and hashed reference")
            bound += args.repetitions * (7 + sum(
                int(p["argv"][p["argv"].index("--tool-max-calls") + 1])
                for p in phases if p["name"] == "implement"))
        rows.append({"name": name, "claim": cases[name][0], "expected_support_types": sorted(cases[name][1]),
                     "reference": reference["cases"][name], **entry})
    if not any(row.get("genuine_code_gap") for row in rows):
        raise ValueError("complete-workflow studies require a genuine algorithm-gap case")
    if bound > maximum:
        raise ValueError(f"declared call bound {bound} exceeds study budget {maximum}")
    protocol = {"schema": "engarg.frozen-support-study/1", "design": config,
                "repetitions": args.repetitions, "model_requested": args.model,
                "declared_model_call_bound": bound, "assets": assets, "cases": rows,
                "declared_model_call_bound_scope": "EngArg analyse and implement only; external qualification calls require separate accounting",
                "reference_sha256": file_digest(REFERENCE), "engarg_source": git_identity(REFERENCE.parents[2]),
                "harness_sha256": file_digest(__file__),
                "driver_sha256": file_digest(Path(__file__).with_name("check-support-domains.py")),
                "review_policy": reference["review_policy"],
                "money": "unestimated; a call bound is not a monetary budget and billing classes are unavailable"}
    protocol["sha256"] = digest(protocol)
    return protocol


def verify_protocol_content(protocol):
    """Validate the retained definition without requiring original executables."""
    raw = dict(protocol)
    expected = raw.pop("sha256", None)
    if expected != digest(raw):
        raise ValueError("frozen protocol digest mismatch")


def verify_protocol(protocol):
    verify_protocol_content(protocol)
    for asset in protocol["assets"].values():
        if file_digest(asset["path"]) != asset["sha256"]:
            raise ValueError("frozen input bytes changed")
    if protocol["harness_sha256"] != file_digest(__file__) or protocol["driver_sha256"] != file_digest(Path(__file__).with_name("check-support-domains.py")):
        raise ValueError("harness changed after freezing")


def read_events(path):
    if not path or not Path(path).exists():
        return []
    events = []
    for line in Path(path).read_text().splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            continue  # Interrupted final lines stay retained in the raw audit.
    return events


def model_usage(events, report=None, failed=False, implementation=False):
    """Use phase-local audit once, never inherited report transcripts twice."""
    completions = []
    requests = 0
    failures = 0
    prompt_hashes = []
    for event in events:
        kind = event.get("event", "").removeprefix("capability_")
        if kind == "request":
            requests += 1
            prompt_hashes.append(digest(event.get("request")))
        if kind in ("provider_failure", "failure", "cancelled"):
            failures += 1
        if kind in ("completion", "response_limit"):
            record = event.get("record") or {}
            completion = record.get("response") or event.get("completion") or event
            completions.append(completion)
            if record.get("prompt"):
                prompt_hashes.append(digest(record["prompt"]))
    if not completions and report is not None and not events and not implementation:
        completions = [call.get("response", {}) for call in report.get("transcript", [])]
        prompt_hashes = [digest(call["prompt"]) for call in report.get("transcript", []) if call.get("prompt")]
    unknown_attempts = max(failures, requests - len(completions), int(failed and not failures and requests <= len(completions)))
    count = max(requests, len(completions) + failures)
    declared_calls = (report or {}).get("tooling", {}).get("model_calls") if implementation else None
    if implementation and integer(declared_calls):
        if declared_calls < len(completions):
            raise ValueError("implementation usage exceeds its reported call count")
        unknown_attempts = max(unknown_attempts, declared_calls - len(completions))
        count = declared_calls
    tokens = {}
    for metric in ("input_tokens", "output_tokens"):
        values = [item.get(metric) for item in completions]
        known = sum(v for v in values if integer(v))
        zero_reuse = implementation and integer(declared_calls) and declared_calls == 0 and not failed
        tokens[metric] = 0 if zero_reuse else None if unknown_attempts or not values or any(not integer(v) for v in values) else known
        tokens["known_" + metric] = known
    return {"calls": count if integer(declared_calls) or not unknown_attempts else None, "known_completed_calls": len(completions),
            "unknown_usage_attempts": unknown_attempts, "prompt_sha256": prompt_hashes,
            "models": sorted({item["model"] for item in completions if item.get("model")}), **tokens}


def run_command(argv, directory, log, timeout, environment, max_bytes=16 * 1024 * 1024):
    started = time.monotonic()
    outcome = {"argv": argv, "returncode": None, "failure": None}
    with log.open("xb") as output:
        try:
            process = subprocess.Popen(argv, cwd=directory, env=environment, stdout=output,
                                       stderr=subprocess.STDOUT, start_new_session=True)
        except OSError as error:
            outcome["failure"] = str(error)
        else:
            while process.poll() is None:
                if time.monotonic() - started >= timeout:
                    outcome["failure"] = "phase deadline exceeded"
                elif log.stat().st_size > max_bytes:
                    outcome["failure"] = "phase log limit exceeded"
                if outcome["failure"]:
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        process.wait(timeout=2)
                    break
                time.sleep(0.02)
            outcome["returncode"] = process.returncode
            if log.stat().st_size > max_bytes:
                outcome["failure"] = "phase log limit exceeded"
    outcome["elapsed_seconds"] = time.monotonic() - started
    outcome["log_sha256"] = file_digest(log)
    return outcome


def read_selection(spec):
    if Path(spec["file"]).stat().st_size > 16 * 1024 * 1024:
        raise ValueError("measurement artefact exceeds 16 MiB")
    return pointer(json.loads(Path(spec["file"]).read_text()), spec.get("pointer", ""))


def check_assertion(spec):
    values = read_selection(spec)
    if "equals" in spec:
        return bool(values) and all(value == spec["equals"] for value in values)
    if "any_equals" in spec:
        return any(value == spec["any_equals"] for value in values)
    if spec.get("nonempty"):
        return bool(values) and all(bool(value) for value in values)
    if spec.get("exists"):
        return bool(values)
    raise ValueError("assertion requires equals, any_equals, nonempty or exists")


def qualification_usage(phase):
    """An external helper has no implicit zero-model-call contract."""
    unknown = {"calls": None, "input_tokens": None, "output_tokens": None,
               "known_input_tokens": 0, "known_output_tokens": 0,
               "unknown_usage_attempts": None}
    receipt = phase.get("model_usage_receipt")
    if receipt:
        try:
            values = read_selection(receipt)
            if len(values) != 1 or not isinstance(values[0], dict):
                raise ValueError("qualification usage receipt requires one object")
            value = values[0]
            if "unknown_usage_attempts" in value and not integer(value["unknown_usage_attempts"]):
                raise ValueError("unknown usage attempt count must be a nonnegative integer")
            for field in ("calls", "input_tokens", "output_tokens"):
                unknown[field] = value.get(field) if integer(value.get(field)) else None
            for field in ("input_tokens", "output_tokens"):
                subtotal = value.get("known_" + field)
                unknown["known_" + field] = unknown[field] if unknown[field] is not None else subtotal if integer(subtotal) else 0
            missing = value.get("unknown_usage_attempts")
            unknown["unknown_usage_attempts"] = missing if integer(missing) else 0 if all(unknown[k] is not None for k in ("calls", "input_tokens", "output_tokens")) else None
            if integer(missing) and missing > 0:
                unknown["input_tokens"] = None
                unknown["output_tokens"] = None
            return unknown, {"kind": "reported_usage", "receipt": receipt}
        except (ValueError, KeyError, OSError) as error:
            return unknown, {"kind": "unavailable_receipt", "error": str(error)}
    attestation = phase.get("deterministic_attestation")
    if isinstance(attestation, dict) and all(attestation.get(k) for k in ("reviewer", "reason", "reference")):
        return {key: 0 for key in unknown}, {"kind": "operator_deterministic_attestation", **attestation}
    return unknown, {"kind": "unavailable", "reason": "external qualification model usage was not measured or attested"}


def execute_phase(phase, bindings, environment, work):
    validate_captures(phase, bindings)
    phase = substitute(phase, bindings)
    name = phase["name"]
    env = dict(environment)
    if name not in ("analyse", "implement"):
        env.pop(bindings["api_key_env"], None)
    result = run_command(phase["argv"], work, work / (name + ".log"),
                         phase["timeout_seconds"], env)
    result["phase"] = name
    result["log"] = str(work / (name + ".log"))
    result["accepted"] = result["returncode"] == 0 and result["failure"] is None
    try:
        result["assertions"] = [check_assertion(item) for item in phase["assertions"]]
        result["accepted"] &= all(result["assertions"])
        captures = {}
        for key, spec in phase.get("captures", {}).items():
            values = read_selection(spec)
            if len(values) != 1 or not isinstance(values[0], (str, int, float, bool)):
                raise ValueError("capture requires exactly one scalar")
            captures[key] = values[0]
        if result["accepted"]:
            bindings.update(captures)
    except (ValueError, KeyError, OSError) as error:
        result["accepted"] = False
        result["assertion_error"] = str(error)
    result["resources"] = {}
    for counter in COUNTERS:
        spec = phase.get("counters", {}).get(counter)
        try:
            # Known zeros describe command boundaries; provider I/O without a
            # supplied counter stays unknown. Qualification helpers are external.
            zero = name in ("register", "export", "assess") or (
                name == "replan" and counter in ("source_requests", "generated_code_count", "reused_sources"))
            if spec:
                values = read_selection(spec)
                result["resources"][counter] = sum(value == spec["count_equals"] for value in values) if "count_equals" in spec else total_or_unknown(values)
            else:
                result["resources"][counter] = 0 if zero else None
        except (ValueError, KeyError, OSError):
            result["resources"][counter] = None
    if name in ("analyse", "implement"):
        report = None
        if phase.get("report") and Path(phase["report"]).exists():
            try:
                report = json.loads(Path(phase["report"]).read_text())
            except ValueError:
                pass
        result["resources"].update(model_usage(read_events(phase.get("audit")), report,
                                               failed=result["returncode"] != 0,
                                               implementation=name == "implement"))
    elif name == "qualify":
        usage, basis = qualification_usage(phase)
        result["resources"].update(usage)
        result["model_usage_basis"] = basis
    else:
        result["resources"].update({"calls": 0, "input_tokens": 0, "output_tokens": 0,
                                    "known_input_tokens": 0, "known_output_tokens": 0,
                                    "unknown_usage_attempts": 0})
    result["artefacts"] = {}
    receipt = phase.get("model_usage_receipt", {})
    for path in [phase.get("report"), phase.get("audit"), receipt.get("file"), *phase.get("artefacts", [])]:
        if path and Path(path).is_file():
            result["artefacts"][path] = {"sha256": file_digest(path), "bytes": Path(path).stat().st_size}
    return result


def aggregate_resources(phases):
    fields = ("calls", "input_tokens", "output_tokens", "known_input_tokens", "known_output_tokens",
              "unknown_usage_attempts", *COUNTERS)
    return {field: total_or_unknown(p["resources"].get(field) for p in phases) for field in fields}


def workflow_comparisons(rows):
    pairs = {}
    for row in rows:
        pairs.setdefault((row["case"], row["repetition"]), {})[row["mode"]] = row
    result = {"attempted_runs": len(rows),
              "accepted_runs": sum(row["workflow_accepted"] for row in rows),
              "unrepresentable_runs": sum(row.get("capability_difference", False) for row in rows),
              "all_attempt_resources": aggregate_resources([p for row in rows for p in row["phases"]]),
              "inference_unit": "case; repeated runs are nested within cases",
              "qualification": "conditional on workflow success; semantic adequacy requires independent review"}
    for metric in ("elapsed_seconds", "calls", "input_tokens", "output_tokens", *COUNTERS):
        per_case = {}
        for (case, _), pair in pairs.items():
            if set(pair) != {"flat", "graph"} or not all(row["workflow_accepted"] for row in pair.values()):
                continue
            left, right = pair["flat"].get(metric), pair["graph"].get(metric)
            if left is not None and right is not None and left > 0:
                per_case.setdefault(case, []).append(right / left)
        means = [statistics.mean(values) for values in per_case.values()]
        result[metric] = {"paired_runs": sum(map(len, per_case.values())), "independent_cases": len(means),
                          "per_case_mean_ratios": {case: statistics.mean(v) for case, v in per_case.items()},
                          "median_case_ratio": statistics.median(means) if means else None,
                          "case_range": [min(means), max(means)] if means else None,
                          "confidence_interval": None,
                          "uncertainty": "purposive exploratory cases do not identify a population sampling distribution"}
    return result


def run_workflow(protocol, output, key_env):
    verify_protocol(protocol)
    rows = []
    result = {"schema": "engarg.support-domain-experiment/2", "protocol": protocol, "rows": rows}

    def retain(row, started=None):
        if started is not None:
            row["elapsed_seconds"] = time.monotonic() - started
        row.update(aggregate_resources(row["phases"]))
        if row.get("analysis") and Path(row["analysis"]).exists():
            row["analysis_sha256"] = file_digest(row["analysis"])
        row["workflow_sha256"] = digest(row["phases"])
        result["comparisons"] = workflow_comparisons(rows)
        write_json(output / "results.json", result)

    initial = {name: asset["path"] for name, asset in protocol["assets"].items()}
    initial.update(model=protocol["model_requested"], api_key_env=key_env)
    for case in protocol["cases"]:
        for repetition in range(1, protocol["repetitions"] + 1):
            for mode in (["flat", "graph"] if repetition % 2 else ["graph", "flat"]):
                recipe = case[mode]
                row = {"case": case["name"], "mode": mode, "repetition": repetition,
                       "partition": case["partition"], "phases": [], "workflow_accepted": False}
                rows.append(row)
                if recipe.get("unrepresentable"):
                    row.update(capability_difference=True, reason=recipe["reason"], elapsed_seconds=0)
                else:
                    work = (output / f"{case['name']}-{repetition}-{mode}").resolve()
                    work.mkdir(mode=0o700)
                    bindings = {**initial, "work": str(work), "analysis": str(work / "analysis.json")}
                    row["analysis"] = bindings["analysis"]
                    command = ["{binary}", "analyse", "--claim", case["claim"], "--model", "{model}",
                               "--api-key-env", key_env, "--api", "responses", "--mode", "hybrid",
                               "--evidence-planning", mode, "--max-calls", "7", "--max-repairs", "1",
                               "--http-retries", "0", "--timeout-secs", "10800", "--request-timeout-secs", "1800",
                               "--output", "{analysis}", "--audit", "{work}/analysis.audit.jsonl"]
                    if mode == "graph":
                        command.extend(["--evidence-config", "{config}"])
                    analysis = {"name": "analyse", "argv": command, "timeout_seconds": 10830,
                                "report": "{analysis}", "audit": "{work}/analysis.audit.jsonl",
                                "assertions": [{"file": "{analysis}", "pointer": "/argument", "nonempty": True}]}
                    started = time.monotonic()
                    for phase in [analysis, *recipe["phases"]]:
                        try:
                            verify_protocol(protocol)
                            validate_captures(phase, bindings)
                        except (ValueError, OSError) as error:
                            row["failed_phase"] = phase["name"]
                            row["preflight_failure"] = str(error)
                            result["study_stopped"] = "frozen phase preflight failed"
                            retain(row, started)
                            return result
                        bindings["now"] = datetime.now(timezone.utc).isoformat()
                        measured = execute_phase(phase, bindings, os.environ, work)
                        row["phases"].append(measured)
                        retain(row, started)
                        if not measured["accepted"]:
                            row["failed_phase"] = phase["name"]
                            break
                    row["workflow_accepted"] = len(row["phases"]) == 1 + len(recipe["phases"]) and all(p["accepted"] for p in row["phases"])
                    retain(row, started)
                retain(row)
    return result


def review_template(result):
    references = {case["name"]: case["reference"] for case in result["protocol"]["cases"]}
    return {"schema": "engarg.support-domain-reviews/1", "protocol_sha256": result["protocol"]["sha256"],
            "reviews": [{"case": row["case"], "mode": row["mode"], "repetition": row["repetition"],
                         "analysis_sha256": row["analysis_sha256"], "reviewer": "",
                         "workflow_sha256": row["workflow_sha256"],
                         "independent_of_authoring": False,
                         "obligations": {item["id"]: "unreviewed" for item in references[row["case"]]["obligations"]},
                         "errors": {name: None for name in ERRORS}, "reasons": ""}
                        for row in result["rows"] if row.get("analysis_sha256")]}


def score_reviews(result, document):
    verify_protocol_content(result["protocol"])
    if document.get("schema") != "engarg.support-domain-reviews/1" or document.get("protocol_sha256") != result["protocol"]["sha256"]:
        raise ValueError("reviews must identify the frozen protocol")
    reference = {case["name"]: case["reference"] for case in result["protocol"]["cases"]}
    keyed = {(r["case"], r["mode"], r["repetition"]): r for r in result["rows"]}
    scored = {}
    for review in document["reviews"]:
        key = review["case"], review["mode"], review["repetition"]
        row = keyed.get(key)
        if not row or review.get("analysis_sha256") != row.get("analysis_sha256"):
            raise ValueError("review does not identify the retained analysis")
        if review.get("workflow_sha256") != row.get("workflow_sha256") or digest(row["phases"]) != row.get("workflow_sha256"):
            raise ValueError("review does not identify the retained workflow")
        if file_digest(row["analysis"]) != row["analysis_sha256"]:
            raise ValueError("retained analysis bytes changed")
        for phase in row["phases"]:
            if file_digest(phase["log"]) != phase["log_sha256"]:
                raise ValueError("retained phase log bytes changed")
            for filename, identity in phase["artefacts"].items():
                if file_digest(filename) != identity["sha256"]:
                    raise ValueError("retained workflow artefact bytes changed")
        if not review.get("reviewer") or review.get("independent_of_authoring") is not True or not review.get("reasons"):
            raise ValueError("review requires independent reviewer identity and reasons")
        required = {item["id"] for item in reference[key[0]]["obligations"]}
        labels = review.get("obligations", {})
        if set(labels) != required or any(v not in ("covered", "partial", "missing", "unrepresentable") for v in labels.values()):
            raise ValueError("every frozen obligation requires a review label")
        errors = review.get("errors", {})
        if set(errors) != set(ERRORS) or any(not integer(v) for v in errors.values()):
            raise ValueError("every error class requires a nonnegative count")
        previous = scored.setdefault(key, [])
        if any(item["reviewer"] == review["reviewer"] for item in previous):
            raise ValueError("duplicate reviewer for one run")
        previous.append({**review, "coverage_numerator": sum(v == "covered" for v in labels.values()),
                         "coverage_denominator": len(required),
                         "adequate": all(v == "covered" for v in labels.values()) and not any(errors.values())})
    adjudications = {}
    for review in document.get("adjudications", []):
        key = review["case"], review["mode"], review["repetition"]
        originals = scored.get(key, [])
        row = keyed.get(key)
        if len(originals) < 2 or not row or key in adjudications:
            raise ValueError("adjudication requires one identified pair of independent reviews")
        reviewers = {item["reviewer"] for item in originals}
        if set(review.get("reviewers", [])) != reviewers or not review.get("reviewer") or review["reviewer"] in reviewers:
            raise ValueError("adjudicator must identify all original reviewers and be independent of them")
        if review.get("analysis_sha256") != row["analysis_sha256"] or review.get("workflow_sha256") != row["workflow_sha256"]:
            raise ValueError("adjudication must identify the retained outputs")
        required = {item["id"] for item in reference[key[0]]["obligations"]}
        labels, errors = review.get("obligations", {}), review.get("errors", {})
        if not review.get("reasons") or review.get("independent_of_authoring") is not True:
            raise ValueError("adjudication requires independent identity and reasons")
        if set(labels) != required or any(v not in ("covered", "partial", "missing", "unrepresentable") for v in labels.values()):
            raise ValueError("adjudication must label every frozen obligation")
        if set(errors) != set(ERRORS) or any(not integer(v) for v in errors.values()):
            raise ValueError("adjudication must count every error class")
        adjudications[key] = {**review, "adequate": all(v == "covered" for v in labels.values()) and not any(errors.values())}
    outcomes = []
    for key, row in keyed.items():
        reviews = scored.get(key, [])
        signatures = {digest({"obligations": item["obligations"], "errors": item["errors"]}) for item in reviews}
        status = "requires_review" if len(reviews) < 2 else "disputed" if len(signatures) != 1 else "adequate" if all(r["adequate"] for r in reviews) else "inadequate"
        adjudication = adjudications.get(key)
        if adjudication:
            status = "adjudicated_adequate" if adjudication["adequate"] else "adjudicated_inadequate"
        outcomes.append({"case": key[0], "mode": key[1], "repetition": key[2], "status": status,
                         "reviews": reviews, "adjudication": adjudication,
                         "reviewers_disagree": len(signatures) > 1,
                         "workflow_accepted": row["workflow_accepted"]})
    reviewed = [row for row in outcomes if row["reviews"]]
    return {"outcomes": outcomes, "required_reviews_per_run": 2,
            "adequate_runs": sum(row["status"] in ("adequate", "adjudicated_adequate") for row in outcomes),
            "attempted_runs": len(outcomes),
            "error_prevalence": {error: {"flagged_runs": sum(any(r["errors"][error] > 0 for r in row["reviews"]) for row in reviewed),
                                          "reviewed_runs": len(reviewed),
                                          "meaning": "at least one original reviewer flagged this class; includes unresolved disagreements"}
                                 for error in ERRORS},
            "qualification": "descriptive case-level review; disagreements and unreviewed outputs remain unresolved"}
