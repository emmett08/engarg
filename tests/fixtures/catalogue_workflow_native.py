"""Native acceptance: official MCP SDK, a real custom command and typed EAL/3.

All claims concern the local demonstration snapshot. The command below is a test
fixture, not an AI-generated implementation or evidence about a deployed system.
"""
import copy
import datetime
import importlib.util
import json
from pathlib import Path
import subprocess
import sys


REPOSITORY = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "catalogue_example", REPOSITORY / "scripts/catalogue_evidence_example.py")
example = importlib.util.module_from_spec(spec)
spec.loader.exec_module(example)


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def run(command, request=None):
    result = subprocess.run(command, input=None if request is None else json.dumps(request).encode(),
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
    if result.returncode:
        raise AssertionError("Command failed: " + str(command) + "\n" + result.stderr.decode()
                             + "\n" + result.stdout.decode())
    return json.loads(result.stdout)


def source_identity(capability):
    provider = capability["provider"]
    return {
        "provider_id": provider["id"], "capability_id": capability["id"],
        "capability_version": capability["version"],
        "input_schema_sha256": capability["input_schema_sha256"],
        "output_schema_sha256": capability["output_schema_sha256"],
        "qualified_contract_sha256": example.digest({
            "schema": "eal.measurement-identity/1", "capability_id": capability["id"],
            "capability_version": capability["version"], "provider": provider,
            "input_schema_sha256": capability["input_schema_sha256"],
            "output_schema_sha256": capability["output_schema_sha256"],
            "semantics": capability["semantics"],
        }),
        "configuration_sha256": provider["configuration_sha256"],
        "authentication_scope_sha256": example.digest(provider["authentication_scope"]),
        "scope": {"snapshot": example.SNAPSHOT}, "historical_interval": None,
    }


def acquisition(identifier, capability, arguments, bindings=None):
    return {"id": identifier, "source": source_identity(capability), "arguments": arguments,
            "bindings": bindings or [], "for_each": None,
            "availability": {"kind": "configured"}, "reuse": "fresh_or_acquire",
            "max_age_seconds": 3600}


def evaluation(identifier, pointer, criterion, step="audit"):
    return {"id": "evaluation_" + identifier, "requirement_id": identifier,
            "inputs": [{"step_id": step, "pointer": pointer}], "criterion": criterion,
            "max_age_seconds": 3600, "required_coverage": "unspecified",
            "applicability": [], "challenges": []}


def prepare(directory, collector, server, scenario):
    directory.mkdir()
    items = [{"id": "alpha"}, {"id": "beta"}, {"id": "gamma"}]
    if scenario == "duplicate_identifiers":
        items[2] = {"id": "alpha"}
    write(directory / "catalogue.json", {
        "snapshot": example.SNAPSHOT, "items": items, "complete": True})
    example.configuration(directory, server, collector)
    configuration = example.read(directory / "connections.json")
    # The missing source is observed through an actual SDK tool error. The MCP
    # protocol still initialises, so failure is distinct from a negative audit.
    if scenario == "mcp_unavailable":
        (directory / "catalogue.json").unlink()
    audit = directory / "fixture-audit"
    audit.write_text("#!" + sys.executable + "\n" + '''
import datetime, json, pathlib, sys
request = json.load(sys.stdin)
value = request["input"]
ids = [item["id"] for item in value["items"]]
measurement = {"snapshot": value["snapshot"], "count": len(ids),
               "distinct": len(set(ids)) == len(ids), "complete": value["complete"],
               "typed_input": value["formal_input"]}
observed = datetime.datetime.now(datetime.timezone.utc).isoformat()
path = pathlib.Path(__file__).with_name("audit-requests.jsonl")
with path.open("a") as output:
    output.write(json.dumps({"request": request, "measurement": measurement}) + "\\n")
identity = {key: request[key] for key in ["tool", "tool_version", "input", "context"]}
print(json.dumps({"observed_at": observed, "measurement": measurement, "request": identity}))
''', encoding="utf-8")
    audit.chmod(0o700)
    capability = copy.deepcopy(configuration["local_catalogues"][0]["capabilities"][0])
    capability.update({"id": "catalogue_audit", "description": "Independent finite catalogue audit test fixture",
                       "input_schema": example.AUDIT_INPUT, "input_schema_sha256": example.digest(example.AUDIT_INPUT),
                       "output_schema": example.AUDIT_OUTPUT, "output_schema_sha256": example.digest(example.AUDIT_OUTPUT),
                       "semantics": example.measurement_semantics()})
    capability["provider"] = {
        "id": "catalogue_audit", "version": "1", "transport": "command",
        "connection": None, "tool": None, "authentication_scope": "local-fixture-audit",
        "configuration_sha256": "0" * 64}
    native = {
        "capability": capability,
        "transport": {"kind": "command", "program": str(audit),
                      "supervisor": str(collector.with_name("eal-process-supervisor")),
                      "arguments": [], "cwd": str(directory), "inherit_env": [],
                      "protocol": {"kind": "eal_acquisition", "environment": "catalogue_demo",
                                   "tool": "catalogue_audit", "tool_version": "1", "context": {}}},
        "mapping": {"tool": "catalogue_audit", "raw_output_schema_sha256": None,
                    "response_format": {"kind": "structured"}, "value_pointer": "/measurement",
                    "observation_time": {"kind": "pointer", "pointer": "/observed_at"},
                    "scope_pointers": {"snapshot": "/snapshot"}},
        "pinned_files": [], "credential_env": [],
    }
    write(directory / "native-input.json", native)
    native = run([str(collector), "prepare-native-provider", "--input", str(directory / "native-input.json")])
    configuration["native_providers"] = [native]
    write(directory / "connections.json", configuration)
    # Construct source identities from the authoritative parsed discovery result.
    # Serde fills contract defaults such as coverage.sample_size_pointer; hashing
    # a hand-built partial descriptor would identify a different measurement.
    discovery = run([str(collector), "evidence-discover", "--config",
                     str(directory / "connections.json"), "--configured-only"])
    assert discovery["incomplete"] is False and not discovery["gaps"], discovery
    write(directory / "discovery.json", discovery)
    capabilities = {item["id"]: item for item in discovery["catalogue"]["capabilities"]}
    read_capability = capabilities["catalogue_snapshot"]
    plan = {
        "schema": "eal.evidence-execution-plan/1",
        "acquisitions": [
            acquisition("snapshot", read_capability, {"snapshot": example.SNAPSHOT}),
            acquisition("audit", capabilities["catalogue_audit"],
                        {"snapshot": example.SNAPSHOT, "items": None, "complete": None,
                         "formal_input": example.FORMAL_INPUT},
                        [{"output": {"step_id": "snapshot", "pointer": "/items"}, "argument_pointer": "/items"},
                         {"output": {"step_id": "snapshot", "pointer": "/complete"}, "argument_pointer": "/complete"}]),
        ], "derivations": [],
        "evaluations": [
            evaluation("count", "/count", {"kind": "equals", "expected": 3}),
            evaluation("distinct", "/distinct", {"kind": "equals", "expected": True}),
            evaluation("formal", "/typed_input", {"kind": "method_input", "method": "deductive/1"}),
        ], "scheduling": {"maximum_concurrent_sources": 2, "per_provider": {}},
    }
    write(directory / "plan.json", plan)
    case_digest = example.digest({"claim": example.CLAIM})
    plan_digest = example.digest(plan)
    context = {"engarg_export_digest": case_digest, "engarg_execution_plan_sha256": plan_digest}
    support_selector = lambda requirement: {
        "schema": "engarg.support-acquisition/1", "case_digest": case_digest,
        "execution_plan_sha256": plan_digest, "requirement_id": requirement}
    formal_selector = {
        "schema": "engarg.reasoning-input/1", "case_digest": case_digest,
        "execution_plan_sha256": plan_digest, "requirement_id": "formal",
        "output": {"step_id": "audit", "pointer": "/typed_input"},
        "premise_bindings": [
            {"premise_id": "count", "input_pointer": "/premises/0", "requirement_id": "count",
             "correspondence": {"kind": "accepted_premise"}},
            {"premise_id": "distinct", "input_pointer": "/premises/1", "requirement_id": "distinct",
             "correspondence": {"kind": "accepted_premise"}},
        ],
    }
    encoded = lambda value: json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    source = 'language "EAL/3"\n\nenvironment catalogue_demo {\n  require engarg_export_digest == ' + encoded(case_digest) + '\n}\n\n'
    source += 'tool support_graph {\n  version "1"\n}\n\n'
    for name, selector, kind, predicate in [
        ("count", support_selector("count"), "test", "criterion_satisfied == true"),
        ("distinct", support_selector("distinct"), "test", "criterion_satisfied == true"),
        ("formal", formal_selector, "logical_case", 'schema == "EAL/typed-input/1"'),
    ]:
        source += (f'evidence {name}_evidence {{\n  tool support_graph\n  kind {kind}\n'
                   f'  environment catalogue_demo\n  max_age 3600\n  input {encoded(selector)}\n'
                   f'  require {predicate}\n}}\n\n')
    formal = example.FORMAL_INPUT
    source += ('reasoning catalogue_conjunction {\n  method "deductive/1"\n'
               '  rationale "The separately acquired exact count and distinctness establish the two formal premises for this snapshot."\n'
               '  require consistent_premises == true\n}\n\n'
               'claim catalogue_supported {\n  statement ' + encoded(example.CLAIM) + '\n'
               '  environment catalogue_demo\n  proposition {\n')
    for field in ["subject", "quantity", "unit", "scope", "valid_from", "valid_until"]:
        source += f'    {field} {encoded(formal[field])}\n'
    source += ('    query ' + encoded(formal["payload"]) + '\n    result entailed == true\n  }\n}\n\n'
               'argument catalogue_argument = [evidence count_evidence, distinct_evidence, formal_evidence] via catalogue_conjunction => catalogue_supported binding formal_evidence\n')
    (directory / "argument.eal").write_text(source, encoding="utf-8")
    arguments = [str(collector), "support-graph", "--plan", str(directory / "plan.json"),
                 "--config", str(directory / "connections.json"), "--store", str(directory / "observations")]
    registry = ('[tools.support_graph]\nkind = "command"\nversion = "1"\nargv = '
                + encoded(arguments) + '\ntimeout_seconds = 60\nmax_output_bytes = 1048576\n'
                'inherit_env = []\nparallel_safe = false\n')
    (directory / "tools.toml").write_text(registry, encoding="utf-8")
    return source, context


def assess(directory, collector, host, server, scenario):
    source, context = prepare(directory, collector, server, scenario)
    command = [str(host), "--workspace", str(directory), "--registry", str(directory / "tools.toml"),
               "--timeout", "75"]
    def operation(request):
        envelope = run(command, request)
        assert envelope["is_error"] is False, envelope
        return envelope["result"]
    validation = operation({"operation": "validate", "source": source})
    assert validation["valid"] is True, validation
    collection = operation({"operation": "collect", "source": source, "context": context})
    write(directory / "collection.json", collection)
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    assessment = operation({"operation": "reason", "source": source, "context": context,
                            "collection_id": collection["collection_id"], "now": now})
    write(directory / "assessment.json", assessment)
    assert assessment["valid"] is True, assessment
    claim = assessment["claims"]["catalogue_supported"]
    expected = "supported" if scenario == "positive" else "unsupported"
    assert claim["status"] == expected, assessment
    assert claim["prose_verified"] is False, claim
    computation = assessment["arguments"]["catalogue_argument"]["reasoning_result"]
    if scenario == "positive":
        assert computation["method"] == "deductive/1", computation
        assert computation["details"]["entailed"] is True, computation
        assert computation["details"]["consistent_premises"] is True, computation
        assert computation["binding"]["holds"] is True, computation
        for record in collection["records"].values():
            assert record["status"] == "ok", record
        mcp_calls = [json.loads(line) for line in (directory / "mcp-requests.jsonl").read_text().splitlines()]
        audit_calls = [json.loads(line) for line in (directory / "audit-requests.jsonl").read_text().splitlines()]
        assert len(mcp_calls) == len(audit_calls) == 1
        assert audit_calls[0]["request"]["input"]["items"] == mcp_calls[0]["response"]["measurement"]["items"]
        assert audit_calls[0]["measurement"]["count"] == 3
        assert audit_calls[0]["measurement"]["distinct"] is True
        projected = collection["records"]["formal_evidence"]["details"]["reasoning_input"]
        assert projected["formal_input_correspondence_checked"] is True, projected
        assert projected["premise_truth_verified"] is False, projected
        assert projected["prose_correspondence_verified"] is False, projected
        assert len(projected["source_ids"]) == len(projected["observation_ids"]) == 2, projected
        # Repeated EAL collection reuses the same retained source observations.
        # It does not rerun MCP/custom work or refresh their observation clocks.
        reused = operation({"operation": "collect", "source": source, "context": context})
        write(directory / "collection-reused.json", reused)
        assert len((directory / "mcp-requests.jsonl").read_text().splitlines()) == 1
        assert len((directory / "audit-requests.jsonl").read_text().splitlines()) == 1
        for name, record in collection["records"].items():
            assert reused["records"][name]["status"] == "ok", reused
            assert reused["records"][name]["collected_at"] == record["collected_at"]
        assert reused["records"]["formal_evidence"]["details"]["reasoning_input"]["observation_ids"] == projected["observation_ids"]
    elif scenario == "duplicate_identifiers":
        assert assessment["evidence"]["distinct_evidence"]["status"] == "unavailable", assessment
        assert "predicate_not_met" in assessment["evidence"]["distinct_evidence"]["availability_issues"]
        assert collection["records"]["distinct_evidence"]["status"] == "ok", collection
        assert collection["records"]["distinct_evidence"]["value"]["criterion_satisfied"] is False
        assert computation["status"] == "not_run", computation
    else:
        assert computation["status"] == "not_run", computation
        assert not (directory / "audit-requests.jsonl").exists()
        assert any(record["status"] != "ok" or not record["value"].get("criterion_satisfied", True)
                   for record in collection["records"].values()), collection
    return {"valid": assessment["valid"], "claim_status": claim["status"],
            "computation_status": computation["status"], "collection_id": collection["collection_id"],
            "assessment_id": assessment["assessment_id"]}


if __name__ == "__main__":
    root, collector, host, server = map(Path, sys.argv[1:])
    result = {scenario: assess(root / scenario, collector, host, server, scenario)
              for scenario in ["positive", "duplicate_identifiers", "mcp_unavailable"]}
    print(json.dumps(result))
