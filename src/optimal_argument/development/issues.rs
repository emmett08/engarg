//! Preserve review obligations until exact investigator-owned evidence resolves them.
use super::{
    CandidateInvestigator, CheckedIssueResolution, CriticalIssue, DevelopmentCandidate, IssueKind,
    IssueResolutionKind, RegisteredIssue, candidate_digest, digest, issue_digest,
};
use crate::Error;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct IssueLedger {
    issues: BTreeMap<String, RegisteredIssue>,
}

impl IssueLedger {
    pub fn register(&mut self, candidate: usize, issues: &[CriticalIssue]) -> Result<(), Error> {
        for issue in issues {
            let hash = issue_digest(issue)?;
            let entry = self
                .issues
                .entry(hash.clone())
                .or_insert_with(|| RegisteredIssue {
                    issue_sha256: hash,
                    issue: issue.clone(),
                    raised_by_candidates: vec![],
                });
            if !entry.raised_by_candidates.contains(&candidate) {
                entry.raised_by_candidates.push(candidate);
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> Vec<RegisteredIssue> {
        self.issues.values().cloned().collect()
    }

    /// A reformulation does not itself establish resolution of an earlier issue.
    pub async fn reconcile<I: CandidateInvestigator, O: Fn(&Value) -> Result<(), Error> + Sync>(
        &self,
        candidate: &mut DevelopmentCandidate,
        investigator: &I,
        observer: &O,
    ) -> Result<(), Error> {
        let mut resolved: BTreeSet<_> = candidate
            .issue_resolutions
            .iter()
            .filter(|resolution| {
                self.issues
                    .get(&resolution.issue_sha256)
                    .is_some_and(|entry| {
                        valid_resolution(candidate, &entry.issue, resolution).unwrap_or(false)
                    })
            })
            .map(|resolution| resolution.issue_sha256.clone())
            .collect();
        let outstanding: Vec<_> = self
            .issues
            .values()
            .filter(|entry| !resolved.contains(entry.issue_sha256.as_str()))
            .map(|entry| entry.issue.clone())
            .collect();
        candidate.pending_critical_issues = outstanding.clone();
        candidate.eligible = false;
        if !outstanding.is_empty() && candidate.outcome.report.is_some() {
            let resolutions = investigator
                .resolve_issues(&candidate.outcome, &outstanding)
                .await?;
            if resolutions.len() > outstanding.len() {
                candidate.diagnostics.push("Investigator returned more issue resolutions than pending obligations; none were admitted".into());
            } else {
                for resolution in resolutions {
                    let entry = self.issues.get(&resolution.issue_sha256);
                    let valid = entry.is_some_and(|entry| {
                        valid_resolution(candidate, &entry.issue, &resolution).unwrap_or(false)
                    });
                    let duplicate = candidate
                        .issue_resolutions
                        .iter()
                        .any(|prior| prior.issue_sha256 == resolution.issue_sha256);
                    if valid && !duplicate {
                        observer(
                            &json!({"event":"issue_resolved","candidate_index":candidate.index,"resolution":resolution}),
                        )?;
                        resolved.insert(resolution.issue_sha256.clone());
                        candidate.issue_resolutions.push(resolution);
                    } else {
                        observer(
                            &json!({"event":"issue_resolution_rejected","candidate_index":candidate.index,"resolution":resolution}),
                        )?;
                        candidate.diagnostics.push("An issue resolution lacked the exact issue, admitted candidate, verification receipt or permitted check evidence".into());
                    }
                }
            }
        }
        candidate.pending_critical_issues = self
            .issues
            .values()
            .filter(|entry| !resolved.contains(entry.issue_sha256.as_str()))
            .map(|entry| entry.issue.clone())
            .collect();
        candidate.eligible = candidate.outcome.assessment.supported
            && candidate.outcome.report.is_some()
            && candidate.pending_critical_issues.is_empty()
            && (candidate.parent_index.is_none() || candidate.criticism.is_some());
        if !candidate.pending_critical_issues.is_empty() {
            let diagnosis = "Critical issues remain pending until exact investigator-owned evidence resolves them; reformulation and model omission do not discharge them";
            if !candidate.diagnostics.iter().any(|prior| prior == diagnosis) {
                candidate.diagnostics.push(diagnosis.into());
            }
            observer(
                &json!({"event":"issue_veto","candidate_index":candidate.index,
                "pending_critical_issues":candidate.pending_critical_issues}),
            )?;
        }
        Ok(())
    }
}

fn valid_resolution(
    candidate: &DevelopmentCandidate,
    issue: &CriticalIssue,
    resolution: &CheckedIssueResolution,
) -> Result<bool, Error> {
    let Some(report) = candidate.outcome.report.as_ref() else {
        return Ok(false);
    };
    let kind_matches = matches!(
        (issue.kind, resolution.kind),
        (
            IssueKind::MissingEvidence,
            IssueResolutionKind::ObservedEvidence
        ) | (
            IssueKind::MissingVerification,
            IssueResolutionKind::PassedVerification
        ) | (
            IssueKind::LifecycleConflict,
            IssueResolutionKind::CompletedLifecycle
        ) | (
            IssueKind::BindingMismatch,
            IssueResolutionKind::MatchedExecutionBinding
        )
    );
    let identified = match issue.kind {
        IssueKind::MissingEvidence => !issue.check_ids.is_empty(),
        IssueKind::MissingVerification => {
            !issue.check_ids.is_empty() || !issue.obligation_ids.is_empty()
        }
        IssueKind::LifecycleConflict | IssueKind::BindingMismatch => !issue.artifacts.is_empty(),
        IssueKind::Semantic | IssueKind::Other => false,
    };
    let receipt_path = format!(
        "{}/verification.json",
        candidate.outcome.assessment.artifact
    );
    let receipt_bound = resolution.artifacts.iter().any(|artifact| {
        artifact.path == receipt_path
            && artifact.sha256.as_deref() == Some(resolution.verification_sha256.as_str())
    });
    let artifacts_bound = receipt_bound
        && resolution.artifacts.iter().all(|artifact| {
            super::critic::valid_artifact(artifact)
                && artifact
                    .sha256
                    .as_deref()
                    .is_some_and(eal_collector_contracts::execution::valid_digest)
        });
    Ok(kind_matches
        && identified
        && artifacts_bound
        && resolution.check_ids.len() <= 128
        && issue
            .check_ids
            .iter()
            .all(|id| resolution.check_ids.contains(id))
        && resolution.issue_sha256 == issue_digest(issue)?
        && resolution.candidate_sha256 == candidate_digest(report)?
        && resolution.verification_sha256 == digest(&candidate.outcome.verification)?)
}
