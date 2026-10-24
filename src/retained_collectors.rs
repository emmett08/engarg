//! EngArg-specific case and review binding over optional EAL retained adapters.
//! Remove facade commands in EngArg 0.11 after operators migrate catalogues.
use crate::Error;
use eal_adapters::retained::{Artifact, Review};
use eal_collector_contracts::AcquisitionRequest;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
const MAX_FILE_BYTES: usize = 1_048_576;
const MAX_ARTIFACT_BYTES: usize = 4_194_304;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactInputs {
    schema: String,
    case_digest: String,
    needs: BTreeMap<String, ArtifactNeed>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactNeed {
    artifacts: Vec<Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewInputs {
    schema: String,
    reviews: Vec<Review>,
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn parse_json(bytes: &[u8]) -> Result<Value, Error> {
    eal_adapters::parse_json(bytes).map_err(Into::into)
}

/// Verify case identity before importing exact retained measurements or review decisions.
pub fn observe(tool: &str, value: Value, inputs_path: &Path) -> Result<Value, Error> {
    if serde_json::to_vec(&value)?.len() > MAX_FILE_BYTES {
        return Err(Error::Budget(
            "retained collector request exceeds 1 MiB".into(),
        ));
    }
    let request: AcquisitionRequest = serde_json::from_value(value)?;
    request.validate().map_err(Error::InvalidInput)?;
    let expected_version = if tool == "artifact_json" {
        eal_adapters::profiles::ARTIFACT.version
    } else {
        "1"
    };
    if request.tool_version != expected_version || !request.input.is_object() {
        return Err(invalid(
            "invalid retained collector request identity or objects",
        ));
    }
    let case_digest = request.input["case_digest"]
        .as_str()
        .filter(|v| digest(v))
        .ok_or_else(|| invalid("retained collector requires the exact export case_digest"))?;
    if request.context["engarg_export_digest"].as_str() != Some(case_digest) {
        return Err(invalid(
            "retained collector case digest differs from its scoped context",
        ));
    }
    let bytes = crate::bounded_file::read(inputs_path, MAX_FILE_BYTES, "retained inputs")?;
    let (value, observed_at) = match tool {
        "artifact_json" => {
            let inputs: ArtifactInputs = serde_json::from_value(parse_json(&bytes)?)?;
            if inputs.schema != "engarg.artifact-inputs/1" || inputs.case_digest != case_digest {
                return Err(invalid("retained artifact inputs do not match this export"));
            }
            if request.input["schema"] != "engarg.eal-acquisition/1" {
                return Err(invalid(
                    "retained artifacts require an EngArg acquisition input",
                ));
            }
            let need = request.input["need"]["id"]
                .as_str()
                .filter(|id| {
                    !id.trim().is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control)
                })
                .ok_or_else(|| invalid("retained artifacts require an exact evidence need ID"))?;
            let selected = inputs.needs.get(need).ok_or_else(|| {
                invalid("retained artifact inputs have no matching evidence need")
            })?;
            let canonical = inputs_path.canonicalize()?;
            let base = canonical
                .parent()
                .ok_or_else(|| invalid("retained inputs require a parent directory"))?;
            eal_adapters::retained::import_artifacts(&selected.artifacts, base)?
        }
        "review_json" => {
            let inputs: ReviewInputs = serde_json::from_value(parse_json(&bytes)?)?;
            if inputs.schema != "engarg.review-inputs/1" {
                return Err(invalid("invalid retained review schema"));
            }
            if request.tool != "engarg_review_contract" || request.input.get("obligation").is_none()
            {
                return Err(invalid(
                    "retained review requires an exact independent review request",
                ));
            }
            eal_adapters::retained::import_review(&request, inputs.reviews)?
        }
        _ => return Err(invalid("unknown retained collector")),
    };
    let envelope = eal_adapters::observation(
        &request,
        value,
        Some(observed_at),
        Some(
            json!({"collector":"engarg-retained/1","inputs_sha256":Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>(),"tool":tool}),
        ),
    )?;
    if serde_json::to_vec(&envelope)?.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::Budget(
            "retained observation envelope exceeds 4 MiB".into(),
        ));
    }
    Ok(envelope)
}
