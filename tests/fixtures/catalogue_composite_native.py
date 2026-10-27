"""Exercise the composite proof through real local MCP, EAL host and audit.

No model or private credentials are used. The independent review intentionally
remains unavailable and the generated EAL claim remains unsupported.
"""
import datetime
import json
from pathlib import Path
import sys

import catalogue_workflow_native as native

example = native.example
original_configuration = example.configuration


def configuration(directory, server, collector):
    original_configuration(directory, server, collector)
    path = directory / "connections.json"
    config = example.read(path)
    descriptor = config["local_catalogues"][0]["capabilities"][0]
    guarantee = descriptor["semantics"]["coverage"][0]
    assert guarantee["strength"] == "complete_enumeration"
    guarantee["population_quantity"] = "items"
    native.write(path, config)


example.configuration = configuration


def execute(directory, collector, host, server, scenario):
    source, context = native.prepare(directory, collector, server, scenario)
    plan_path = directory / "plan.json"
    plan = example.read(plan_path)
    old_digest = example.digest(plan)
    snapshot, audit = plan["acquisitions"]
    audit["arguments"]["snapshot"] = None
    audit["bindings"].append({
        "output": {"step_id": "snapshot", "pointer": "/snapshot"},
        "argument_pointer": "/snapshot",
    })
    audit["coverage_proof"] = {
        "kind": "identifier_audit_v1",
        "source_step_id": "snapshot",
        "source_acquisition_id": "snapshot",
        "source_qualified_contract_sha256": snapshot["source"]["qualified_contract_sha256"],
        "population": "demonstration-v1 snapshot items",
        "population_quantity": "items",
        "source_raw_items_pointer": "/items",
        "source_raw_complete_pointer": "/complete",
        "source_items_pointer": "/items",
        "source_complete_pointer": "/complete",
        "source_identity_pointer": "/snapshot",
        "argument_items_pointer": "/items",
        "argument_complete_pointer": "/complete",
        "argument_identity_pointer": "/snapshot",
        "result_count_pointer": "/count",
        "result_distinct_pointer": "/distinct",
        "result_complete_pointer": "/complete",
        "result_identity_pointer": "/snapshot",
        "item_id_pointer": "/id",
        "identity_scope_key": "snapshot",
        "echoes": {"/formal_input": "/typed_input"},
    }
    for evaluation in plan["evaluations"]:
        if evaluation["requirement_id"] in ("count", "distinct"):
            evaluation["required_coverage"] = "complete_enumeration"
            evaluation["applicability"] = [
                {"step_id": "snapshot", "pointer": "/complete"},
                {"step_id": "audit", "pointer": "/complete"},
            ]
    native.write(plan_path, plan)
    digest = example.digest(plan)
    source = source.replace(old_digest, digest)
    context["engarg_execution_plan_sha256"] = digest
    review = (
        'tool independent_review {\n  version "1"\n}\n\n'
        'evidence pending_review {\n  tool independent_review\n  kind review\n'
        '  environment catalogue_demo\n  max_age 3600\n  input {}\n'
        '  require accepted == true\n}\n\n'
    )
    source = source.replace('argument catalogue_argument = ', review + 'argument catalogue_argument = ')
    source = source.replace('evidence count_evidence, distinct_evidence, formal_evidence]',
                            'evidence count_evidence, distinct_evidence, formal_evidence, pending_review]')
    (directory / "argument.eal").write_text(source, encoding="utf-8")
    if scenario == "incomplete":
        catalogue = example.read(directory / "catalogue.json")
        catalogue["complete"] = False
        native.write(directory / "catalogue.json", catalogue)
    command = [str(host), "--workspace", str(directory), "--registry", str(directory / "tools.toml"), "--timeout", "75"]

    def operation(request):
        envelope = native.run(command, request)
        assert envelope["is_error"] is False, envelope
        return envelope["result"]

    validation = operation({"operation": "validate", "source": source})
    assert validation["valid"] is True, validation
    collection = operation({"operation": "collect", "source": source, "context": context})
    native.write(directory / "composite-collection.json", collection)
    assessment = operation({"operation": "reason", "source": source, "context": context,
                            "collection_id": collection["collection_id"],
                            "now": datetime.datetime.now(datetime.timezone.utc).isoformat()})
    native.write(directory / "composite-assessment.json", assessment)
    assert assessment["valid"] is True, assessment
    assert assessment["claims"]["catalogue_supported"]["status"] == "unsupported", assessment
    assert assessment["evidence"]["pending_review"]["status"] == "unavailable", assessment
    mcp_path = directory / "mcp-requests.jsonl"
    audit_path = directory / "audit-requests.jsonl"
    if scenario == "positive":
        assert mcp_path.exists(), "the source MCP did not retain an observation"
        mcp_calls = [json.loads(line) for line in mcp_path.read_text().splitlines()]
        assert len(mcp_calls) == 1, mcp_calls
        audit_calls = [json.loads(line) for line in audit_path.read_text().splitlines()]
        assert len(audit_calls) == 1, audit_calls
        measured_source = mcp_calls[0]["response"]["measurement"]
        measured_audit = audit_calls[0]["measurement"]
        assert audit_calls[0]["request"]["input"]["items"] == measured_source["items"]
        assert measured_source["complete"] is True
        assert measured_audit["count"] == 3 and measured_audit["distinct"] is True
        for name in ("count_evidence", "distinct_evidence"):
            assert collection["records"][name]["status"] == "ok", collection
            assert collection["records"][name]["value"]["criterion_satisfied"] is True, collection
            evaluation = collection["records"][name]["value"]["evaluation"]
            assert len(evaluation["observation_ids"]) == 2, evaluation
            assert evaluation["observed_at"] == mcp_calls[0]["response"]["observed_at"], evaluation
    else:
        # The example MCP rejects incomplete input before recording a successful
        # tool response. Separate runtime tests exercise a provider that emits
        # an explicit complete=false observation.
        assert not mcp_path.exists(), "the incomplete snapshot was accepted"
        assert not audit_path.exists(), "an incomplete population reached the audit"
        assert collection["records"]["count_evidence"]["status"] != "ok", collection
        mcp_calls = []
    return {"claim_status": assessment["claims"]["catalogue_supported"]["status"],
            "review_status": assessment["evidence"]["pending_review"]["status"],
            "source_calls": len(mcp_calls)}


if __name__ == "__main__":
    root, collector, host, server = map(Path, sys.argv[1:])
    print(json.dumps({scenario: execute(root / scenario, collector, host, server, scenario)
                      for scenario in ("positive", "incomplete")}))
