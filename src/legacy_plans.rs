//! Frozen plan admission used only by persisted v1/v2 reconstruction.
use std::collections::BTreeSet;

use serde::Serialize;

use crate::{domain::ArgumentCase, legacy_catalogue::CollectorCatalogue};

pub use crate::acquisition::AcquisitionPlan;

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
    if let Some(tool) = crate::legacy_native_collectors::spec(tool_id) {
        return Some(tool.parameters);
    }
    TOOLS
        .iter()
        .find(|tool| tool.id == tool_id)
        .map(|tool| tool.arguments)
}

/// Validate selection and exact need binding, without asserting that proposed inputs are true.
pub fn validate(case: &ArgumentCase, plans: &[AcquisitionPlan]) -> Vec<String> {
    validate_with_catalogue(case, plans, None)
}

/// Validate a recorded plan. Legacy built-in IDs remain readable for existing reports.
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
        if plan.input_preparation.is_some() {
            errors.push(
                "historical acquisition plans do not admit current-case preparation metadata"
                    .into(),
            );
        }
        if !seen.insert(plan.need_id.as_str()) {
            errors.push(format!("duplicate acquisition plan for {}", plan.need_id));
        }
        if !expected.contains(plan.need_id.as_str()) {
            errors.push(format!(
                "acquisition plan names unknown evidence need {}",
                plan.need_id
            ));
        }
        let descriptor = selected.and_then(|catalogue| catalogue.get(&plan.tool_id));
        let required = descriptor
            .map(|tool| {
                tool.parameters
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
        if let Some(tool) = descriptor
            && let Err(error) = tool.input(plan)
        {
            errors.push(format!(
                "acquisition plan {} has invalid input {error}",
                plan.need_id
            ));
        }
        if descriptor.is_none()
            && let Some(tool) = crate::legacy_native_collectors::spec(&plan.tool_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acquisition::{InputPreparation, ScopedOperation};

    #[test]
    fn historical_admission_preserves_its_original_plan_contract() {
        let case: ArgumentCase =
            serde_json::from_str(include_str!("../examples/case.json")).unwrap();
        let mut plans: Vec<_> = case
            .evidence_plan
            .iter()
            .map(|need| AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: "custom".into(),
                arguments: vec![],
                unresolved_arguments: vec!["adapter".into()],
                input_preparation: None,
            })
            .collect();
        assert!(validate(&case, &plans).is_empty());
        plans[0].tool_id = "scoped_json".into();
        plans[0].unresolved_arguments = vec!["inputs".into()];
        plans[0].input_preparation = Some(InputPreparation::Scoped {
            operation: ScopedOperation::GrafanaSearch,
        });
        assert!(
            validate(&case, &plans)
                .iter()
                .any(|error| error.contains("historical acquisition plans"))
        );
    }
}
