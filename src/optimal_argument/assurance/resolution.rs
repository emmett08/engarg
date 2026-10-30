//! Resolve only typed claims about check state using the investigator's receipts.
use super::*;
use crate::optimal_argument::development::{
    ArtifactReference, CandidateOutcome, CheckedIssueResolution, CriticalIssue, IssueKind,
    IssueResolutionKind, candidate_digest, issue_digest,
};

pub fn resolve(
    outcome: &CandidateOutcome,
    issues: &[CriticalIssue],
) -> Result<Vec<CheckedIssueResolution>, Error> {
    let Some(report) = &outcome.report else {
        return Ok(vec![]);
    };
    let Ok(receipt) = serde_json::from_value::<AssuranceReceipt>(outcome.verification.clone())
    else {
        return Ok(vec![]);
    };
    let Some(bindings) = &receipt.lifecycle.bindings else {
        return Ok(vec![]);
    };
    let candidate_sha256 = candidate_digest(report)?;
    if receipt.schema != RECEIPT_SCHEMA
        || !receipt.passed()
        || !outcome.assessment.supported
        || receipt.lifecycle.phase != "post_execution"
        || receipt.lifecycle.runtime_status.as_deref() != Some("supported")
        || !bindings.replay_checked
        || !bindings.normalization_checked
        || bindings.admitted_candidate_sha256 != candidate_sha256
        || bindings.admitted_report_sha256 != digest(report)?
        || receipt.candidate_sha256 != bindings.raw_candidate_sha256
    {
        return Ok(vec![]);
    }
    let verification_sha256 = digest(&outcome.verification)?;
    let mut resolutions = Vec::new();
    for issue in issues {
        let checks: Option<Vec<_>> = issue
            .check_ids
            .iter()
            .map(|id| {
                receipt.checks.iter().find(|c| {
                    &c.id == id && c.target_id == issue.target_id && c.status == CheckStatus::Pass
                })
            })
            .collect();
        let Some(checks) = checks else {
            continue;
        };
        let obligations_pass = issue.obligation_ids.iter().all(|id| {
            receipt
                .verification_reports
                .iter()
                .filter(|r| r.target_id == issue.target_id)
                .flat_map(|r| &r.obligations)
                .any(|o| &o.id == id && o.status == eal_core::verification::Status::Pass)
        });
        if !obligations_pass {
            continue;
        }
        // Unsupported or stale artifact assertions require an actual new
        // investigation; a broad lifecycle label cannot erase them.
        let artifact_name = |path: &str| match path {
            "report.json" => Some("report.json"),
            "verification.json" => Some("verification.json"),
            path if path
                == format!(
                    "{}/report.json",
                    outcome.assessment.artifact.trim_end_matches('/')
                ) =>
            {
                Some("report.json")
            }
            path if path
                == format!(
                    "{}/verification.json",
                    outcome.assessment.artifact.trim_end_matches('/')
                ) =>
            {
                Some("verification.json")
            }
            _ => None,
        };
        let mut identifies_lifecycle = false;
        let mut identifies_binding = false;
        let artifacts_match = issue.artifacts.iter().all(|a| {
            let name = artifact_name(&a.path);
            let expected = match name {
                Some("report.json") => Some(bindings.admitted_report_sha256.as_str()),
                Some("verification.json") => Some(verification_sha256.as_str()),
                _ => None,
            };
            let lifecycle_pointer = matches!(
                a.pointer.as_deref(),
                Some("/lifecycle" | "/lifecycle/phase" | "/lifecycle/bindings")
            );
            let whole_artifact = a.pointer.as_deref().is_none_or(str::is_empty);
            if name == Some("verification.json") && lifecycle_pointer {
                identifies_lifecycle = true;
                identifies_binding = true;
            }
            if name == Some("report.json") && whole_artifact {
                identifies_binding = true;
            }
            let pointer_matches = whole_artifact
                || (matches!(
                    issue.kind,
                    IssueKind::LifecycleConflict | IssueKind::BindingMismatch
                ) && name == Some("verification.json")
                    && lifecycle_pointer);
            expected.is_some()
                && a.sha256.as_deref().is_none_or(|d| Some(d) == expected)
                && pointer_matches
        });
        if !artifacts_match {
            continue;
        }
        let kind = match issue.kind {
            IssueKind::MissingEvidence
                if !checks.is_empty()
                    && checks.iter().all(|c| {
                        c.details["record_status"] == "ok"
                            && c.details["native_evidence"]["status"] == "available"
                            && c.details
                                .get("evaluation_input")
                                .is_some_and(|v| !v.is_null())
                    }) =>
            {
                IssueResolutionKind::ObservedEvidence
            }
            IssueKind::MissingVerification
                if !checks.is_empty() || !issue.obligation_ids.is_empty() =>
            {
                IssueResolutionKind::PassedVerification
            }
            IssueKind::LifecycleConflict
                if identifies_lifecycle
                    && receipt.lifecycle.phase == "post_execution"
                    && receipt.lifecycle.runtime_status.as_deref() == Some("supported") =>
            {
                IssueResolutionKind::CompletedLifecycle
            }
            IssueKind::BindingMismatch
                if identifies_binding
                    && !bindings.source_digest.is_empty()
                    && !bindings.executable_digest.is_empty()
                    && !bindings.collection_id.is_empty()
                    && !bindings.assessment_id.is_empty() =>
            {
                IssueResolutionKind::MatchedExecutionBinding
            }
            _ => continue,
        };
        if crate::domain::ArgumentIndex::new(&report.argument)
            .node(&issue.target_id)
            .is_none()
        {
            continue;
        }
        resolutions.push(CheckedIssueResolution {
            issue_sha256: issue_digest(issue)?,
            candidate_sha256: candidate_sha256.clone(),
            verification_sha256: verification_sha256.clone(),
            kind,
            check_ids: issue.check_ids.clone(),
            artifacts: vec![
                ArtifactReference {
                    path: format!(
                        "{}/verification.json",
                        outcome.assessment.artifact.trim_end_matches('/')
                    ),
                    pointer: None,
                    sha256: Some(verification_sha256.clone()),
                },
                ArtifactReference {
                    path: format!(
                        "{}/report.json",
                        outcome.assessment.artifact.trim_end_matches('/')
                    ),
                    pointer: None,
                    sha256: Some(bindings.admitted_report_sha256.clone()),
                },
            ],
        });
    }
    Ok(resolutions)
}
