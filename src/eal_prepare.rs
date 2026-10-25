//! Prepare private EAL files while preserving each evidence need's chosen collector.

use crate::{
    Error,
    acquisition::AcquisitionPlan,
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request},
    eal_acquisition::export_with_policy,
};
use serde_json::json;
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

/// Retain supplied need bindings and add an unresolved stub only for an unbound need.
fn complete_plans(
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
) -> Result<Vec<AcquisitionPlan>, Error> {
    let mut by_need = BTreeMap::new();
    for plan in retained {
        if !case
            .evidence_plan
            .iter()
            .any(|need| need.id == plan.need_id)
            || by_need.insert(plan.need_id.as_str(), plan).is_some()
        {
            return Err(Error::InvalidInput(
                "preparation repeats a need or names an unknown evidence need".into(),
            ));
        }
    }
    Ok(case
        .evidence_plan
        .iter()
        .map(|need| {
            by_need
                .get(need.id.as_str())
                .map(|plan| (**plan).clone())
                .unwrap_or_else(|| AcquisitionPlan {
                    input_preparation: None,
                    need_id: need.id.clone(),
                    tool_id: "custom".into(),
                    arguments: Vec::new(),
                    unresolved_arguments: vec!["adapter".into()],
                })
        })
        .collect())
}

/// Compatibility entry point. Infrastructure settings now belong to each need's binding.
///
/// Earlier callers can retain this signature when both legacy overrides are absent.
pub fn prepare(
    request: &Request,
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    directory: &Path,
    executable: &Path,
    kube_context: Option<&str>,
    grafana_origin: Option<&str>,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    if kube_context.is_some() || grafana_origin.is_some() {
        return Err(Error::InvalidInput(
            "Kubernetes and Grafana preparation overrides require explicit per-need collector bindings and a selected collector catalogue".into(),
        ));
    }
    prepare_with_catalogue(request, case, retained, directory, executable, None)
}

/// Prepare source, bindings and pending reviews without acquiring or replacing inputs.
///
/// The optional catalogue is operator-selected. Each retained plan keeps its adapter,
/// inputs and unresolved parameters. Existing artifact-input files remain operator-owned.
pub fn prepare_with_catalogue(
    request: &Request,
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    directory: &Path,
    executable: &Path,
    catalogue: Option<&CollectorCatalogue>,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    prepare_with_policy(
        request,
        case,
        retained,
        directory,
        executable,
        catalogue,
        &crate::export_policy::ExportPolicy::default(),
    )
}

pub fn prepare_with_policy(
    request: &Request,
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    directory: &Path,
    executable: &Path,
    catalogue: Option<&CollectorCatalogue>,
    policy: &crate::export_policy::ExportPolicy,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    prepare_with_operator_inputs(
        request, case, retained, directory, executable, catalogue, policy, None,
    )
}

/// Prepare current-case inputs from declared operations and operator-owned scope.
/// Paths are fixed before export so every generated input uses the final revision.
#[allow(clippy::too_many_arguments)]
pub fn prepare_with_operator_inputs(
    request: &Request,
    case: &ArgumentCase,
    retained: &[AcquisitionPlan],
    directory: &Path,
    executable: &Path,
    catalogue: Option<&CollectorCatalogue>,
    policy: &crate::export_policy::ExportPolicy,
    operator: Option<(&serde_json::Value, &Path)>,
) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    if !directory.is_absolute() || !executable.is_absolute() {
        return Err(Error::InvalidInput(
            "prepare requires absolute output and executable paths".into(),
        ));
    }
    let executable = path_text(executable)?;
    let review_path = path_text(&directory.join("review-inputs.json"))?;
    let plans = complete_plans(case, retained)?;
    let prepared = operator
        .map(|(inputs, base)| {
            crate::collector_preparation::prepare_plans(case, &plans, inputs, base, directory)
        })
        .transpose()?;
    let plans = prepared.as_ref().map_or(&plans, |prepared| &prepared.plans);
    let export = export_with_policy(request, case, plans, catalogue, policy)?;
    let mut registry = export.registry;
    let review = registry
        .tools
        .get_mut("engarg_review_contract")
        .ok_or_else(|| Error::InvalidInput("export omitted its review binding".into()))?;
    review.argv = vec![
        executable.clone(),
        "eal".into(),
        "observe".into(),
        "--tool".into(),
        "review_json".into(),
        "--inputs".into(),
        review_path,
    ];
    review.inherit_env.clear();
    review.parallel_safe = false;
    for binding in registry.tools.values_mut() {
        if binding.argv.first().is_some_and(|path| path == "engarg") {
            binding.argv[0] = executable.clone();
        }
    }
    let bundle = export.bundle;
    let mut files = BTreeMap::new();
    files.insert("argument.eal".into(), bundle.source.as_bytes().to_vec());
    files.insert("argument.bundle.json".into(), bytes(&bundle)?);
    files.insert(
        "argument.tools.toml".into(),
        registry
            .to_toml()
            .map_err(Error::InvalidInput)?
            .into_bytes(),
    );
    files.insert("acquisition-inputs.json".into(), bytes(plans)?);
    files.insert(
        "review-inputs.json".into(),
        bytes(&json!({"schema": "engarg.review-inputs/1", "reviews": []}))?,
    );
    files.insert(
        "review-requests.json".into(),
        bytes(&export.review_requests)?,
    );
    files.insert("context.json".into(), bytes(&bundle.context)?);
    if let Some(prepared) = prepared {
        files.extend(prepared.finish(&bundle.revision)?);
    }
    Ok(files)
}
