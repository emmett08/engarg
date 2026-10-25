//! Targeted support repair reuses paid assessments and freezes the accepted argument.
use super::*;

/// Each attempted completion reaches `observer` before parsing. The caller can
/// checkpoint it even when a later attempt fails or exhausts its budget.
pub async fn repair_support<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    original: &SupportedArgument,
    transcript: &[CallRecord],
    config: &RunConfig,
    observer: &O,
) -> Result<(SupportedArgument, Vec<CallRecord>), Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    if !(1..=3).contains(&config.max_repairs) || config.max_calls != u32::from(config.max_repairs) {
        return Err(Error::InvalidInput(
            "support repair requires 1–3 separately budgeted calls".into(),
        ));
    }
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    crate::domain::validate_argument(request, &original.argument).map_err(|errors| {
        Error::InvalidInput(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let design = support_requirements::review_acquisition_design(
        &original.support_elaboration,
        transcript,
        16 * 1_048_576,
    )?;
    let mut candidate = original.clone();
    let mut calls = Vec::new();
    let mut errors = support_requirements::validate_elaboration(
        &original.argument,
        &original.support_elaboration,
    );
    if errors.is_empty() {
        errors.push("Review the retained pending acquisition designs and repair their representations. Preserve genuine source, method, configuration and judgement gaps explicitly; do not invent qualification or retire a design merely to pass validation.".into());
    }
    for attempt in 1..=config.max_repairs {
        let data = json!({
            "request":request,
            "candidate_json":{"argument":candidate.argument,"support_elaboration":candidate.support_elaboration},
            "validation_errors":errors,
            "repair_attempt":attempt,"repair_limit":config.max_repairs,
            "original_support_elaboration":original.support_elaboration,
            "original_requirement_sha256":requirement_identities(&original.support_elaboration)?,
            "retained_acquisition_designs":design,
            "frozen_argument":original.argument,
        });
        let mut prompt = support_prompt(Role::Adjudicator, data, config)?;
        prompt.system.push_str("\nTARGETED SUPPORT RECOVERY: return the frozen_argument exactly unchanged. Repair only the support elaboration, retaining every original requirement and acceptance clause and prior-digest revision when changed. Retained acquisition designs are fallible, non-executable repair inputs. Correct schemas, input slots and lineage instead of treating structural diagnostics as source unavailability. Missing real sources and qualified methods remain gaps. Never invent observation results or qualification. Return a complete envelope, not a patch.");
        let call = execute(model, prompt, config, observer).await?;
        let parsed = support_requirements::parse_supported_argument(
            &call.response.content,
            config.max_response_bytes,
        );
        calls.push(call);
        match parsed {
            Ok(repaired) => {
                errors = support_requirements::validate_revision(
                    &original.argument,
                    &original.support_elaboration,
                    &repaired.argument,
                    &repaired.support_elaboration,
                );
                if serde_json::to_value(&repaired.argument)?
                    != serde_json::to_value(&original.argument)?
                {
                    errors.push("targeted support repair must preserve frozen_argument exactly, including every objection, status, scope and acceptance clause".into());
                }
                if errors.is_empty() {
                    return Ok((repaired, calls));
                }
                candidate = repaired;
            }
            Err(error) => errors = vec![error.to_string()],
        }
    }
    Err(Error::InvalidOutput(format!(
        "{}; support repair exhausted {} calls; completed responses remain in the repair audit",
        errors.join("; "),
        config.max_repairs,
    )))
}
