//! One current authored report contract for generation, revision and packaging.
use crate::{
    Error,
    authoring_checkpoint::Checkpoint,
    direct_planning::DirectPlan,
    engine::{RunReport, SupportRunReport},
    provider::CompletionRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const RUN_REPORT_SCHEMA: &str = "engarg.run.v3";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageByteLimits {
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
}

/// Immutable accounting for the authoring calls restored from a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringRecovery {
    pub schema: String,
    pub parent_checkpoint_sha256: String,
    pub previous_checkpoint: Checkpoint,
    pub previous_call_order: Vec<usize>,
    pub recorded_calls: usize,
    pub new_model_calls: usize,
    pub additional_repair_limit: u8,
    pub stage_byte_limits: StageByteLimits,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub previous_provider_config: Value,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub provider_config: Value,
    pub protocol_migration: bool,
    pub previous_protocol: String,
    pub current_protocol: String,
}

/// Source recovery history remains metadata; current execution uses the typed support fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportRecovery {
    pub schema: String,
    pub input_report_sha256: String,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub previous_report: Value,
    pub recorded_calls: usize,
    pub model_calls: usize,
    pub repair_limit: u8,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub provider_config: Value,
    pub audit_path: String,
    pub provider_requests: Vec<CompletionRequest>,
}

pub fn with_direct(result: SupportRunReport<DirectPlan>) -> Result<RunReport, Error> {
    let mut report = result.report;
    report.support_elaboration = Some(result.evidence_planning.elaboration);
    report.direct_bindings = Some(result.evidence_planning.routes);
    report.design_review = result.evidence_planning.design_review;
    validate(&report)?;
    Ok(report)
}

/// Strict, exact-number admission. Unknown report fields cannot disappear at a consumer boundary.
pub fn read(value: &Value) -> Result<RunReport, Error> {
    if serde_json::to_vec(value)?.len() > crate::authoring_checkpoint::MAX_REPORT_BYTES {
        return Err(Error::Budget("authored report exceeds 16 MiB".into()));
    }
    let report: RunReport = serde_json::from_value(value.clone())?;
    validate(&report)?;
    Ok(report)
}

/// Optional discovery is authoring information. Its operation contracts enter the
/// same direct report and must be rebound to operator TOML before execution.
pub fn read_direct(value: &Value) -> Result<RunReport, Error> {
    let mut report = read(value)?;
    if report.support_elaboration.is_none()
        && let Some(planning) = &report.evidence_planning
    {
        report.support_elaboration = Some(planning.elaboration.clone());
        report.design_review = planning.design_review.clone();
        report.direct_bindings = Some(DirectPlan::unbound(&planning.elaboration));
    }
    if report.support_elaboration.is_none() {
        return Err(Error::InvalidInput(
            "Executable report requires authored support_elaboration; author or explicitly revise its support requirements".into(),
        ));
    }
    validate(&report)?;
    Ok(report)
}

pub fn validate(report: &RunReport) -> Result<(), Error> {
    if report.schema_version != RUN_REPORT_SCHEMA {
        return Err(Error::InvalidInput(format!(
            "Unsupported authored report {}; regenerate using {RUN_REPORT_SCHEMA}",
            report.schema_version
        )));
    }
    if let Some(target) = &report.target_runtime {
        crate::runtime_compatibility::validate_receipt(target)?;
    }
    crate::domain::validate_request(&report.request)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    crate::domain::validate_case(&report.request, &report.argument)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    let original = crate::engine::support::revision::origin(report)?;
    let original_value = serde_json::to_value(&original)?;
    crate::authoring_checkpoint::validate_recovery_report(&original_value)?;
    super::validate_history(&original_value)?;
    super::authorship::validate(&original)?;
    if let Some(planning) = &report.evidence_planning {
        if planning.schema != "engarg.support-planning/1"
            || planning.routes.schema != crate::route_planning::ROUTE_PLAN_SCHEMA
        {
            return Err(Error::InvalidInput(
                "Unsupported discovery planning schema".into(),
            ));
        }
        if let Some(current) = &report.support_elaboration
            && super::digest(current)? != super::digest(&planning.elaboration)?
        {
            return Err(Error::InvalidInput(
                "Current support differs from retained discovery planning; revise explicitly"
                    .into(),
            ));
        }
    }
    let support = report.support_elaboration.as_ref().or_else(|| {
        report
            .evidence_planning
            .as_ref()
            .map(|planning| &planning.elaboration)
    });
    if let Some(support) = support {
        let errors = crate::support_requirements::validate_elaboration(&report.argument, support);
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
        for review in report.design_review.iter().chain(
            report
                .evidence_planning
                .as_ref()
                .and_then(|planning| planning.design_review.as_ref()),
        ) {
            let expected = crate::support_requirements::review_acquisition_design(
                support,
                &report.transcript,
                crate::authoring_checkpoint::MAX_BYTES,
            )?;
            if super::digest(review)? != super::digest(&expected)? {
                return Err(Error::InvalidInput(
                    "Acquisition design review differs from retained authoring and current support"
                        .into(),
                ));
            }
        }
        if let Some(bindings) = &report.direct_bindings {
            validate_bindings(bindings, support)?;
        }
    } else if report.direct_bindings.is_some() || report.design_review.is_some() {
        return Err(Error::InvalidInput(
            "Direct binding metadata requires support_elaboration".into(),
        ));
    }
    Ok(())
}

fn validate_bindings(
    bindings: &DirectPlan,
    support: &crate::support_requirements::SupportElaboration,
) -> Result<(), Error> {
    let expected: BTreeSet<_> = support.acquisitions.iter().map(|a| a.id.as_str()).collect();
    let bound: BTreeSet<_> = bindings.bindings.keys().map(String::as_str).collect();
    let unbound: BTreeSet<_> = bindings
        .unbound_operations
        .iter()
        .map(String::as_str)
        .collect();
    if bindings.schema != "engarg.direct-bindings/1"
        || !eal_collector_contracts::execution::valid_digest(&bindings.manifest_sha256)
        || unbound.len() != bindings.unbound_operations.len()
        || !bound.is_disjoint(&unbound)
        || bound.union(&unbound).copied().collect::<BTreeSet<_>>() != expected
        || bindings
            .bindings
            .iter()
            .any(|(id, name)| name != id && name != &crate::support_export::operation_tool_name(id))
    {
        return Err(Error::InvalidInput(
            "Direct binding metadata does not cover exactly the authored operations".into(),
        ));
    }
    Ok(())
}
