"""Independent local ticket inputs and operator qualification."""
import copy
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("catalogue_example", REPOSITORY / "scripts/catalogue_evidence_example.py")
EXAMPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXAMPLE)


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + "\n")


def contract(calculation):
    semantics = EXAMPLE.measurement_semantics(EXAMPLE.METHOD if calculation else "catalogue.snapshot/1")
    return {
        "schema": "eal.support-contract/1",
        "support_type": "calculation" if calculation else "source_retrieval",
        "scope": {"kind": "catalogue_snapshot", "subject": "catalogue_snapshot", "selectors": {"snapshot": EXAMPLE.SNAPSHOT}, "identity_dimensions": ["snapshot"]},
        "acceptable_methods": [EXAMPLE.METHOD if calculation else "catalogue.snapshot/1"],
        "quantities": [{"name": v["name"], "unit": v["unit"]} for v in semantics["quantities"]],
        "logical_strength": "exact_calculation" if calculation else "complete_enumeration",
        "population": None if calculation else "demonstration-v1 snapshot items",
        "time": {"historical_interval": None, "max_age_seconds": 3600, "max_uncertainty_seconds": 1, "require_original_observation_time": True},
        "provenance": {"required_authorities": ["catalogue-audit" if calculation else "catalogue-fixture"], "transformation_history": True},
        "expected_output_schema_sha256": None,
    }


def argument(root):
    case = json.loads((root / "case.base.json").read_text())
    claim = EXAMPLE.CLAIM
    case["original_claim"] = claim
    case["current_claim"] = {"text": claim, "qualifier": "The identified complete finite local snapshot only."}
    case["stronger_target"] = {"text": "The same finite catalogue audit preserves this cardinality and identifier distinction.", "qualifier": "The same local snapshot and operation contract."}
    case["grounds"][0]["statement"] = "The exact finite catalogue contains three entries."
    case["grounds"].append({"id": "G2", "statement": "The supplied identifiers are pairwise distinct.", "evidence_ids": [], "status": "missing"})
    case["scope"].update(system_boundary="catalogue_snapshot", configuration="Local finite catalogue demonstration-v1.", time_horizon="The recorded observation remains within its one-hour freshness budget.")
    warrant = case["warrants"][0]
    warrant["premise_ids"] = ["G1", "G2"]
    binding = warrant["reasoning_method"]
    binding["method"] = {"kind": "deductive", **EXAMPLE.FORMAL_INPUT["payload"]}
    binding["formal_premises"] = [
        {"premise_id": "G1", "input_pointer": "/premises/0", "evidence_need_id": "N7", "correspondence": {"kind": "accepted_premise"}, "interpretation": "The separately evaluated exact count equals three."},
        {"premise_id": "G2", "input_pointer": "/premises/1", "evidence_need_id": "N8", "correspondence": {"kind": "accepted_premise"}, "interpretation": "The finite identifier audit reports pairwise distinctness."},
    ]
    binding["evidence_requirements"].append({"evidence_need_id": "N8", "role": "premise_observation", "requirement": "Evaluate the exact finite identifier audit."})
    binding["conclusion"].update(subject="catalogue_snapshot", scope=EXAMPLE.METHOD_SCOPE, valid_from=EXAMPLE.FORMAL_INPUT["valid_from"], valid_until=EXAMPLE.FORMAL_INPUT["valid_until"], result={"path": "entailed", "operator": "==", "expected": True})
    need = copy.deepcopy(case["evidence_plan"][-1])
    need.update(id="N8", target_id="G2", question="Are all supplied catalogue identifiers distinct?")
    case["evidence_plan"].append(need)
    case["reading"][0]["paragraphs"][0].update(text=claim, reference_ids=["current_claim", "G1", "G2", "W1", "A1", "N5", "N6", "N7", "N8"])
    write(root / "request.json", {"claim": claim, "context": "Native ticket acceptance over the bounded local catalogue fixture.", "evidence": []})
    write(root / "case.json", case)
    needs = {target: sorted([n for n in case["evidence_plan"] if n["target_id"] == target], key=lambda n:n["id"]) for target in ["W1", "G1", "G2", "A1", "current_claim", "stronger_target"]}
    criteria = {
        "W1": ({"kind": "method_input", "method": "deductive/1"}, "audit", "/typed_input", True, "unspecified"),
        "G1": ({"kind": "equals", "expected": 3}, "audit", "/count", True, "unspecified"),
        "G2": ({"kind": "equals", "expected": True}, "audit", "/distinct", True, "unspecified"),
        "A1": ({"kind": "equals", "expected": True}, "snapshot", "/complete", False, "complete_enumeration"),
        "current_claim": ({"kind": "equals", "expected": True}, "snapshot", "/complete", False, "complete_enumeration"),
        "stronger_target": ({"kind": "equals", "expected": True}, "snapshot", "/complete", False, "complete_enumeration"),
    }
    requirements, evaluations = [], []
    for target, (criterion, step, pointer, calculation, coverage) in criteria.items():
        identifier = "requirement_" + target
        selected_contract = contract(calculation)
        selected_contract["quantities"] = [q for q in selected_contract["quantities"] if q["name"] == pointer[1:]]
        requirements.append({"id": identifier, "target_id": target, "evidence_need_ids": [n["id"] for n in needs[target]], "original_acceptance_criteria": [n["acceptance_criterion"] for n in needs[target]], "contract": selected_contract, "discriminating_evidence": [], "acceptance": criterion, "acceptance_rationale": "Apply this explicit finite predicate to the identified source observation and retain the result.", "gaps": []})
        evaluations.append({"id": "evaluation_"+identifier, "requirement_id": identifier, "inputs": [{"step_id": step, "pointer": pointer}], "criterion": criterion, "max_age_seconds": 3600, "required_coverage": coverage})
    elaboration = {"schema": "engarg.support-elaboration/2", "requirements": requirements, "acquisitions": [
        {"id": "snapshot", "contract": contract(False), "arguments": {"snapshot": EXAMPLE.SNAPSHOT}, "bindings": [], "for_each": None},
        {"id": "audit", "contract": contract(True), "arguments": {"snapshot": EXAMPLE.SNAPSHOT, "items": None, "complete": None, "formal_input": EXAMPLE.FORMAL_INPUT}, "bindings": [{"output": {"step_id": "snapshot", "pointer": "/items"}, "argument_pointer": "/items"}, {"output": {"step_id": "snapshot", "pointer": "/complete"}, "argument_pointer": "/complete"}], "for_each": None, "operation_spec": EXAMPLE.operation_specification()},
    ], "derivations": [], "evaluations": evaluations, "revisions": [], "acquisition_revisions": [], "alternative_support": []}
    write(root / "elaboration.json", elaboration)


def prepare(root, collector, server, native):
    root = Path(root)
    argument(root)
    write(root / "catalogue.json", {"snapshot": EXAMPLE.SNAPSHOT, "items": [{"id":"alpha"}, {"id":"beta"}, {"id":"gamma"}], "complete": True})
    EXAMPLE.configuration(root, Path(server), Path(collector))
    configured = json.loads((root / "connections.json").read_text())
    registrations = copy.deepcopy(configured)
    registrations.pop("native_providers", None)
    registrations.pop("discovery", None)
    write(root / "mcp-registrations.json", registrations)
    configured["connections"] = []
    if native:
        output = copy.deepcopy(EXAMPLE.AUDIT_OUTPUT)
        output["properties"]["collected"] = {"type":"boolean"}
        output["required"].append("collected")
        descriptor = copy.deepcopy(configured["local_catalogues"][0]["capabilities"][0])
        descriptor.update(id="ticket_native_audit", description="Finite independently compiled catalogue audit", input_schema=EXAMPLE.AUDIT_INPUT, input_schema_sha256=EXAMPLE.digest(EXAMPLE.AUDIT_INPUT), output_schema=output, output_schema_sha256=EXAMPLE.digest(output), semantics=EXAMPLE.measurement_semantics())
        descriptor["provider"] = {"id": "ticket_native_audit", "version":"1", "transport":"command", "connection":None, "tool":None, "authentication_scope":"local-ticket-fixture", "configuration_sha256":"0"*64}
        provider = {"capability":descriptor,"transport":{"kind":"command","program":str(Path(native).resolve()),"supervisor":str(Path(collector).with_name("eal-process-supervisor")),"arguments":[],"cwd":str(root),"inherit_env":[],"protocol":{"kind":"eal_acquisition","environment":"ticket_fixture","tool":"ticket_native_audit","tool_version":"1","context":{}}},"mapping":{"tool":"ticket_native_audit","raw_output_schema_sha256":None,"response_format":{"kind":"structured"},"value_pointer":"/value","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"snapshot":"/snapshot"}},"pinned_files":[],"credential_env":[]}
        write(root / "native.input.json", provider)
        result = subprocess.run([collector,"prepare-native-provider","--input",str(root/"native.input.json")],capture_output=True,check=True)
        configured["native_providers"] = [json.loads(result.stdout)]
    write(root / "connections.json", configured)
    write(root / "planning.json", {"schema":"engarg.capability-planning-config/1","collector":collector,"connections":str(root/"connections.json"),"store":str(root/"observations"),"environment":[],"ranking":{"dimensions":["execution_requests"]},"scheduling":{"maximum_concurrent_sources":2,"per_provider":{}},"bindings":[]})
    write(root / "review-binding.json", {
        "kind":"command", "version":"1",
        "argv":[sys.executable, str(Path(__file__).resolve()), "review", str(root)],
        "timeout_seconds":30, "max_output_bytes":1048576,
        "inherit_env":[], "parallel_safe":False,
    })


def review(root):
    """Decide only this finite fixture's interpretation and applicability obligations.

    This operator policy is independent of the model response. Ground reviews read
    the source catalogue again; method reviews check the predeclared formal query.
    The normal evidence predicates and native reasoning still execute separately.
    """
    root = Path(root)
    request = json.load(sys.stdin)
    expected_fields = {"evidence_id", "environment", "tool", "tool_version", "input", "context"}
    if set(request) != expected_fields or request["tool"] != "engarg_review_contract" or request["tool_version"] != "1" or request["environment"] != "engarg_case":
        raise ValueError("Unexpected review request identity")
    data = request["input"]
    digest = data.get("case_digest", "")
    if data.get("schema") != "engarg.eal-export/3" or not re.fullmatch("[0-9a-f]{64}", digest) or request["context"].get("engarg_export_digest") != digest:
        raise ValueError("Review is not bound to its exact exported case")
    case = json.loads((root / "case.json").read_text())
    catalogue = json.loads((root / "catalogue.json").read_text())
    obligation = data["obligation"]
    identifier = obligation["target_id"]
    candidates = {
        "G1": ("ground_interpretation", "ground_review_4731", case["grounds"][0]),
        "G2": ("ground_interpretation", "ground_review_4732", case["grounds"][1]),
        "W1": ("warrant_adequacy", "warrant_review_5731", case["warrants"][0]),
        "A1": ("assumption_validation", "assumption_review_4131", case["assumptions"][0]),
        "current_claim": ("composition_adequacy", "composition_review_63757272656e745f636c61696d", case["compositions"][0]),
    }
    selected = candidates.get(identifier)
    accepted = False
    if selected:
        kind, evidence, detail = selected
        accepted = (obligation["kind"] == kind and obligation["id"] == f"{kind}:{identifier}"
                    and obligation["eal_evidence_name"] == evidence and request["evidence_id"] == evidence
                    and obligation["source_evidence_ids"] == [] and data["authored_detail"] == detail)
        source_applies = catalogue["snapshot"] == EXAMPLE.SNAPSHOT and catalogue["complete"] is True
        formal = case["warrants"][0]["reasoning_method"]
        model_applies = (formal["method"] == {"kind":"deductive", **EXAMPLE.FORMAL_INPUT["payload"]}
                         and formal["conclusion"]["scope"] == EXAMPLE.METHOD_SCOPE
                         and formal["conclusion"]["subject"] == "catalogue_snapshot"
                         and formal["conclusion"]["result"] == {"path":"entailed", "operator":"==", "expected":True}
                         and [(p["premise_id"], p["input_pointer"], p["correspondence"]) for p in formal["formal_premises"]]
                         == [("G1", "/premises/0", {"kind":"accepted_premise"}), ("G2", "/premises/1", {"kind":"accepted_premise"})])
        accepted = accepted and source_applies
        if identifier == "G1":
            accepted = accepted and len(catalogue["items"]) == 3 and detail["statement"] == "The exact finite catalogue contains three entries."
        elif identifier == "G2":
            ids = [item["id"] for item in catalogue["items"]]
            accepted = accepted and len(set(ids)) == len(ids) and detail["statement"] == "The supplied identifiers are pairwise distinct."
        elif identifier == "W1":
            accepted = accepted and model_applies and detail["premise_ids"] == ["G1", "G2"] and detail["conclusion_id"] == "current_claim"
        elif identifier == "A1":
            accepted = accepted and model_applies and detail["applies_to"] == ["W1"]
        else:
            accepted = accepted and model_applies and detail["relationship"] == "jointly_required" and detail["selected_warrant_ids"] == ["W1"] and case["current_claim"]["text"] == EXAMPLE.CLAIM
    value = {"accepted":bool(accepted), "policy":"finite-catalogue-interpretation/1", "obligation_id":obligation["id"]}
    with (root / "review-decisions.jsonl").open("a") as audit:
        audit.write(json.dumps(value) + "\n")
    print(json.dumps({"value":value, "observed_at":datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                      "context":request["context"], "request":{key:request[key] for key in ["tool", "tool_version", "input", "context"]},
                      "details":{"reviewer":"independent finite fixture operator policy", "source":str(root / "catalogue.json")}}))


def qualify():
    request = json.load(sys.stdin)
    providers = request["providers"]
    if isinstance(providers, dict):
        providers = providers["native_providers"]
    for provider in providers:
        provider["capability"]["semantics"] = EXAMPLE.measurement_semantics()
        provider["mapping"]["scope_pointers"] = {"snapshot":"/snapshot"}
    print(json.dumps({"schema":"eal.evidence-config/1", "connections":[], "local_catalogues":[], "approved_catalogues":[], "native_providers":providers}))


if __name__ == "__main__":
    if sys.argv[1] == "qualify":
        qualify()
    elif sys.argv[1] == "review":
        review(sys.argv[2])
    else:
        prepare(*sys.argv[1:])
