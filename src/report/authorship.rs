//! Preserve declared origin and final authoring output across executable reports.
//!
//! Retained hashes check consistency of unsigned records, not source authenticity.
use crate::{
    Error, authoring_checkpoint::Checkpoint, domain::ArgumentCase, engine::RunReport,
    support_requirements::SupportElaboration,
};
use serde::Deserialize;

fn invalid() -> Error {
    Error::InvalidInput(
        "Authored report differs from its retained origin; use an explicit argument revision"
            .into(),
    )
}

/// Source metadata stays at its authoring cut. Recovery may append calls and
/// limitations, while changing only support through its own checked transitions.
fn identity(report: &RunReport) -> Result<String, Error> {
    super::digest(&(
        &report.schema_version,
        &report.execution_mode,
        &report.input_sha256,
        &report.provider_config,
        &report.mode,
        &report.run_config,
        &report.request,
        &report.target_runtime,
        &report.authoring_recovery,
    ))
}

pub(super) fn validate(report: &RunReport) -> Result<(), Error> {
    let origin = super::authoring_cut(report)?;
    if identity(report)? != identity(&origin)? {
        return Err(invalid());
    }
    match origin.execution_mode.as_str() {
        "provider_calls" => validate_provider(&origin)?,
        "offline_synthetic_fixture" => {
            if origin.provider_config.is_some()
                || origin.run_config.is_some()
                || origin.authoring_recovery.is_some()
            {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    if origin.transcript.is_empty() {
        return Ok(());
    }
    for call in &origin.transcript {
        if call.role != call.prompt.role
            || call.response_content_omitted
            || call.response_content_bytes != call.response.content.len()
            || call.response_content_sha256
                != crate::authoring_checkpoint::digest(call.response.content.as_bytes())
        {
            return Err(invalid());
        }
    }
    let last = origin
        .transcript
        .last()
        .expect("nonempty authoring history");
    let (mut candidate, authored_support) = parse_final(&last.response.content)?;
    crate::engine::reconcile_retained_argument(
        &origin.request,
        &mut candidate,
        &origin.argument,
        &origin.transcript,
    )?;
    if super::digest(&candidate)? != super::digest(&origin.argument)? {
        return Err(invalid());
    }
    let retained_support = origin.support_elaboration.as_ref().or_else(|| {
        origin
            .evidence_planning
            .as_ref()
            .map(|planning| &planning.elaboration)
    });
    // Provider output defines whether support was authored at this cut. Adding
    // support later is a semantic revision even when the argument is unchanged.
    // Offline fixtures may deliberately supply support outside a simulated reply.
    if origin.execution_mode == "provider_calls" || authored_support.is_some() {
        let authored_digest = authored_support.as_ref().map(super::digest).transpose()?;
        let retained_digest = retained_support.map(super::digest).transpose()?;
        if authored_digest != retained_digest {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_provider(report: &RunReport) -> Result<(), Error> {
    let config = report.run_config.as_ref().ok_or_else(invalid)?;
    let mut allowances = report
        .authoring_recovery
        .as_ref()
        .map(|recovery| recovery.previous_checkpoint.repair_allowances.clone())
        .unwrap_or_default();
    if let Some(recovery) = &report.authoring_recovery {
        allowances.push(crate::authoring_checkpoint::RepairAllowance {
            recorded_calls: recovery.recorded_calls,
            max_calls: recovery.additional_repair_limit,
            max_input_bytes: recovery.stage_byte_limits.max_input_bytes,
            max_response_bytes: recovery.stage_byte_limits.max_response_bytes,
        });
    }
    crate::authoring_checkpoint::validate_call_allowances(config, &report.transcript, &allowances)?;
    if report.mode != config.mode
        || report.input_sha256
            != crate::authoring_checkpoint::digest(serde_json::to_vec(&report.request)?)
    {
        return Err(invalid());
    }
    if report.support_elaboration.is_some()
        && report.evidence_planning.is_none()
        && (report.design_review.is_none() || report.direct_bindings.is_none())
    {
        return Err(invalid());
    }
    let mut checkpoint = Checkpoint::new(
        report.request.clone(),
        config.clone(),
        serde_json::to_value(&report.provider_config)?,
        String::new(),
    );
    checkpoint.transcript = report.transcript.clone();
    checkpoint.validate_transcript()?;
    if checkpoint.inspection()["resumable"] != true {
        return Err(invalid());
    }
    Ok(())
}

/// Current provider protocols produce a plain argument, planned argument, or
/// supported argument. Synthetic retained fixtures also use the argument-only
/// envelope. Every form keeps its actual typed parser and rejects unknown fields.
fn parse_final(content: &str) -> Result<(ArgumentCase, Option<SupportElaboration>), Error> {
    let value = eal_collector_contracts::strict_json_exact(
        content.as_bytes(),
        eal_collector_contracts::JsonLimits {
            max_bytes: crate::authoring_checkpoint::MAX_REPORT_BYTES,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidOutput)?;
    if value.get("support_elaboration_json").is_some() {
        let parsed = crate::support_requirements::parse_supported_argument(
            content,
            crate::authoring_checkpoint::MAX_REPORT_BYTES,
        )?;
        Ok((parsed.argument, Some(parsed.support_elaboration)))
    } else if value.get("acquisition_plan").is_some() {
        let parsed: crate::acquisition::PlannedArgument = serde_json::from_value(value)?;
        Ok((parsed.argument, None))
    } else if value.get("argument").is_some() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ArgumentEnvelope {
            argument: ArgumentCase,
        }
        let parsed: ArgumentEnvelope = serde_json::from_value(value)?;
        Ok((parsed.argument, None))
    } else {
        Ok((serde_json::from_value::<ArgumentCase>(value)?, None))
    }
}
