//! Proposed collector choices bound to evidence needs without changing the case schema.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, domain::ArgumentCase};

/// A proposed named input for a trusted collector, never an executable or shell command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolArgument {
    pub name: String,
    pub value: String,
}

/// One unperformed acquisition proposal for one exact canonical evidence need.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionPlan {
    pub need_id: String,
    pub tool_id: String,
    pub arguments: Vec<ToolArgument>,
    pub unresolved_arguments: Vec<String>,
}

/// Internal provider response whose argument is retained in the unchanged public report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannedArgument {
    pub argument: ArgumentCase,
    pub acquisition_plan: Vec<AcquisitionPlan>,
}

#[derive(Serialize)]
struct ToolSpec {
    id: &'static str,
    purpose: &'static str,
    arguments: &'static [&'static str],
}

const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "artifact_json",
        purpose: "Read explicitly supplied, digest-checked retained JSON artifacts for the exact evidence need. Missing artifacts fail collection; existence does not establish acceptance.",
        arguments: &["inputs"],
    },
    ToolSpec {
        id: "scoped_json",
        purpose: "Read Kubernetes Application JSON or exact Grafana Tempo search/full traces using an operator-owned inputs file. Unresolved selectors fail before I/O; scope and interpretation require independent review.",
        arguments: &["inputs"],
    },
    ToolSpec {
        id: "kubectl_get",
        purpose: "Read Kubernetes resource JSON",
        arguments: &["context", "namespace", "resource", "name"],
    },
    ToolSpec {
        id: "gh_api_get",
        purpose: "Read a GitHub REST API JSON endpoint",
        arguments: &["endpoint"],
    },
    ToolSpec {
        id: "http_json_get",
        purpose: "Read an HTTP JSON endpoint",
        arguments: &["url"],
    },
    ToolSpec {
        id: "custom",
        purpose: "A custom collection or independent review adapter",
        arguments: &["adapter"],
    },
];

/// Required named inputs for the fixed collector catalogue.
///
/// The executable and transport remain operator configuration. A catalogue ID
/// describes a prospective adapter rather than granting execution authority.
pub fn required_arguments(tool_id: &str) -> Option<&'static [&'static str]> {
    if let Some(tool) = crate::native_collectors::spec(tool_id) {
        return Some(tool.parameters);
    }
    TOOLS
        .iter()
        .find(|tool| tool.id == tool_id)
        .map(|tool| tool.arguments)
}

/// Compact planning catalogue. It deliberately contains no executable paths or credentials.
pub fn catalogue() -> Value {
    let mut tools = serde_json::to_value(TOOLS).expect("tool catalogue serialises");
    tools.as_array_mut().expect("catalogue array").extend(
        crate::native_collectors::TOOLS
            .iter()
            .map(|tool| json!({"id":tool.id,"purpose":tool.purpose,"arguments":tool.parameters})),
    );
    tools
}

pub fn planned_case_schema() -> Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(PlannedArgument)).expect("schema serialises");
    schema.as_object_mut().expect("object").remove("$schema");
    schema["$defs"]["AcquisitionPlan"]["properties"]["tool_id"]["enum"] = json!(
        catalogue()
            .as_array()
            .expect("catalogue array")
            .iter()
            .map(|tool| tool["id"].clone())
            .collect::<Vec<_>>()
    );
    schema
}

/// Decode new provider envelopes and historical direct cases without changing either format.
pub fn parse_completion(content: &str) -> Result<PlannedArgument, Error> {
    match serde_json::from_str::<PlannedArgument>(content) {
        Ok(planned) => Ok(planned),
        Err(envelope_error) => serde_json::from_str::<ArgumentCase>(content)
            .map(|argument| PlannedArgument {
                argument,
                acquisition_plan: Vec::new(),
            })
            .map_err(|_| {
                Error::InvalidOutput(format!("invalid acquisition response: {envelope_error}"))
            }),
    }
}

/// Validate selection and exact need binding, without asserting that proposed inputs are true.
pub fn validate(case: &ArgumentCase, plans: &[AcquisitionPlan]) -> Vec<String> {
    let expected: BTreeSet<_> = case
        .evidence_plan
        .iter()
        .map(|need| need.id.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    for plan in plans {
        if !seen.insert(plan.need_id.as_str()) {
            errors.push(format!("duplicate acquisition plan for {}", plan.need_id));
        }
        if !expected.contains(plan.need_id.as_str()) {
            errors.push(format!(
                "acquisition plan names unknown evidence need {}",
                plan.need_id
            ));
        }
        let Some(required) = required_arguments(&plan.tool_id) else {
            errors.push(format!(
                "acquisition plan {} selects unknown tool {}",
                plan.need_id, plan.tool_id
            ));
            continue;
        };
        if let Some(tool) = crate::native_collectors::spec(&plan.tool_id)
            && let Err(error) = tool.input(plan)
        {
            errors.push(format!(
                "acquisition plan {} has invalid native input {error}",
                plan.need_id
            ));
        }
        let mut inputs = BTreeSet::new();
        for argument in &plan.arguments {
            if !required.contains(&argument.name.as_str()) {
                errors.push(format!(
                    "acquisition plan {} has unknown argument {}",
                    plan.need_id, argument.name
                ));
            }
            if !inputs.insert(argument.name.as_str()) {
                errors.push(format!(
                    "acquisition plan {} repeats argument {}",
                    plan.need_id, argument.name
                ));
            }
            if argument.value.trim().is_empty() {
                errors.push(format!(
                    "acquisition plan {} has empty argument {}",
                    plan.need_id, argument.name
                ));
            }
        }
        for name in &plan.unresolved_arguments {
            if !required.contains(&name.as_str()) {
                errors.push(format!(
                    "acquisition plan {} has unknown unresolved argument {name}",
                    plan.need_id
                ));
            }
            if !inputs.insert(name.as_str()) {
                errors.push(format!("acquisition plan {} repeats or both supplies and leaves unresolved argument {name}", plan.need_id));
            }
        }
        for name in required {
            if !inputs.contains(name) {
                errors.push(format!(
                    "acquisition plan {} must supply or leave unresolved argument {name}",
                    plan.need_id
                ));
            }
        }
        if plan.tool_id == "custom"
            && (!plan.arguments.is_empty() || plan.unresolved_arguments != ["adapter"])
        {
            errors.push(format!(
                "custom acquisition plan {} must leave its adapter unresolved",
                plan.need_id
            ));
        }
    }
    for id in expected.difference(&seen) {
        errors.push(format!("evidence need {id} requires an acquisition plan"));
    }
    errors
}

/// Bind operator overrides to exact needs while retaining untouched proposed investigations.
pub fn override_plans(
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    overrides: Vec<AcquisitionPlan>,
) -> Result<Vec<AcquisitionPlan>, Error> {
    if !retained.is_empty() {
        let errors = validate(case, retained);
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
    }
    let mut merged: std::collections::BTreeMap<_, _> = case
        .evidence_plan
        .iter()
        .map(|need| {
            (
                need.id.clone(),
                AcquisitionPlan {
                    need_id: need.id.clone(),
                    tool_id: "custom".into(),
                    arguments: vec![],
                    unresolved_arguments: vec!["adapter".into()],
                },
            )
        })
        .collect();
    for plan in retained {
        merged.insert(plan.need_id.clone(), plan.clone());
    }
    let mut seen = BTreeSet::new();
    for plan in overrides {
        if !merged.contains_key(&plan.need_id) || !seen.insert(plan.need_id.clone()) {
            return Err(Error::InvalidInput(
                "acquisition override repeats a need or names an unknown need".into(),
            ));
        }
        merged.insert(plan.need_id.clone(), plan);
    }
    let plans: Vec<_> = merged.into_values().collect();
    let errors = validate(case, &plans);
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    Ok(plans)
}

pub(crate) const PLANNING_INSTRUCTIONS: &str = "ACQUISITION PLANNING: acquisition_tools lists proposed trusted collector IDs and named string inputs. Use native kubernetes for fleet resource reads, github for exact commit identity and tempo for bounded cluster-scoped rollout searches when those measurements fit. Prefer known collectors even when their configuration is unknown and retain unresolved arguments. Split executable acquisitions from independent interpretation and controlled experiment design. Use gh_api_get for other GitHub GET endpoints and http_json_get for exact literal queries or trace-by-ID reads. Use custom only for an investigation the listed collectors cannot perform. Identify the required tools and inputs while constructing or auditing the evidence methods. Values must come from supplied claim, context or evidence. Leave unknown hosts, cluster contexts, resource identities, dates and other inputs unresolved rather than guessing. kubectl_get allows * for namespace or name only when that all-resources scope is established. A proposed collector or input is an unperformed plan and supplies no evidence. Collecting raw output does not establish an acceptance criterion or answer an inferential review.";

pub(crate) const ENVELOPE_INSTRUCTIONS: &str = "PLANNED RESPONSE: Return the supplied envelope with argument holding the complete unchanged ArgumentCase and acquisition_plan holding exactly one record for every argument.evidence_plan ID. Each record names need_id and one acquisition_tools ID. Put each required named input either in arguments as {name,value} string entries or in unresolved_arguments, never both. Use only the catalogue's input names. For custom, arguments is empty and unresolved_arguments is [\"adapter\"]. Revise the argument and its acquisition bindings together. The plan is separate from the canonical case and must preserve its target, enabled claim, question, measurement, comparator and acceptance criterion.";

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> ArgumentCase {
        serde_json::from_str(include_str!("../examples/claim-only-case.json")).unwrap()
    }

    fn custom_plans(case: &ArgumentCase) -> Vec<AcquisitionPlan> {
        case.evidence_plan
            .iter()
            .map(|need| AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: "custom".into(),
                arguments: vec![],
                unresolved_arguments: vec!["adapter".into()],
            })
            .collect()
    }

    #[test]
    fn every_need_has_one_checked_binding_and_custom_stays_unresolved() {
        let case = case();
        let mut plans = custom_plans(&case);
        assert!(validate(&case, &plans).is_empty());
        plans[0].arguments.push(ToolArgument {
            name: "adapter".into(),
            value: "invented executable".into(),
        });
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("both supplies"))
        );
        plans.pop();
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("requires an acquisition plan"))
        );
    }

    #[test]
    fn rejects_unknown_tools_arguments_and_duplicate_needs() {
        let case = case();
        let mut plans = custom_plans(&case);
        plans[0].tool_id = "execute_shell".into();
        plans.push(plans[0].clone());
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("unknown tool"))
        );
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("duplicate acquisition"))
        );
        plans[0].tool_id = "http_json_get".into();
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("unknown unresolved argument"))
        );
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|e| e.contains("argument url"))
        );
    }

    #[test]
    fn historical_cases_and_provider_envelopes_preserve_the_canonical_case() {
        let case = case();
        let legacy = serde_json::to_string(&case).unwrap();
        let parsed = parse_completion(&legacy).unwrap();
        assert!(parsed.acquisition_plan.is_empty());
        let planned = PlannedArgument {
            acquisition_plan: custom_plans(&case),
            argument: case,
        };
        let parsed = parse_completion(&serde_json::to_string(&planned).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed.argument).unwrap(),
            serde_json::from_str::<Value>(&legacy).unwrap()
        );
        assert_eq!(
            planned_case_schema()["properties"]["argument"]["$ref"],
            "#/$defs/ArgumentCase"
        );
    }
}
