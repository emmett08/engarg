//! Prepare private, executable collector configuration without acquiring evidence.

use crate::{
    Error,
    acquisition::{AcquisitionPlan, ToolArgument},
    domain::{ArgumentCase, Request},
    eal_acquisition::export_with_plan,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, Error> {
    let mut result = serde_json::to_vec_pretty(value)?;
    result.push(b'\n');
    Ok(result)
}

fn path_text(path: &Path) -> Result<String, Error> {
    path.to_str()
        .filter(|text| !text.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| Error::InvalidInput("collector paths must be UTF-8 without controls".into()))
}

fn proposed_operation(plan: Option<&AcquisitionPlan>, method: &str) -> Option<&'static str> {
    match plan.map(|plan| plan.tool_id.as_str()) {
        Some("kubernetes" | "kubectl_get") => Some("kubernetes"),
        Some("tempo") => Some("grafana_search"),
        Some("http_json_get") => {
            let text = method.to_lowercase();
            if text.contains("full") || text.contains("trace-by-id") || text.contains("trace by id")
            {
                Some("grafana_trace")
            } else if ["tempo", "grafana", "traceql", "query"]
                .iter()
                .any(|word| text.contains(word))
            {
                Some("grafana_search")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn common_argument(plans: &[AcquisitionPlan], name: &str) -> String {
    let values: std::collections::BTreeSet<_> = plans
        .iter()
        .flat_map(|plan| &plan.arguments)
        .filter(|argument| argument.name == name)
        .map(|argument| argument.value.as_str())
        .collect();
    if values.len() == 1 {
        values.into_iter().next().unwrap().to_owned()
    } else {
        String::new()
    }
}

fn review_requests(source: &str, context: &Value) -> Result<Vec<Value>, Error> {
    let mut result = Vec::new();
    let mut evidence = None;
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("evidence ") {
            evidence = rest.strip_suffix(" {").map(str::to_owned);
        } else if let Some(input) = line.strip_prefix("  input ") {
            let input: Value = serde_json::from_str(input)?;
            if input.get("obligation").is_some() {
                result.push(json!({
                    "evidence_id": evidence,
                    "environment": "engarg_case",
                    "tool": "engarg_review_contract",
                    "tool_version": "1",
                    "input": input,
                    "context": context
                }));
            }
        }
    }
    Ok(result)
}

/// Return files for a new private directory. Missing operator facts stay unset.
///
/// Proposed operations must be checked in collector-inputs.json before use.
/// Acquisitions remain measurements, and every original independent review is retained.
pub fn prepare(
    request: &Request,
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    directory: &Path,
    executable: &Path,
    kube_context: Option<&str>,
    grafana_origin: Option<&str>,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    if !directory.is_absolute() || !executable.is_absolute() {
        return Err(Error::InvalidInput(
            "prepare requires absolute output and executable paths".into(),
        ));
    }
    let artifact_path = path_text(&directory.join("artifact-inputs.json"))?;
    let scoped_path = path_text(&directory.join("collector-inputs.json"))?;
    let review_path = path_text(&directory.join("review-inputs.json"))?;
    let executable = path_text(executable)?;
    let mut artifacts = BTreeMap::<String, Value>::new();
    let mut operations = BTreeMap::<String, &str>::new();
    let plans: Vec<_> = case
        .evidence_plan
        .iter()
        .map(|need| {
            let plan = retained.iter().find(|plan| plan.need_id == need.id);
            let operation =
                proposed_operation(plan, &format!("{} {}", need.method, need.measurement));
            let (tool_id, path) = if let Some(operation) = operation {
                operations.insert(need.id.clone(), operation);
                ("scoped_json", &scoped_path)
            } else {
                artifacts.insert(need.id.clone(), json!({"artifacts": []}));
                ("artifact_json", &artifact_path)
            };
            AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: tool_id.into(),
                arguments: vec![ToolArgument {
                    name: "inputs".into(),
                    value: path.clone(),
                }],
                unresolved_arguments: Vec::new(),
            }
        })
        .collect();
    let export = export_with_plan(request, case, &plans)?;
    let bundle = export.bundle;
    let review_argv = json!([
        executable,
        "eal",
        "observe",
        "--tool",
        "review_json",
        "--inputs",
        review_path
    ]);
    let registry = export
        .tools_toml
        .replace(
            "# Replace review stubs with a trusted reviewer that handles each exact request.",
            "# Independently authored review records must match each exact request.",
        )
        .replace(
            "# REPLACE selectors and executable paths require operator configuration.",
            "# Configure private input files before collection; missing inputs fail closed.",
        )
        .replace(
            "# Stub for independent review. Each review has a different acquisition request.",
            "# Retained independent reviews. Each review has a different acquisition request.",
        )
        .replace(
            "argv = [\"/REPLACE_WITH_INDEPENDENT_REVIEW_COLLECTOR\"]",
            &format!("argv = {review_argv}\ninherit_env = []\nparallel_safe = false"),
        )
        .replace(
            "argv = [\"engarg\",",
            &format!("argv = [{},", serde_json::to_string(&executable)?),
        );
    let query = request
        .claim
        .find('{')
        .and_then(|start| {
            request
                .claim
                .rfind('}')
                .filter(|end| *end > start)
                .map(|end| request.claim[start..=end].to_owned())
        })
        .unwrap_or_default();
    let scoped = json!({
        "schema": "engarg.scoped-inputs/1",
        "case_digest": bundle.revision,
        "needs": operations,
        "kubernetes": {
            "executable": "kubectl", "context": kube_context.unwrap_or(""),
            "resource": null, "namespace": null,
            "kind": common_argument(retained, "kind"), "name": common_argument(retained, "name")
        },
        "grafana": {
            "origin": grafana_origin.unwrap_or(""), "datasource_uid": null,
            "query": query, "start_unix": null, "end_unix": null, "trace_ids": [],
            "rollout_kind": common_argument(retained, "kind"),
            "rollout_name": common_argument(retained, "name"),
            "token_env": null, "tenant_env": null
        }
    });
    let mut files = BTreeMap::new();
    files.insert("argument.eal".into(), bundle.source.as_bytes().to_vec());
    files.insert("argument.bundle.json".into(), bytes(&bundle)?);
    files.insert("argument.tools.toml".into(), registry.into_bytes());
    files.insert("acquisition-inputs.json".into(), bytes(&plans)?);
    files.insert("collector-inputs.json".into(), bytes(&scoped)?);
    files.insert(
        "artifact-inputs.json".into(),
        bytes(&json!({
            "schema": "engarg.artifact-inputs/1", "case_digest": bundle.revision, "needs": artifacts
        }))?,
    );
    files.insert(
        "review-inputs.json".into(),
        bytes(&json!({
            "schema": "engarg.review-inputs/1", "reviews": []
        }))?,
    );
    files.insert(
        "review-requests.json".into(),
        bytes(&review_requests(&bundle.source, &bundle.context)?)?,
    );
    files.insert("context.json".into(), bytes(&bundle.context)?);
    Ok(files)
}
