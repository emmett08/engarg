//! Classify admitted candidates by independently checked support and selected size.
use super::{CandidateOutcome, DevelopmentCandidate, candidate_digest, digest};
use crate::domain::ArgumentIndex;
use std::cmp::Ordering;

pub(super) fn candidate(
    index: usize,
    parent_index: Option<usize>,
    round: u8,
    mut outcome: CandidateOutcome,
) -> DevelopmentCandidate {
    // A rejected proposal can have no admitted report. Retain the actual
    // investigator diagnosis so the next round can repair it from its receipt.
    let mut diagnostics = if outcome.assessment.supported {
        Vec::new()
    } else {
        outcome.assessment.reasons.clone()
    };
    let (length, argument_sha256) = if let Some(report) = &outcome.report {
        match crate::report::validate(report).and_then(|_| digest(&report.argument)) {
            Ok(hash) => (
                Some(
                    ArgumentIndex::new(&report.argument)
                        .selected_dependency("current_claim")
                        .len(),
                ),
                Some(hash),
            ),
            Err(error) => {
                diagnostics.push(error.to_string());
                outcome.assessment.supported = false;
                outcome.assessment.status = "invalid_report".into();
                outcome.report = None;
                (None, None)
            }
        }
    } else {
        (None, None)
    };
    let proposal_sha256 = if let Some(report) = &outcome.report {
        match candidate_digest(report) {
            Ok(hash) => Some(hash),
            Err(error) => {
                diagnostics.push(error.to_string());
                outcome.assessment.supported = false;
                outcome.assessment.status = "invalid_proposal_identity".into();
                None
            }
        }
    } else {
        None
    };
    let investigation_sha256 = if let Some(report) = &outcome.report {
        match digest(&(
            &report.argument,
            &report.support_elaboration,
            &outcome.verification,
        )) {
            Ok(hash) => Some(hash),
            Err(error) => {
                diagnostics.push(error.to_string());
                outcome.assessment.supported = false;
                outcome.assessment.status = "invalid_verification".into();
                None
            }
        }
    } else {
        None
    };
    let eligible = parent_index.is_none() && length.is_some() && outcome.assessment.supported;
    DevelopmentCandidate {
        index,
        parent_index,
        round,
        length,
        argument_sha256,
        proposal_sha256,
        investigation_sha256,
        rejected_proposal: None,
        outcome,
        criticism: None,
        pending_critical_issues: Vec::new(),
        issue_resolutions: Vec::new(),
        eligible,
        diagnostics,
    }
}

pub(super) fn compare(left: &DevelopmentCandidate, right: &DevelopmentCandidate) -> Ordering {
    right
        .eligible
        .cmp(&left.eligible)
        .then_with(|| {
            right
                .outcome
                .assessment
                .supported
                .cmp(&left.outcome.assessment.supported)
        })
        .then_with(|| {
            left.length
                .unwrap_or(usize::MAX)
                .cmp(&right.length.unwrap_or(usize::MAX))
        })
        .then_with(|| left.argument_sha256.cmp(&right.argument_sha256))
        .then_with(|| left.index.cmp(&right.index))
}
