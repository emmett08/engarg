#!/usr/bin/env python3
"""Bounded local catalogue example and independently specified tool qualification.

The claim concerns this demonstration snapshot. The extra MCP server reads the
snapshot at collection time. The generated tool computes only a finite identifier
audit. Its qualification is limited to that declared operation and retained input.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def method_scope_identity(scope):
    """The host derives the EAL scope identity, rather than asking a model to hash it."""
    return 'engarg.scope:' + digest({
        'schema': 'engarg.method-scope/1', 'kind': scope['kind'],
        'selectors': {item['name']: item['value'] for item in scope['selectors']},
        'identity_dimensions': sorted(scope['identity_dimensions']),
    })


SNAPSHOT = "demonstration-v1"
METHOD = "catalogue.identifier-audit/1"
CLAIM = "The demonstration-v1 catalogue snapshot contains three entries with distinct identifiers."
METHOD_SCOPE = {
    'kind': 'catalogue_snapshot',
    'selectors': [{'name': 'snapshot', 'value': SNAPSHOT}],
    'identity_dimensions': ['snapshot'],
}
STRING = {"type": "string", "minLength": 1, "maxLength": 64}
ITEMS = {"type": "array", "maxItems": 32, "items": {
    "type": "object", "properties": {"id": STRING}, "required": ["id"], "additionalProperties": False}}
SNAPSHOT_PROPERTY = {"type": "string", "minLength": 1, "maxLength": 128}
READ_INPUT = {"type": "object", "properties": {"snapshot": SNAPSHOT_PROPERTY},
              "required": ["snapshot"], "additionalProperties": False}
FORMAL_INPUT = {
    "schema": "EAL/typed-input/1", "method": "deductive/1", "subject": "catalogue_snapshot",
    "quantity": "proposition", "unit": "1", "scope": method_scope_identity(METHOD_SCOPE),
    "valid_from": "2000-01-01T00:00:00Z", "valid_until": "2100-01-01T00:00:00Z",
    "payload": {"premises": ["count_is_three", "identifiers_distinct"],
                "conclusion": {"and": ["count_is_three", "identifiers_distinct"]}},
}
FORMAL_SCHEMA = {"type": "object", "const": FORMAL_INPUT}
AUDIT_INPUT = {"type": "object", "properties": {
    "snapshot": SNAPSHOT_PROPERTY, "items": ITEMS, "complete": {"type": "boolean"},
    "formal_input": FORMAL_SCHEMA},
    "required": ["snapshot", "items", "complete", "formal_input"], "additionalProperties": False}
AUDIT_OUTPUT = {"type": "object", "properties": {
    "snapshot": SNAPSHOT_PROPERTY, "count": {"type": "integer", "minimum": 0, "maximum": 32},
    "distinct": {"type": "boolean"}, "complete": {"type": "boolean"}, "typed_input": FORMAL_SCHEMA},
    "required": ["snapshot", "count", "distinct", "complete", "typed_input"], "additionalProperties": False}
READ_OUTPUT = {"type": "object", "properties": {
    "snapshot": SNAPSHOT_PROPERTY, "items": ITEMS, "complete": {"type": "boolean"}},
    "required": ["snapshot", "items", "complete"], "additionalProperties": False}


def read(path):
    path = Path(path)
    with path.open("rb") as source:
        raw = source.read(16_777_217)
    if len(raw) > 16_777_216:
        raise ValueError("Example input exceeds its byte limit")

    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise ValueError("Duplicate JSON key")
            value[key] = item
        return value
    return json.loads(raw, object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("Non-finite number")))


def write(path, value):
    path = Path(path)
    with path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n")
    path.chmod(0o600)


def operation_specification():
    return {
        "schema": "engarg.operation-specification/1",
        "definition": (
            "catalogue.identifier-audit/1: For the supplied complete boolean and finite items array, "
            "return snapshot unchanged, count equal to the array length, distinct equal to whether every "
            "item's case-sensitive exact Unicode id string differs from every other id, complete unchanged, "
            "and typed_input equal to the supplied formal_input unchanged. "
            "The empty array has count 0 and distinct true. Do not acquire data, trim, case-fold, sort, "
            "filter or substitute identities. Reject input outside the fixed input schema. "
            "Frozen verification fixtures must include empty, singleton, three distinct identifiers, duplicate "
            "identifiers, case-sensitive identifiers a/A, and complete false, with exact independently computed outputs."
        ),
        "input_conditions": ["Inputs satisfy the complete fixed schema; at most 32 items, with nonempty 1–64 character id strings."],
        "output_guarantees": [
            "count is the exact number of supplied items, including duplicates.",
            "distinct is true exactly when the supplied id strings are pairwise distinct using exact string equality.",
            "snapshot and complete are returned without change. No empirical completeness is inferred from distinctness.",
            "typed_input exactly echoes formal_input. It encodes only a formal conjunction and does not establish the truth of either empirical premise.",
            "The acquisition envelope includes observed_at as the actual UTC RFC3339 time of this fresh calculation ending in Z, with the original request and context unchanged. The graph executor retains upstream observation ages separately.",
        ],
        "input_schema": AUDIT_INPUT,
        "output_schema": AUDIT_OUTPUT,
    }


def request_for(inputs):
    """Called by the shell script's bounded input preflight, before inference."""
    if (set(inputs) != {"schema", "snapshot"}
            or inputs.get("schema") != "engarg.catalogue-evidence-example/1"
            or inputs.get("snapshot") != SNAPSHOT):
        raise ValueError("Unsupported catalogue demonstration input")
    context = {
        "example": "Local read-only MCP catalogue demonstration. This is not evidence about a deployed system.",
        "subject_scope": {"kind": "catalogue_snapshot", "subject": "One retained local catalogue snapshot",
                          "selectors": {"snapshot": SNAPSHOT}, "identity_dimensions": ["snapshot"]},
        "observations_supplied": False,
        "available_acquisition": {"method": "catalogue.snapshot/1", "support_type": "source_retrieval",
                                  "input_schema": READ_INPUT, "output_schema": READ_OUTPUT,
                                  "quantities": [{"name": "items", "unit": "json"},
                                                 {"name": "complete", "unit": "boolean"}],
                                  "population": "demonstration-v1 snapshot items",
                                  "coverage": "The named local fixture snapshot is read completely at the returned observation time."},
        "required_custom_operation": operation_specification(),
        "formal_input": FORMAL_INPUT,
        "formal_scope": METHOD_SCOPE,
        "required_workflow": (
            "Retrieve the named snapshot through the configured additional MCP. Pass its exact items and completeness "
            "into the custom catalogue.identifier-audit/1 operation using dependency bindings. Use its computed count "
            "and distinctness in separately evaluated support requirements. Pass the exact formal_input object in "
            "this context as a literal custom operation argument. Its typed_input result requires a separate "
            "MethodInput evaluation targeting the warrant. The formal premises each map to the appropriate "
            "ground and its count/distinctness evidence requirement with correspondence kind accepted_premise. "
            "The formal atoms are licensed by those separately satisfied empirical predicates; they do not copy "
            "a count or boolean measurement into a formula string. Every acquisition declares the snapshot "
            "selector and preserves observation time and provenance. Use source retrieval and complete enumeration "
            "for the snapshot, and calculation/exact_calculation for the audit. Use a typed deductive warrant whose "
            "formal premises correspond to count == 3 and distinct == true, with the conjunction as the conclusion. "
            "Copy formal_scope into the typed warrant conclusion.scope and copy the supplied formal_input.scope "
            "identity exactly into the acquired envelope. That identity is host-derived from the structured scope, "
            "not a scope label invented by the model. All formal-input and empirical premise requirements have "
            "kind catalogue_snapshot, snapshot selector demonstration-v1 and identity dimension snapshot. "
            "Do not invent observations, introduce deployment claims, self-certify arbitrary prose or silently turn "
            "missing evidence into support. Missing model or formal correspondence remains explicitly unresolved. "
            "The generated collector response must include a valid original UTC RFC3339 observed_at timestamp "
            "ending in Z in its EAL "
            "acquisition envelope, as well as unchanged request and context. The graph executor separately retains "
            "the original age of the upstream snapshot. "
            "The supporting argument needs no unrelated alternative explanations or background deployment claims."
        ),
        "freshness": {"max_age_seconds": 3600, "require_original_observation_time": True},
        "authorities": {"snapshot": "catalogue-fixture", "calculation": "catalogue-audit"},
    }
    return {"claim": CLAIM, "context": json.dumps(context, ensure_ascii=False), "evidence": []}


def measurement_semantics(method=METHOD):
    calculation = method == METHOD
    return {
        "qualification_reference": "engarg.local-catalogue-demonstration/1",
        "support_types": ["calculation" if calculation else "source_retrieval"],
        "subject_kinds": ["catalogue_snapshot"], "methods": [method],
        "identity_dimensions": ["snapshot"], "scope_constraints": {"snapshot": SNAPSHOT},
        "scope_bindings": {"snapshot": "/snapshot"},
        "quantities": ([{"name": "count", "unit": "count", "output_pointer": "/count"},
                        {"name": "distinct", "unit": "boolean", "output_pointer": "/distinct"},
                        {"name": "complete", "unit": "boolean", "output_pointer": "/complete"},
                        {"name": "typed_input", "unit": "json", "output_pointer": "/typed_input"}]
                       if calculation else [{"name": "items", "unit": "json", "output_pointer": "/items"},
                                            {"name": "complete", "unit": "boolean", "output_pointer": "/complete"}]),
        "coverage": [{"strength": "exact_calculation" if calculation else "complete_enumeration",
                      "population": None if calculation else "demonstration-v1 snapshot items",
                      "completeness_pointer": "/complete"}],
        "time": {"historical_intervals": False, "historical_input": None,
                 "original_observation_time": True, "uncertainty_seconds": 1},
        "provenance": {"authorities": ["catalogue-audit" if calculation else "catalogue-fixture"],
                       "transformation_history": True},
        "limitations": ["Local demonstration snapshot only. It establishes no private deployment facts."]
    }


def audit_reference(value):
    """Independent finite calculation used on fixtures and actual observations."""
    if (not isinstance(value, dict) or set(value) != set(AUDIT_INPUT["required"])
            or value["formal_input"] != FORMAL_INPUT or type(value["complete"]) is not bool
            or not isinstance(value["snapshot"], str) or not 1 <= len(value["snapshot"]) <= 128
            or not isinstance(value["items"], list) or len(value["items"]) > 32):
        raise ValueError("Audit reference received an invalid bounded input")
    identifiers = []
    for item in value["items"]:
        if (not isinstance(item, dict) or set(item) != {"id"}
                or not isinstance(item["id"], str) or not 1 <= len(item["id"]) <= 64):
            raise ValueError("Audit reference received an invalid item")
        identifiers.append(item["id"])
    return {"collected": True, "snapshot": value["snapshot"], "count": len(identifiers),
            "distinct": len(set(identifiers)) == len(identifiers), "complete": value["complete"],
            "typed_input": value["formal_input"]}


def verify_fixtures(fixtures, require_coverage=True):
    covered = set()
    for fixture in fixtures:
        if fixture.get("kind") == "acquisition_error":
            continue
        inputs = fixture["input"]
        expected = audit_reference(inputs)
        if fixture.get("expected_value") != expected:
            raise ValueError("Frozen audit fixture disagrees with the independent reference")
        identifiers = [item["id"] for item in inputs["items"]]
        if not identifiers:
            covered.add("empty")
        if len(identifiers) == 1:
            covered.add("singleton")
        if len(identifiers) == 3 and expected["distinct"]:
            covered.add("three_distinct")
        if not expected["distinct"]:
            covered.add("duplicates")
        if "a" in identifiers and "A" in identifiers and expected["distinct"]:
            covered.add("case_sensitive")
        if not inputs["complete"]:
            covered.add("incomplete")
    required = {"empty", "singleton", "three_distinct", "duplicates", "case_sensitive", "incomplete"}
    if require_coverage and covered != required:
        raise ValueError("Frozen audit fixtures omit independently required cases: " + ", ".join(sorted(required - covered)))
    return sorted(covered)


def frozen_probe_plan():
    cases = []
    for name, identifiers, complete in [
            ('empty', [], True), ('singleton', ['one'], True),
            ('three-distinct', ['one', 'two', 'three'], True),
            ('duplicates', ['same', 'same'], True), ('case-sensitive', ['a', 'A'], True),
            ('incomplete', ['one', 'two'], False)]:
        inputs = {'snapshot': SNAPSHOT, 'items': [{'id': value} for value in identifiers],
                  'complete': complete, 'formal_input': FORMAL_INPUT}
        cases.append({'name': name, 'kind': 'negative' if name == 'duplicates' else 'positive',
                      'input': inputs, 'expected_value': audit_reference(inputs)})
    return {'schema': 'engarg.catalogue-audit-probes/1', 'method': METHOD, 'cases': cases}


def run_probes(directory, executable, package, plan):
    """Exercise the verified package through its real Docker-backed launcher."""
    directory, executable = Path(directory), Path(executable).resolve()
    profile = read(Path(package['package_manifest']).parent/'profile.json')
    timeout = int(profile['timeout_seconds']) + 60
    results = []
    audit = directory/'tooling/demo-probes.audit.jsonl'
    with audit.open('x', encoding='utf-8') as log:
        for index, case in enumerate(plan['cases'], 1):
            request = {'evidence_id': 'catalogue_probe_'+str(index), 'environment': 'engarg_generated',
                       'tool': package['collector']['id'], 'tool_version': '1', 'input': case['input'],
                       'context': {'probe': case['name'], 'frozen_probe_plan_sha256': digest(plan)}}
            log.write(json.dumps({'event': 'request', 'request': request}, ensure_ascii=False)+'\n')
            log.flush()
            os.fsync(log.fileno())
            print('Catalogue qualification probe %s/%s: %s' % (index, len(plan['cases']), case['name']),
                  file=sys.stderr, flush=True)
            process = subprocess.Popen([str(executable), 'eal', 'tool-run', '--package', package['package_manifest']],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            start = time.monotonic()
            pending_input = canonical(request)
            timed_out = False
            while True:
                try:
                    stdout, stderr = process.communicate(input=pending_input, timeout=min(10, max(0.1, timeout-(time.monotonic()-start))))
                    break
                except subprocess.TimeoutExpired:
                    pending_input = None
                    if time.monotonic()-start >= timeout:
                        timed_out = True
                        process.terminate()
                        try:
                            stdout, stderr = process.communicate(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            stdout, stderr = process.communicate()
                        break
                    print('Catalogue qualification probe %s/%s still running' % (index, len(plan['cases'])),
                          file=sys.stderr, flush=True)
            try:
                envelope = json.loads(stdout)
            except (ValueError, UnicodeError):
                envelope = None
            passed = (not timed_out and process.returncode == 0 and isinstance(envelope, dict)
                      and envelope.get('value') == case['expected_value']
                      and envelope.get('context') == request['context']
                      and envelope.get('request') == {key: request[key] for key in ['tool', 'tool_version', 'input', 'context']}
                      and isinstance(envelope.get('observed_at'), str))
            result = {'name': case['name'], 'passed': passed, 'returncode': process.returncode,
                      'timed_out': timed_out, 'response': envelope,
                      'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
                      'stderr_sha256': hashlib.sha256(stderr).hexdigest()}
            results.append(result)
            log.write(json.dumps({'event': 'response', **result}, ensure_ascii=False)+'\n')
            log.flush()
            os.fsync(log.fileno())
            if not passed:
                break
    receipt = {'schema': 'engarg.catalogue-audit-probe-receipt/1', 'plan_sha256': digest(plan),
               'package_manifest': package['package_manifest'], 'manifest_sha256': package['manifest_sha256'],
               'passed': len(results) == len(plan['cases']) and all(result['passed'] for result in results),
               'results': results}
    write(directory/'tooling/demo-probes.receipt.json', receipt)
    if not receipt['passed']:
        raise ValueError('Independent frozen custom-tool probes failed. The plan, audit and observed receipt are retained.')
    return receipt


def verify_observations(directory):
    directory = Path(directory)
    bundle = read(directory / "argument.bundle.json")
    collection = read(directory / "evidence-collection.json")
    store = Path(bundle["locations"]["store"]).resolve()
    observations = {}
    for record in collection["result"].get("records", {}).values():
        for observation in (record.get("details") or {}).get("observations", []):
            snapshot = (store / observation["snapshot"]).resolve()
            if not snapshot.is_relative_to(store / "observations"):
                raise ValueError("Observation snapshot path escaped its retained store")
            retained = read(snapshot)
            if (retained.get("observation_id") != observation["observation_id"]
                    or retained.get("value_sha256") != digest(retained["value"])
                    or observation["value_sha256"] != retained["value_sha256"]):
                raise ValueError("Observation reference does not match its retained payload")
            observations[retained["observation_id"]] = retained
    sources = [record for record in observations.values()
               if record["source"]["capability_id"] == "catalogue_snapshot"]
    calculations = [record for record in observations.values()
                    if record["source"]["capability_id"] != "catalogue_snapshot"
                    and "formal_input" in record.get("arguments", {})]
    if not sources or not calculations:
        raise ValueError("Both actual MCP snapshot and generated audit observations are required")
    for calculation in calculations:
        inputs = calculation["arguments"]
        if not any(all(inputs[key] == source["value"][key] for key in ["snapshot", "items", "complete"])
                   for source in sources):
            raise ValueError("Generated audit input differs from the acquired MCP snapshot")
        if calculation["value"] != audit_reference(inputs):
            raise ValueError("Generated audit result disagrees with the independently computed acquired snapshot")
    result = {"schema": "engarg.catalogue-reference-check/1", "passed": True,
              "mcp_observation_ids": [record["observation_id"] for record in sources],
              "custom_tool_observation_ids": [record["observation_id"] for record in calculations],
              "scope": "Exact retrieved local demonstration snapshot, finite identifier count and distinctness, unchanged formal input."}
    write(directory / "catalogue-reference-check.json", result)
    return result


def configuration(directory, server, collector="managed"):
    """Prepare operator-owned authority independently of generated argument text."""
    directory, server = Path(directory).resolve(), Path(server).resolve()
    catalogue = directory / "catalogue.json"
    connection = {
        "id": "catalogue_fixture", "authentication_scope": "local-catalogue-reader",
        "transport": {"kind": "stdio", "command": str(server),
                      "arguments": [str(catalogue), str(directory / "mcp-requests.jsonl")],
                      "inherit_env": [], "cwd": str(directory)},
        "tools": [{"tool": "read_catalogue_snapshot", "raw_output_schema_sha256": None,
                   "response_format": {"kind": "structured"}, "value_pointer": "/measurement",
                   "observation_time": {"kind": "pointer", "pointer": "/observed_at"},
                   "scope_pointers": {"snapshot": "/snapshot"}}],
        "timeout_seconds": 30, "output_bytes": 1_048_576,
    }
    capability = {
        "id": "catalogue_snapshot", "version": "1", "description": "Complete read-only local catalogue snapshot",
        "provider": {"id": "catalogue_fixture", "version": "1", "transport": "mcp_stdio",
                     "connection": connection["id"], "tool": "read_catalogue_snapshot",
                     "authentication_scope": connection["authentication_scope"],
                     "configuration_sha256": digest(connection)},
        "input_schema": READ_INPUT, "input_schema_sha256": digest(READ_INPUT),
        "output_schema": READ_OUTPUT, "output_schema_sha256": digest(READ_OUTPUT),
        "semantics": measurement_semantics("catalogue.snapshot/1"),
        "configuration_requirements": [],
        "limits": {"timeout_seconds": 30, "output_bytes": 1_048_576, "concurrency": 1, "requests_per_minute": None},
        "costs": {name: {"value": value, "basis": "unknown" if value is None else "declared",
                         "reference": "Local demonstration. Latency and bytes are measured at execution."}
                  for name, value in {"configuration_effort_seconds": 0, "implementation_effort_seconds": 0,
                                      "execution_requests": 1, "execution_latency_ms": None,
                                      "monetary_microunits": 0, "transfer_bytes": None}.items()},
    }
    write(directory / "connections.json", {
        "schema": "eal.evidence-config/1", "connections": [connection], "native_providers": [],
        "local_catalogues": [{"schema": "eal.capability-catalogue/1", "capabilities": [capability]}],
        "approved_catalogues": [],
        "discovery": {"requests": 64, "bytes": 4_194_304, "candidates": 128,
                      "pages_per_connection": 16, "timeout_seconds": 60},
    })
    write(directory / "planning.json", {
        "schema": "engarg.capability-planning-config/1", "collector": str(collector),
        "connections": str(directory / "connections.json"), "store": str(directory / "observations"),
        "environment": [], "ranking": {"dimensions": ["execution_requests"]},
        "scheduling": {"maximum_concurrent_sources": 2, "per_provider": {}},
        "bindings": [{"capability_id": "catalogue_snapshot", "arguments": {"snapshot": SNAPSHOT},
                      "input_bindings": {"/snapshot": "/snapshot"}, "configured_prerequisites": []}],
    })


def qualify(directory, executable):
    """Limit automatic qualification to the independently specified demo operation."""
    directory = Path(directory)
    analysis = read(directory / "analysis.json")
    specifications = {item["id"]: item.get("operation_spec") for item in
                      analysis["evidence_planning"]["elaboration"]["acquisitions"]}
    packages = analysis.get("tooling", {}).get("capabilities", [])
    if len(packages) != 1:
        raise ValueError("The demonstration requires exactly one verified generated audit operation")
    generated = read(directory / "tooling/generated-native-providers.json")
    providers = generated.get("native_providers", [])
    if len(providers) != 1:
        raise ValueError("The demonstration requires exactly one generated native provider")
    provider = providers[0]
    package = packages[0]
    # The verified package retains the source-operation request independently of
    # the current case. Match its exact logical definition before adding semantics.
    contract_path = Path(package["package_manifest"]).parent / "contract.json"
    contract = read(contract_path)
    operation = json.loads(contract["request_context"])
    verify_fixtures(contract["draft"]["fixtures"], require_coverage=False)
    wanted = operation_specification()
    purpose = json.loads(operation["operation_purpose"])
    if (list(specifications.values()).count(wanted) != 1
            or any(purpose.get(key) != wanted[key] for key in ["schema", "definition", "input_conditions", "output_guarantees"])
            or purpose.get("methods") != [METHOD]):
        raise ValueError("The generated audit operation differs from the independently specified demonstration contract")
    capability = provider["capability"]
    input_schema = capability["input_schema"]
    output_schema = copy.deepcopy(capability["output_schema"])
    output_schema.get("properties", {}).pop("collected", None)
    if "collected" in output_schema.get("required", []):
        output_schema["required"].remove("collected")
    def unannotated(value):
        if isinstance(value, dict):
            return {key: unannotated(item) for key, item in value.items() if not key.startswith("x-engarg-")}
        if isinstance(value, list):
            return list(map(unannotated, value))
        return value
    if unannotated(input_schema) != AUDIT_INPUT or unannotated(output_schema) != AUDIT_OUTPUT:
        raise ValueError("Generated audit interface differs from the bounded demonstration schemas")
    plan_path = directory/'catalogue-probes.snapshot.json'
    identity = read(directory/'input-identity.json')
    if hashlib.sha256(plan_path.read_bytes()).hexdigest() != identity['example_qualification_probes_sha256']:
        raise ValueError('Independent frozen probe plan changed after input preflight')
    plan = read(plan_path)
    if plan != frozen_probe_plan():
        raise ValueError('Unsupported independent catalogue probe plan')
    checked_cases = verify_fixtures(plan['cases'])
    run_probes(directory, executable, package, plan)
    provider["capability"]["semantics"] = measurement_semantics()
    provider["mapping"]["scope_pointers"] = {"snapshot": "/snapshot"}
    destination = directory / "tooling/demo-qualified-providers.json"
    write(destination, generated)
    write(directory / "tooling/demo-qualification.json", {
        "schema": "engarg.catalogue-operation-qualification/1", "method": METHOD,
        "independent_fixture_cases": checked_cases,
        "package_manifest": package["package_manifest"], "manifest_sha256": package["manifest_sha256"],
        "scope": "Fixed local demonstration operation and schema. Frozen fixture results do not establish general scientific model adequacy.",
    })
    return destination


def launch(arguments):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--tool-language", default="rust", choices=["python", "rust", "javascript"])
    parser.add_argument("--toolchain", type=Path)
    graph = parser.add_mutually_exclusive_group()
    graph.add_argument("--topology", choices=['fully-connected', 'fully_connected', 'grouped', 'chain', 'ring', 'star', 'tree', 'layered', 'sparse', 'bipartite'])
    graph.add_argument("--topology-config", type=Path)
    parser.add_argument("--debate-participants", type=int, choices=range(3, 17))
    parser.add_argument("--debate-rounds", type=int, choices=range(1, 9))
    parser.add_argument("--eal-coordinate", type=Path)
    parser.add_argument("--coordination-run", type=Path)
    options = parser.parse_args(arguments)
    if (options.debate_participants is not None or options.debate_rounds is not None) and options.topology is None:
        parser.error('participant and round options require --topology')
    if (options.eal_coordinate is not None or options.coordination_run is not None) and not (options.topology or options.topology_config):
        parser.error('coordinator options require a topology')
    repo = Path(__file__).resolve().parents[1]
    os.umask(0o077)
    base = repo / ".engarg-local"
    base.mkdir(exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="catalogue-evidence-example-", dir=base))
    write(directory / "catalogue.json", {"snapshot": SNAPSHOT, "items": [
        {"id": "alpha"}, {"id": "beta"}, {"id": "gamma"}], "complete": True})
    write(directory / "inputs.json", {"schema": "engarg.catalogue-evidence-example/1", "snapshot": SNAPSHOT})
    write(directory / 'qualification-probes.json', frozen_probe_plan())
    profile = (options.toolchain or repo / ("examples/toolchains/" + options.tool_language + ".json")).resolve()
    if options.check_only:
        server = repo / "target/release/examples/catalogue_mcp"
    else:
        if not os.environ.get("OPENAI_API_KEY"):
            raise ValueError("Export OPENAI_API_KEY before the live catalogue example")
        if subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--'], cwd=repo).returncode:
            raise ValueError('Full runs require a clean tracked checkout. Use --check-only while editing.')
        if not shutil.which("docker"):
            raise ValueError("Docker is required to verify and execute the generated custom tool")
        subprocess.run(["docker", "info"], check=True, stdout=subprocess.DEVNULL)
        selected_profile = read(profile)
        if selected_profile.get('schema') != 'engarg.toolchain/1' or selected_profile.get('language') != options.tool_language:
            raise ValueError('Tool language must match the selected toolchain profile')
        if options.toolchain:
            subprocess.run(['docker', 'image', 'inspect', selected_profile['image']], check=True, stdout=subprocess.DEVNULL)
        elif options.tool_language == 'rust':
            print('Preparing the isolated Rust toolchain image', file=sys.stderr, flush=True)
            subprocess.run(['docker', 'build', '--tag', selected_profile['image'], str(repo/'examples/toolchains/rust')], check=True)
        elif subprocess.run(['docker', 'image', 'inspect', selected_profile['image']],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
            print('Preparing the isolated toolchain image: '+selected_profile['image'], file=sys.stderr, flush=True)
            subprocess.run(['docker', 'pull', selected_profile['image']], check=True)
        subprocess.run(["cargo", "build", "--locked", "--release", "--example", "catalogue_mcp"], cwd=repo, check=True)
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=repo))
        server = Path(metadata["target_directory"]) / "release/examples/catalogue_mcp"
    configuration(directory, server)
    command = ["bash", str(repo / "scripts/run-catalog-sync-authoring.sh"), "--evidence-config", str(directory / "planning.json"),
               "--implement-tools", "--tool-language", options.tool_language, "--toolchain", str(profile)]
    for name in ['topology', 'debate_participants', 'debate_rounds', 'topology_config', 'eal_coordinate', 'coordination_run']:
        value = getattr(options, name)
        if value is not None:
            if isinstance(value, Path):
                value = value.resolve()
            command += ['--' + name.replace('_', '-'), str(value)]
    command += ["--check-only"] if options.check_only else ["--collect-and-assess"]
    command.append(str(directory / "inputs.json"))
    print("Catalogue demonstration inputs: " + str(directory), file=sys.stderr, flush=True)
    return subprocess.run(command, cwd=repo).returncode


def main():
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "qualify":
            print(qualify(sys.argv[2], sys.argv[3]))
            return 0
        return launch(sys.argv[1:])
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print("Catalogue example unresolved: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
