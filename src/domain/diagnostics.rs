//! Stable validation categories with a location and an edge-formatted message.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    InvalidRequest,
    ReferenceIntegrity,
    SupportCycle,
    InvalidStructure,
    NarrativePolicy,
    EvidencePolicy,
    StatusPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DomainDiagnostic {
    pub code: DiagnosticCode,
    pub location: String,
    pub message: String,
}

impl std::fmt::Display for DomainDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub(crate) fn record(
    code: DiagnosticCode,
    location: &str,
    messages: Vec<String>,
) -> Vec<DomainDiagnostic> {
    messages
        .into_iter()
        .map(|message| DomainDiagnostic {
            code,
            location: location.to_owned(),
            message,
        })
        .collect()
}
