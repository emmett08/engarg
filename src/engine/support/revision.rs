//! Explicit argument revisions preserve their source and invalidate derived work.

use crate::{
    Error,
    domain::{ArgumentCase, Request},
    engine::{RunReport, retained::reconstruct_retained},
    output_validation::{self, RetainedObjection},
    provider::Role,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentRevision {
    pub schema: String,
    pub parent_report_sha256: String,
    pub request: Request,
    pub previous_argument: ArgumentCase,
    pub argument: ArgumentCase,
    pub invalidated: Vec<String>,
}

fn retained(report: &RunReport) -> Result<Vec<RetainedObjection>, Error> {
    crate::domain::validate_case(&report.request, &report.argument)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    if report.argument.dissent_revisions.is_some() && report.transcript.is_empty() {
        return Err(Error::InvalidInput(
            "revision of retained dissent origins requires the source transcript".into(),
        ));
    }
    let mut retained = reconstruct_retained(&report.transcript)?;
    if !retained.is_empty() {
        let errors = output_validation::errors(&report.request, &report.argument, &retained, None);
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
    }
    output_validation::retain_objections(&report.argument, Role::Adjudicator, &mut retained);
    Ok(retained)
}

/// Compute the exact change contexts needed to author dissent applications.
/// This is inspection only: it grants no evidential status to a proposed edit.
pub fn preview(report: &RunReport, candidate: &ArgumentCase) -> Result<Value, Error> {
    let retained = retained(report)?;
    Ok(json!({
        "parent_report_sha256": eal_collector_contracts::execution::canonical_digest(report)
            .map_err(Error::InvalidInput)?,
        "dissent_revision_candidates": output_validation::dissent_changes(candidate, &retained),
    }))
}

/// Accept an explicit complete revision without calling a model or reusing old
/// executable plans. Original observations remain in their original run.
pub fn revise(report: &RunReport, mut candidate: ArgumentCase) -> Result<ArgumentRevision, Error> {
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
    Ok(ArgumentRevision {
        schema: "engarg.argument-revision/1".into(),
        parent_report_sha256: eal_collector_contracts::execution::canonical_digest(report)
            .map_err(Error::InvalidInput)?,
        request: report.request.clone(),
        previous_argument: report.argument.clone(),
        argument: candidate,
        invalidated: [
            "support_elaboration",
            "route_plan",
            "eal_export",
            "collection",
            "assessment",
        ]
        .map(str::to_owned)
        .into(),
    })
}
