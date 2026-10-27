"""Declared finite decision policy over observations supplied by an EAL dependency."""
import json
import sys

request = json.load(sys.stdin)
data = request["input"]["data"]
budget = request["input"]["work_budget"]
visited = 0
eligible = []
calculation = {"status": "incomplete"}
for option in data["options"]:
    if visited == budget:
        break
    visited += 1
    if option["available"] and option["cost_gbp"] <= data["constraints"]["maximum_cost_gbp"]:
        eligible.append(option)
value = {"calculation": calculation,
         "work": {"unit": "option_visit", "allowance": budget,
                  "performed": visited, "required": len(data["options"])},
         "objective": "minimum duration among available options within cost cap; lexical id breaks ties"}
if visited == len(data["options"]):
    eligible.sort(key=lambda option: (option["duration_s"], option["id"]))
    calculation.update(status="complete", selected=eligible[0]["id"] if eligible else None)
    value["feasible_options"] = [option["id"] for option in eligible]
print(json.dumps({"value": value, "observed_at": "2026-10-25T12:00:00Z", "context": request["context"],
                  "request": {key: request[key] for key in ["tool", "tool_version", "input", "context"]}}))
