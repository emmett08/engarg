//! Explicit argument revisions retain their authored source and rebuild support dependencies.
use crate::{
    Error,
    direct_planning::DirectPlan,
    domain::ArgumentCase,
    engine::{RunReport, retained::reconstruct_retained},
    output_validation::{self, RetainedObjection},
    provider::Role,
    support_requirements::SupportElaboration,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MAX_REVISIONS: usize = 16;
const REVISION_SCHEMA: &str = "engarg.argument-revision/2";
const INVALIDATED: &[&str] = &[
    "direct_bindings",
    "tooling",
    "eal_export",
    "collection",
    "assessment",
];

/// Each source snapshot omits the preceding history, which is stored once in the
/// enclosing report. Restoring that prefix reconstructs the exact parent report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionRecord {
    pub schema: String,
    pub parent_report_sha256: String,
    pub previous_report: Box<RunReport>,
    pub argument_sha256: String,
    pub support_elaboration_sha256: Option<String>,
    pub invalidated: Vec<String>,
}

fn digest(value: &impl Serialize) -> Result<String, Error> {
    eal_collector_contracts::execution::canonical_digest(value).map_err(Error::InvalidInput)
}

fn authoring_identity(report: &RunReport) -> Result<String, Error> {
    digest(&(
        &report.schema_version,
        &report.execution_mode,
        &report.input_sha256,
        &report.provider_config,
        &report.mode,
        &report.run_config,
        &report.request,
        &report.transcript,
        &report.limitations,
        &report.target_runtime,
        &report.authoring_recovery,
    ))
}

fn retained(report: &RunReport) -> Result<Vec<RetainedObjection>, Error> {
    crate::domain::validate_case(&report.request, &report.argument)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    if report.argument.dissent_revisions.is_some()
        && report.transcript.is_empty()
        && report.argument_revisions.is_empty()
    {
        return Err(Error::InvalidInput(
            "revision of retained dissent origins requires the source transcript or checked revision history".into(),
        ));
    }
    let mut retained = reconstruct_retained(&report.transcript)?;
    for record in &report.argument_revisions {
        output_validation::retain_objections(
            &record.previous_report.argument,
            Role::Adjudicator,
            &mut retained,
        );
    }
    output_validation::retain_objections(&report.argument, Role::Adjudicator, &mut retained);
    Ok(retained)
}

fn checked_candidate(
    report: &RunReport,
    mut candidate: ArgumentCase,
) -> Result<ArgumentCase, Error> {
    let retained = retained(report)?;
    let errors = output_validation::errors(
        &report.request,
        &candidate,
        &retained,
        report
            .run_config
            .as_ref()
            .and_then(|config| config.max_argument_depth),
    );
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    output_validation::install_history(&mut candidate, &retained);
    Ok(candidate)
}

/// Verify every semantic transition and source digest without accepting current
/// binding/tooling metadata as evidence that the revised claim is supported.
pub fn origin(report: &RunReport) -> Result<RunReport, Error> {
    if report.argument_revisions.len() > MAX_REVISIONS {
        return Err(Error::Budget(
            "argument revision history exceeds sixteen revisions".into(),
        ));
    }
    let mut current = report.clone();
    while let Some(record) = current.argument_revisions.pop() {
        if record.schema != REVISION_SCHEMA
            || !record.previous_report.argument_revisions.is_empty()
            || record.argument_sha256 != digest(&current.argument)?
            || record.support_elaboration_sha256
                != current
                    .support_elaboration
                    .as_ref()
                    .map(digest)
                    .transpose()?
            || record.invalidated
                != INVALIDATED
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect::<Vec<_>>()
        {
            return Err(Error::InvalidInput(
                "argument revision identity or invalidation record changed".into(),
            ));
        }
        let mut parent = *record.previous_report;
        parent.argument_revisions = std::mem::take(&mut current.argument_revisions);
        if digest(&parent)? != record.parent_report_sha256
            || authoring_identity(&parent)? != authoring_identity(&current)?
            || digest(&checked_candidate(&parent, current.argument.clone())?)?
                != digest(&current.argument)?
        {
            return Err(Error::InvalidInput(
                "argument revision changed its retained parent or dissent".into(),
            ));
        }
        validate_support_change(
            &parent,
            &current.argument,
            current.support_elaboration.as_ref(),
        )?;
        current = parent;
    }
    Ok(current)
}

/// Exact change contexts needed to author explicit dissent dispositions.
pub fn preview(report: &RunReport, candidate: &ArgumentCase) -> Result<Value, Error> {
    origin(report)?;
    let retained = retained(report)?;
    Ok(json!({
        "parent_report_sha256": digest(report)?,
        "dissent_revision_candidates": output_validation::dissent_changes(candidate, &retained),
        "support_elaboration_required": digest(candidate)? != digest(&report.argument)? && report.support_elaboration.is_some(),
    }))
}

fn validate_support_change(
    report: &RunReport,
    argument: &ArgumentCase,
    support: Option<&SupportElaboration>,
) -> Result<(), Error> {
    match (&report.support_elaboration, support) {
        (Some(previous), Some(current)) => {
            // An unchanged accepted support graph retains its historical revision
            // annotations. Revalidate it against the new argument without treating
            // those old annotations as fresh changes to the immediate parent.
            let errors = if digest(previous)? == digest(current)? {
                crate::support_requirements::validate_elaboration(argument, current)
            } else {
                crate::support_requirements::validate_argument_revision(
                    &report.argument,
                    previous,
                    argument,
                    current,
                )
            };
            if !errors.is_empty() {
                return Err(Error::InvalidInput(errors.join("; ")));
            }
        }
        (None, Some(current)) => {
            let errors = crate::support_requirements::validate_elaboration(argument, current);
            if !errors.is_empty() {
                return Err(Error::InvalidInput(errors.join("; ")));
            }
        }
        (Some(_), None) => {
            return Err(Error::InvalidInput(
                "revision cannot discard executable support requirements".into(),
            ));
        }
        (None, None) => {}
    }
    Ok(())
}

/// Revision of argument-only reports. Executable reports require an explicitly
/// supplied support elaboration whenever their argument changes.
pub fn revise(report: &RunReport, candidate: ArgumentCase) -> Result<RunReport, Error> {
    revise_with_support(report, candidate, None, None)
}

pub fn revise_with_support(
    report: &RunReport,
    candidate: ArgumentCase,
    support: Option<SupportElaboration>,
    bindings: Option<DirectPlan>,
) -> Result<RunReport, Error> {
    crate::report::validate(report)?;
    if report.argument_revisions.len() >= MAX_REVISIONS {
        return Err(Error::Budget(
            "argument revision history already contains sixteen revisions".into(),
        ));
    }
    let changed = digest(&candidate)? != digest(&report.argument)?;
    if changed && report.support_elaboration.is_some() && support.is_none() {
        return Err(Error::InvalidInput(
            "Changed executable arguments require --support-elaboration with the complete revised support requirements; old dependencies cannot be reused implicitly".into(),
        ));
    }
    let candidate = checked_candidate(report, candidate)?;
    let support = support.or_else(|| report.support_elaboration.clone());
    validate_support_change(report, &candidate, support.as_ref())?;
    let mut updated = report.clone();
    updated.argument = candidate;
    updated.support_elaboration = support;
    updated.direct_bindings = updated
        .support_elaboration
        .as_ref()
        .map(|support| bindings.unwrap_or_else(|| DirectPlan::unbound(support)));
    updated.design_review = updated
        .support_elaboration
        .as_ref()
        .map(|support| {
            crate::support_requirements::review_acquisition_design(
                support,
                &updated.transcript,
                crate::authoring_checkpoint::MAX_BYTES,
            )
        })
        .transpose()?;
    updated.tooling = None;
    updated.evidence_planning = None;
    updated.replanning = None;
    updated.support_recovery = None;
    let mut previous = report.clone();
    previous.argument_revisions.clear();
    updated.argument_revisions.push(RevisionRecord {
        schema: REVISION_SCHEMA.into(),
        parent_report_sha256: digest(report)?,
        previous_report: Box::new(previous),
        argument_sha256: digest(&updated.argument)?,
        support_elaboration_sha256: updated
            .support_elaboration
            .as_ref()
            .map(digest)
            .transpose()?,
        invalidated: INVALIDATED.iter().map(|s| (*s).to_owned()).collect(),
    });
    crate::report::validate(&updated)?;
    if serde_json::to_vec(&updated)?.len() > crate::authoring_checkpoint::MAX_REPORT_BYTES {
        return Err(Error::Budget(
            "revised authored report exceeds 16 MiB".into(),
        ));
    }
    Ok(updated)
}
