//! Versioned records for bounded argument development and its independent checks.
use super::{CheckedIssueResolution, CriticalIssue, RegisteredIssue};
use crate::{Error, engine::RunReport, provider::Role, support_requirements::SupportedArgument};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentConfig {
    pub rounds: u8,
    pub max_calls: u16,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub beam_width: usize,
}

impl Default for DevelopmentConfig {
    fn default() -> Self {
        Self {
            rounds: 3,
            max_calls: 6,
            max_input_bytes: 1_048_576,
            max_response_bytes: 1_048_576,
            beam_width: 4,
        }
    }
}

/// The investigator owns immutable target/condition/discharge guards, explicit
/// revision admission, tool authority and actual native/evidence execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateOutcome {
    /// The execution adapter retains the actual report under its artifact path.
    /// Keep it in memory for development without duplicating its complete
    /// revision history in every result/audit summary.
    #[serde(skip_serializing)]
    pub report: Option<RunReport>,
    pub assessment: super::super::Assessment,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub verification: Value,
}

/// An application-owned verification boundary, not an LLM's declared verdict.
///
/// Set `assessment.supported` only when every mandatory assurance check and the
/// native evidence assessment pass. `verification` supplies the safe, inspectable
/// results and counterexamples for subsequent authoring; it must exclude
/// credentials, transport configuration and unrelated confidential material.
pub trait CandidateInvestigator: Send + Sync {
    fn assess(
        &self,
        parent: &RunReport,
        proposal: SupportedArgument,
        index: usize,
    ) -> impl Future<Output = Result<CandidateOutcome, Error>> + Send;

    /// Resolve only the exact typed issue with actual operator-owned checks and
    /// receipt/binding evidence. The default preserves every finding as pending.
    fn resolve_issues(
        &self,
        _outcome: &CandidateOutcome,
        _issues: &[CriticalIssue],
    ) -> impl Future<Output = Result<Vec<CheckedIssueResolution>, Error>> + Send {
        async { Ok(vec![]) }
    }
}

/// A separate sceptic can expose unresolved problems but supplies no proof or
/// acquired empirical observation merely by agreeing with another model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criticism {
    #[serde(deserialize_with = "super::issue_types::deserialize_issues")]
    pub critical_issues: Vec<CriticalIssue>,
    pub observations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentCandidate {
    pub index: usize,
    pub parent_index: Option<usize>,
    pub round: u8,
    pub length: Option<usize>,
    pub argument_sha256: Option<String>,
    /// Stable semantic proposal identity for unresolved sceptic findings.
    /// Observation clocks and other receipt metadata cannot clear a finding.
    pub proposal_sha256: Option<String>,
    pub investigation_sha256: Option<String>,
    /// Retain rejected parsed authoring data for diagnosis-driven repair only.
    /// It is never an admitted report, execution authority or beam parent. The
    /// full proposed value is already retained in the separate audit/artifacts.
    #[serde(skip)]
    pub rejected_proposal: Option<SupportedArgument>,
    pub outcome: CandidateOutcome,
    pub criticism: Option<Criticism>,
    #[serde(deserialize_with = "super::issue_types::deserialize_issues")]
    pub pending_critical_issues: Vec<CriticalIssue>,
    #[serde(default)]
    pub issue_resolutions: Vec<CheckedIssueResolution>,
    pub eligible: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentFailure {
    pub round: u8,
    pub parent_index: usize,
    pub call_index: Option<u16>,
    pub phase: String,
    pub reason: String,
}

/// Full exchanges are retained through the observer before parsing. This small
/// summary supplies accounting without duplicating every prompt in the result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentCall {
    pub index: u16,
    pub round: u8,
    pub parent_index: usize,
    pub role: Role,
    pub phase: String,
    pub completed: bool,
    pub response_content_bytes: Option<usize>,
    pub response_content_sha256: Option<String>,
    pub response_content_omitted: bool,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentResult {
    pub schema: String,
    pub config: DevelopmentConfig,
    pub calls_attempted: u16,
    pub calls: Vec<DevelopmentCall>,
    pub rounds_completed: u8,
    pub budget_exhausted: bool,
    pub stop_reason: String,
    pub pruned_states: usize,
    pub best: Option<usize>,
    pub candidates: Vec<DevelopmentCandidate>,
    #[serde(default)]
    pub issues: Vec<RegisteredIssue>,
    pub failures: Vec<DevelopmentFailure>,
    /// LLM proposal space is not exhaustively enumerable by this bounded run.
    pub search_complete: bool,
    pub limitations: Vec<String>,
}
