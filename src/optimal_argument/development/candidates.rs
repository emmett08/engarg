//! Classify checked candidates and preserve unresolved critical issues across repeats.
use super::{CandidateOutcome, Criticism, DevelopmentCandidate, digest};
use crate::domain::ArgumentIndex;
use std::{cmp::Ordering, collections::BTreeSet};

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
        match digest(&(&report.argument, &report.support_elaboration)) {
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

pub(super) fn apply_criticism(
    proposed: &mut DevelopmentCandidate,
    criticism: Criticism,
    previous: &mut [DevelopmentCandidate],
) -> Vec<usize> {
    let same_proposal = |prior: &DevelopmentCandidate| {
        proposed.proposal_sha256.is_some() && prior.proposal_sha256 == proposed.proposal_sha256
    };
    // Findings concern the argument and support, including earlier instances.
    // Receipt clocks, new reviewer wording and later omission cannot discharge
    // a finding. Until explicit checked issue resolutions exist, require a
    // changed proposal before reconsideration.
    proposed.pending_critical_issues = previous
        .iter()
        .filter(|prior| same_proposal(prior))
        .flat_map(|prior| prior.pending_critical_issues.iter().cloned())
        .chain(criticism.critical_issues.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut vetoed = Vec::new();
    if !proposed.pending_critical_issues.is_empty() {
        for prior in previous.iter_mut().filter(|prior| same_proposal(prior)) {
            prior.pending_critical_issues = proposed.pending_critical_issues.clone();
            prior.eligible = false;
            let diagnosis = "A critical finding against this identical argument and support proposal remains unresolved";
            if !prior.diagnostics.iter().any(|value| value == diagnosis) {
                prior.diagnostics.push(diagnosis.into());
            }
            vetoed.push(prior.index);
        }
        vetoed.push(proposed.index);
    }
    proposed.eligible =
        proposed.outcome.assessment.supported && proposed.pending_critical_issues.is_empty();
    proposed
        .diagnostics
        .extend(criticism.critical_issues.iter().cloned());
    if criticism.critical_issues.is_empty() && !proposed.pending_critical_issues.is_empty() {
        proposed.diagnostics.push("Earlier critical issues remain unresolved for this identical argument and support proposal; refreshed verification metadata does not discharge them".into());
    }
    proposed.criticism = Some(criticism);
    vetoed
}
