//! Typed review findings and investigator-owned evidence for their resolution.
use crate::{Error, engine::RunReport};
use serde::{Deserialize, Serialize};

/// Only narrowly stated claims about observed check state may be resolved by
/// existing machine evidence. Meaning adequacy remains a substantive issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    MissingEvidence,
    MissingVerification,
    LifecycleConflict,
    BindingMismatch,
    Semantic,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub path: String,
    pub pointer: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriticalIssue {
    pub id: String,
    pub kind: IssueKind,
    pub target_id: String,
    pub description: String,
    pub obligation_ids: Vec<String>,
    pub check_ids: Vec<String>,
    pub artifacts: Vec<ArtifactReference>,
}

impl CriticalIssue {
    fn legacy(description: String) -> Self {
        let hash = crate::authoring_checkpoint::digest(description.as_bytes());
        Self {
            id: format!("legacy_{}", &hash[..16]),
            kind: IssueKind::Other,
            target_id: "current_claim".into(),
            description,
            obligation_ids: vec![],
            check_ids: vec![],
            artifacts: vec![],
        }
    }
}

/// Reading an older retained result never treats an untyped finding as resolved.
pub(super) fn deserialize_issues<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<CriticalIssue>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StoredIssue {
        Typed(CriticalIssue),
        Legacy(String),
    }
    Ok(Vec::<StoredIssue>::deserialize(decoder)?
        .into_iter()
        .map(|issue| match issue {
            StoredIssue::Typed(issue) => issue,
            StoredIssue::Legacy(description) => CriticalIssue::legacy(description),
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueResolutionKind {
    ObservedEvidence,
    PassedVerification,
    CompletedLifecycle,
    MatchedExecutionBinding,
}

/// Produced by the trusted investigator, never by a model's approval or omission.
/// All identities use canonical structured-value SHA-256, not pretty-file bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckedIssueResolution {
    pub issue_sha256: String,
    pub candidate_sha256: String,
    pub verification_sha256: String,
    pub kind: IssueResolutionKind,
    pub check_ids: Vec<String>,
    pub artifacts: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredIssue {
    pub issue_sha256: String,
    pub issue: CriticalIssue,
    pub raised_by_candidates: Vec<usize>,
}

pub fn issue_digest(issue: &CriticalIssue) -> Result<String, Error> {
    super::digest(issue)
}

/// Identity of the admitted argument and complete support, excluding clocks.
pub fn candidate_digest(report: &RunReport) -> Result<String, Error> {
    super::digest(&(&report.argument, &report.support_elaboration))
}
