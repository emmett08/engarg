//! Proposed collector choices bound to evidence needs without changing the case schema.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, collector_catalogue::CollectorCatalogue, domain::ArgumentCase};

/// A proposed named input for a trusted collector, never an executable or shell command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolArgument {
    pub name: String,
    pub value: String,
}

/// The exact source operation an operator can configure for a scoped reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopedOperation {
    Kubernetes,
    GrafanaSearch,
    GrafanaTrace,
}

/// Preparation instructions select supplied configuration, never observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputPreparation {
    Scoped { operation: ScopedOperation },
    Artifact { reference: String },
}

/// One unperformed acquisition proposal for one exact canonical evidence need.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionPlan {
    pub need_id: String,
    pub tool_id: String,
    pub arguments: Vec<ToolArgument>,
    pub unresolved_arguments: Vec<String>,
    /// Historical plans omit this field and preserve their exact serialisation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_preparation: Option<InputPreparation>,
}

/// Internal provider response whose argument is retained in the unchanged public report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannedArgument {
    pub argument: ArgumentCase,
    pub acquisition_plan: Vec<AcquisitionPlan>,
}

const CUSTOM_ARGUMENTS: &[&str] = &["adapter"];

/// Required named inputs for the fixed collector catalogue.
///
/// The executable and transport remain operator configuration. A catalogue ID
/// describes a prospective adapter rather than granting execution authority.
pub fn required_arguments(tool_id: &str) -> Option<&'static [&'static str]> {
    if tool_id == "custom" {
        Some(CUSTOM_ARGUMENTS)
    } else {
        eal_adapters::profiles::get(tool_id).map(|profile| profile.arguments)
    }
}

/// Compact planning catalogue. It deliberately contains no executable paths or credentials.
pub fn catalogue() -> Value {
    catalogue_with(None)
}

pub fn catalogue_with(selected: Option<&CollectorCatalogue>) -> Value {
    // Planning exposes every implemented compatibility adapter. The EAL owner
    // supplies its contract; selection grants neither execution nor credentials.
    let mut tools: Vec<Value> = eal_adapters::profiles::ALL
        .iter()
        .map(|profile| profile.planning_view())
        .collect();
    tools.push(json!({"id":"custom","purpose":"An acquisition operation unsupported by selected collectors. Missing independent review records remain unresolved evidence gaps.","arguments":CUSTOM_ARGUMENTS}));
    if let Some(selected) = selected {
        tools.extend(
            selected
                .planning_view()
                .as_array()
                .expect("catalogue array")
                .iter()
                .cloned(),
        );
    }
    json!(tools)
}

pub fn planned_case_schema() -> Value {
    planned_case_schema_with(None)
}

pub fn planned_case_schema_with(selected: Option<&CollectorCatalogue>) -> Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(PlannedArgument)).expect("schema serialises");
    schema.as_object_mut().expect("object").remove("$schema");
    schema["$defs"]["AcquisitionPlan"]["properties"]["tool_id"]["enum"] = json!(
        catalogue_with(selected)
            .as_array()
            .expect("catalogue array")
            .iter()
            .map(|tool| tool["id"].clone())
            .collect::<Vec<_>>()
    );
    // Structured-output providers require all object properties, with null for
    // an unavailable optional value. Deserialisation still admits saved plans
    // that predate preparation metadata.
    let required = schema["$defs"]["AcquisitionPlan"]["required"]
        .as_array_mut()
        .expect("plan required fields");
    if !required.iter().any(|field| field == "input_preparation") {
        required.push(json!("input_preparation"));
    }
    schema["$defs"]["AcquisitionPlan"]["properties"]["input_preparation"] = json!({
        "anyOf": [{"$ref":"#/$defs/InputPreparation"}, {"type":"null"}]
    });
    // The tagged variants are disjoint, so anyOf preserves the Rust wire
    // contract while avoiding unsupported oneOf/const provider keywords.
    schema["$defs"]["InputPreparation"] = json!({"anyOf":[
        {"type":"object", "additionalProperties":false,
         "properties":{"kind":{"type":"string","enum":["scoped"]},
                       "operation":{"$ref":"#/$defs/ScopedOperation"}},
         "required":["kind","operation"]},
        {"type":"object", "additionalProperties":false,
         "properties":{"kind":{"type":"string","enum":["artifact"]},
                       "reference":{"type":"string"}},
         "required":["kind","reference"]}
    ]});
    // Share the same provider-facing warrant contract with plain and graph
    // authoring. Saved flat plans remain admitted by their serde defaults.
    for (name, definition) in crate::domain::case_schema()["$defs"]
        .as_object()
        .expect("case definitions")
    {
        schema["$defs"][name] = definition.clone();
    }
    schema
}

/// References name operator-supplied retained records rather than filesystem paths.
pub fn valid_artifact_reference(reference: &str) -> bool {
    (1..=128).contains(&reference.len())
        && reference.as_bytes()[0].is_ascii_alphanumeric()
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Decode the current acquisition response. A missing plan is not inferred.
pub fn parse_completion(content: &str) -> Result<PlannedArgument, Error> {
    serde_json::from_str(content)
        .map_err(|error| Error::InvalidOutput(format!("invalid acquisition response: {error}")))
}

/// Validate selection and exact need binding, without asserting that proposed inputs are true.
pub fn validate(case: &ArgumentCase, plans: &[AcquisitionPlan]) -> Vec<String> {
    validate_with_catalogue(case, plans, None)
}

/// Validate a current plan against implemented defaults and an explicitly selected catalogue.
pub fn validate_with_catalogue(
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
    selected: Option<&CollectorCatalogue>,
) -> Vec<String> {
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
        match &plan.input_preparation {
            Some(InputPreparation::Scoped { .. }) if plan.tool_id != "scoped_json" => {
                errors.push(format!(
                    "acquisition plan {} scoped preparation requires scoped_json",
                    plan.need_id
                ));
            }
            Some(InputPreparation::Artifact { reference }) => {
                if plan.tool_id != "artifact_json" {
                    errors.push(format!(
                        "acquisition plan {} artifact preparation requires artifact_json",
                        plan.need_id
                    ));
                }
                if !valid_artifact_reference(reference) {
                    errors.push(format!(
                        "acquisition plan {} artifact reference must be a bounded identifier",
                        plan.need_id
                    ));
                }
            }
            _ => {}
        }
        let descriptor = selected.and_then(|catalogue| catalogue.get(&plan.tool_id));
        let required = descriptor
            .map(|tool| {
                tool.0
                    .parameters
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            })
            .or_else(|| required_arguments(&plan.tool_id).map(|fields| fields.to_vec()));
        let Some(required) = required else {
            errors.push(format!(
                "acquisition plan {} selects unknown tool {}",
                plan.need_id, plan.tool_id
            ));
            continue;
        };
        let mandatory = if let Some(descriptor) = descriptor {
            match descriptor.required_parameters() {
                Ok(parameters) => parameters,
                Err(error) => {
                    errors.push(format!("invalid collector requirements {error}"));
                    continue;
                }
            }
        } else {
            required.iter().map(|name| (*name).to_owned()).collect()
        };
        if let Some(tool) = descriptor
            && let Err(error) = tool.input(plan)
        {
            errors.push(format!(
                "acquisition plan {} has invalid input {error}",
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
        for name in mandatory {
            if !inputs.contains(name.as_str()) {
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
    override_plans_with_catalogue(case, retained, overrides, None)
}

pub fn override_plans_with_catalogue(
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    overrides: Vec<AcquisitionPlan>,
    catalogue: Option<&CollectorCatalogue>,
) -> Result<Vec<AcquisitionPlan>, Error> {
    if !retained.is_empty() {
        let errors = validate_with_catalogue(case, retained, catalogue);
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
                    input_preparation: None,
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
    let errors = validate_with_catalogue(case, &plans, catalogue);
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    Ok(plans)
}

/// Current generation admits only implemented defaults and the operator-selected catalogue.
pub fn validate_selected(
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
    selected: Option<&CollectorCatalogue>,
) -> Vec<String> {
    let mut errors = validate_with_catalogue(case, plans, selected);
    let available = catalogue_with(selected);
    for plan in plans {
        if !available
            .as_array()
            .expect("catalogue array")
            .iter()
            .any(|tool| tool["id"] == plan.tool_id)
        {
            errors.push(format!(
                "collector {} is absent from the selected catalogue",
                plan.tool_id
            ));
        }
    }
    errors
}

pub(crate) const PLANNING_INSTRUCTIONS: &str = concat!(
    "ACQUISITION PLANNING: acquisition_tools is the complete selected collector catalogue. ",
    "Select by the measurement needed for this argument. Prefer an implemented collector whose actual operation supplies the required observation. ",
    "Missing observations, credentials or input values do not require a custom implementation. Keep missing named inputs in unresolved_arguments while retaining the fitting implemented collector. ",
    "For scoped_json set input_preparation to {kind:scoped,operation:kubernetes|grafana_search|grafana_trace} according to the actual required operation. ",
    "A full trace requires supplied actual trace IDs. Scope settings alone do not supply trace IDs or establish observations. ",
    "artifact_json imports existing retained records and never creates missing measurements, experiments, derivations or independent reviews. ",
    "Use input_preparation {kind:artifact,reference:ALIAS} only for an identified operator-supplied retained source reference. ",
    "References are 1 to 128 ASCII characters, starting with a letter or digit and continuing with letters, digits, dot, underscore or hyphen. Never invent an alias, path, hash, provenance or observation. ",
    "If a producing acquisition operation is required, choose an implemented collector performing that operation, or custom when none performs it. Selecting an artifact reader does not identify or implement its producer. ",
    "An unavailable independent review remains an unresolved evidence gap with null input_preparation and unresolved inputs. Generated code and generated judgements cannot manufacture that independent review. ",
    "Use null input_preparation for other adapters. Each collector describes its admitted inputs and measurement results. required_arguments names unconditional input and command requirements. ",
    "Optional input properties may be omitted. Preserve supplied null separately from an omitted property. Respect declared time-window constraints. Keep empty search results and incomplete coverage separate from demonstrated absence. ",
    "For a string property supply the literal string. For another JSON type encode the value as JSON in the named string value. Use custom only when no selected collector performs the required operation. ",
    "Derive values only from supplied claim, context or evidence. Reuse a compatible acquisition for shared investigations while retaining a separate plan for every targeted evidence need. ",
    "An existing independently authored review can be acquired as a retained artifact, but a generated judgement or successful acquisition cannot supply independent acceptance. Keep unavailable review records and their inputs unresolved. ",
    "A proposed collector is an unperformed plan. Adapter selection and a zero-call implementation stage establish neither configured inputs nor available evidence. ",
    "Successful acquisition establishes the recorded measurement, and independent review establishes applicability and sufficiency."
);

pub(crate) const ENVELOPE_INSTRUCTIONS: &str = "PLANNED RESPONSE: Return the supplied envelope with argument holding the complete unchanged ArgumentCase and acquisition_plan holding exactly one record for every argument.evidence_plan ID. Each record names need_id and one acquisition_tools ID and supplies input_preparation as its typed preparation instruction or null. Put each required named input either in arguments as {name,value} string entries or in unresolved_arguments, never both. Use only the catalogue's input names. For custom, arguments is empty and unresolved_arguments is [\"adapter\"], with null input_preparation. Revise the argument and its acquisition bindings together. The plan is separate from the canonical case and must preserve its target, enabled claim, question, measurement, comparator and acceptance criterion.";

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
                input_preparation: None,
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
    fn provider_envelope_requires_an_explicit_acquisition_plan() {
        let case = case();
        let canonical = serde_json::to_string(&case).unwrap();
        assert!(parse_completion(&canonical).is_err());
        let planned = PlannedArgument {
            acquisition_plan: custom_plans(&case),
            argument: case,
        };
        let parsed = parse_completion(&serde_json::to_string(&planned).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed.argument).unwrap(),
            serde_json::from_str::<Value>(&canonical).unwrap()
        );
        assert_eq!(
            planned_case_schema()["properties"]["argument"]["$ref"],
            "#/$defs/ArgumentCase"
        );
    }

    #[test]
    fn preparation_metadata_preserves_legacy_plan_bytes_and_is_explicit_when_supplied() {
        let source = r#"{"need_id":"E1","tool_id":"scoped_json","arguments":[],"unresolved_arguments":["inputs"]}"#;
        let mut plan: AcquisitionPlan = serde_json::from_str(source).unwrap();
        assert!(plan.input_preparation.is_none());
        assert_eq!(serde_json::to_string(&plan).unwrap(), source);
        plan.input_preparation = Some(InputPreparation::Scoped {
            operation: ScopedOperation::GrafanaTrace,
        });
        assert_eq!(
            serde_json::to_value(&plan).unwrap()["input_preparation"],
            json!({"kind":"scoped","operation":"grafana_trace"})
        );
        let malformed = json!({"need_id":"E1","tool_id":"scoped_json","arguments":[],
            "unresolved_arguments":["inputs"],
            "input_preparation":{"kind":"scoped","operation":"grafana_query"}});
        assert!(serde_json::from_value::<AcquisitionPlan>(malformed).is_err());
    }

    #[test]
    fn preparation_validates_adapter_roles_and_retained_reference_identifiers() {
        let case = case();
        let mut plans = custom_plans(&case);
        plans[0].tool_id = "scoped_json".into();
        plans[0].unresolved_arguments = vec!["inputs".into()];
        plans[0].input_preparation = Some(InputPreparation::Scoped {
            operation: ScopedOperation::GrafanaSearch,
        });
        assert!(validate(&case, &plans).is_empty());
        plans[0].input_preparation = Some(InputPreparation::Artifact {
            reference: "existing-record_1.json".into(),
        });
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|error| error.contains("requires artifact_json"))
        );
        plans[0].tool_id = "artifact_json".into();
        assert!(validate(&case, &plans).is_empty());
        for reference in [
            "",
            "../old-case.json",
            "/operator/capture.json",
            "has space",
            "a:b",
        ] {
            plans[0].input_preparation = Some(InputPreparation::Artifact {
                reference: reference.into(),
            });
            assert!(
                validate(&case, &plans)
                    .iter()
                    .any(|error| error.contains("bounded identifier"))
            );
        }
        assert!(!valid_artifact_reference(&"a".repeat(129)));
        plans[0].input_preparation = None;
        assert!(validate(&case, &plans).is_empty());
    }

    #[test]
    fn provider_preparation_schema_requires_nullable_closed_tagged_variants() {
        let planned = planned_case_schema();
        let schema = json!({"$ref":"#/$defs/AcquisitionPlan","$defs":planned["$defs"]});
        let validator = jsonschema::validator_for(&schema).unwrap();
        let mut plan = json!({"need_id":"E1","tool_id":"scoped_json","arguments":[],
            "unresolved_arguments":["inputs"],"input_preparation":null});
        assert!(validator.is_valid(&plan));
        for operation in ["kubernetes", "grafana_search", "grafana_trace"] {
            plan["input_preparation"] = json!({"kind":"scoped","operation":operation});
            assert!(validator.is_valid(&plan));
        }
        plan["input_preparation"] = json!({"kind":"artifact","reference":"capture_1"});
        assert!(validator.is_valid(&plan));
        plan["input_preparation"]["operation"] = json!("grafana_search");
        assert!(!validator.is_valid(&plan));
        plan["input_preparation"] = json!({"kind":"scoped","operation":"other"});
        assert!(!validator.is_valid(&plan));
        plan.as_object_mut().unwrap().remove("input_preparation");
        assert!(!validator.is_valid(&plan));
        fn forbidden(value: &Value) -> bool {
            match value {
                Value::Object(fields) => {
                    fields.contains_key("oneOf")
                        || fields.contains_key("const")
                        || fields.values().any(forbidden)
                }
                Value::Array(values) => values.iter().any(forbidden),
                _ => false,
            }
        }
        assert!(!forbidden(&schema));
    }
}
