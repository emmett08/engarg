//! EngArg case-specific mapping to optional EAL source adapters.
//! Retain the facade until native catalogues preserve its literal-query contract
//! and existing saved bindings have a verified migration.
use crate::Error;
use eal_adapters::scoped::{Grafana, Kubernetes, Operation};
use eal_collector_contracts::AcquisitionRequest;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_BYTES: usize = 1_048_576;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    schema: String,
    case_digest: String,
    needs: BTreeMap<String, Operation>,
    kubernetes: Option<Kubernetes>,
    grafana: Option<Grafana>,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}
fn timestamp() -> Result<Value, Error> {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("collector clock is before Unix epoch"))?;
    Ok(json!({"unix_seconds":t.as_secs(),"subsec_nanos":t.subsec_nanos()}))
}
fn read_inputs(path: &Path) -> Result<(Inputs, String), Error> {
    let bytes = crate::bounded_file::read(path, MAX_BYTES, "scoped inputs")?;
    let inputs: Inputs = serde_json::from_value(crate::retained_collectors::parse_json(&bytes)?)
        .map_err(|_| invalid("invalid engarg.scoped-inputs/1 input file"))?;
    if inputs.schema != "engarg.scoped-inputs/1" || !nonempty(&inputs.case_digest) {
        return Err(invalid(
            "scoped inputs require their schema and exact case digest",
        ));
    }
    Ok((inputs, sha256(&bytes)))
}

fn validate_binding<'a>(
    request: &'a AcquisitionRequest,
    inputs: &Inputs,
) -> Result<&'a str, Error> {
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ]
    .iter()
    .any(|value| !nonempty(value))
        || request.input.get("schema").and_then(Value::as_str) != Some("engarg.eal-acquisition/1")
        || request.input.get("case_digest").and_then(Value::as_str)
            != Some(inputs.case_digest.as_str())
        || request
            .context
            .get("engarg_export_digest")
            .and_then(Value::as_str)
            != Some(inputs.case_digest.as_str())
    {
        return Err(invalid(
            "scoped collection requires an exact EAL acquisition and case digest binding",
        ));
    }
    request
        .input
        .get("need")
        .and_then(|need| need.get("id"))
        .and_then(Value::as_str)
        .filter(|id| nonempty(id) && inputs.needs.contains_key(*id))
        .ok_or_else(|| invalid("scoped inputs have no operation for this exact evidence need"))
}

/// Check case identity before delegating source acquisition.
pub async fn observe(value: Value, inputs_path: &Path) -> Result<Value, Error> {
    let request: AcquisitionRequest = serde_json::from_value(value)?;
    request.validate().map_err(Error::InvalidInput)?;
    if request.tool_version != eal_adapters::profiles::SCOPED.version {
        return Err(invalid(
            "collector request version differs from its selected compatibility profile",
        ));
    }
    let (inputs, inputs_sha256) = read_inputs(inputs_path)?;
    let need = validate_binding(&request, &inputs)?;
    let started_at = timestamp()?;
    let observation = match inputs.needs[need] {
        Operation::Kubernetes => {
            eal_adapters::scoped::kubernetes(
                inputs
                    .kubernetes
                    .as_ref()
                    .ok_or_else(|| invalid("scoped inputs require Kubernetes configuration"))?,
            )
            .await?
        }
        operation @ (Operation::GrafanaSearch | Operation::GrafanaTrace) => {
            eal_adapters::scoped::grafana(
                inputs
                    .grafana
                    .as_ref()
                    .ok_or_else(|| invalid("scoped inputs require Grafana configuration"))?,
                operation,
            )
            .await?
        }
    };
    eal_adapters::observation(&request,json!({"collected":true,"measurement_only":true,"observation":observation,
        "details":{"source_inputs_sha256":inputs_sha256,"started_at":started_at,"finished_at":timestamp()?}}),None,None).map_err(Into::into)
}
