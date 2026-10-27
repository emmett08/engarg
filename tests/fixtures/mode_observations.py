"""Acquire identified synthetic records; formal queries remain authored EAL data."""
import json
import sys

request = json.load(sys.stdin)
value = json.loads(__PAYLOAD__)
scenario = request["input"]["scenario"]
operation = request["input"]["operation"]
if scenario == "unavailable" and operation == "premise_source":
    raise RuntimeError("The identified source snapshot is unavailable")
if scenario == "challenge-unavailable" and operation == "challenge_source":
    raise RuntimeError("The applicability challenge source is unavailable")
if scenario == "inapplicable":
    value["design"] = {"identity": "a different sampling or model design"}
if scenario == "wrong-scope":
    value["review"]["scope"]["subject"] = "different-fixture"
if scenario in ("rejected", "disanalogy"):
    value["review"]["verdict"] = "rejected"
if scenario == "disanalogy":
    value["transfer"]["differences"][0]["relevance"] = "target operation crosses a measured network boundary absent from the source case"
    value["review"]["reasons"] = ["The region difference adds target network latency absent from the source observation; the source duration bound cannot transfer under this changed mechanism."]
if scenario == "inconclusive":
    value["review"]["verdict"] = "inconclusive"
if scenario == "missing-review-time":
    value["review"].pop("reviewed_at")
if scenario == "missing-reviewer":
    value["review"].pop("reviewer")
if scenario == "old-nested-review":
    value["review"]["reviewed_at"] = "2020-01-01T00:00:00Z"
if scenario == "malformed-review-reasons":
    value["review"]["reasons"] = [None]
if scenario == "blank-reviewer":
    value["review"]["reviewer"] = "  "
if operation == "review_source":
    observed = {key: item for key, item in value.items() if key != "review"}
    if request["input"]["observed"] != observed:
        raise RuntimeError("The retained review does not identify the acquired observation basis")
    value = {"review": value["review"]}
elif operation == "challenge_source":
    value = {"active": scenario != "resolved-challenge", "resolved": scenario == "resolved-challenge"}
else:
    value.pop("review", None)
observed_at = "2020-01-01T00:00:00Z" if scenario == "stale" else "2026-10-25T12:00:00Z"
print(json.dumps({"value": value, "observed_at": observed_at, "context": request["context"],
                  "request": {key: request[key] for key in ["tool", "tool_version", "input", "context"]}}))
