//! Construct and challenge engineering arguments from a claim and optional evidence.
//!
//! Validation checks references and explicit status rules. It cannot establish
//! empirical truth, logical soundness or the completeness of explored alternatives.

pub mod acquisition;
pub mod authoring_checkpoint;
pub mod collector_catalogue;
pub mod collector_preparation;
pub mod domain;
pub mod eal_acquisition;
pub mod eal_export;
pub mod eal_host;
pub mod eal_observe;
pub mod eal_prepare;
pub mod engine;
pub mod export_policy;
pub mod glossary;
pub mod presentations;
pub mod prompts;
pub mod provider;
pub mod render;
pub mod report;
pub mod retained_collectors;
pub mod route_planning;
pub mod scoped_collectors;
pub mod support_export;
pub mod support_graph;
pub mod support_requirements;
pub mod tooling;
pub mod trace_inspection;

mod argument_depth;
pub mod bounded_file;
mod collector_registry;
mod legacy_native_collectors;
// Frozen persisted projection readers deliberately retain their historical rules.
#[allow(dead_code)]
mod legacy_acquisition;
#[allow(dead_code)]
mod legacy_catalogue;
#[allow(dead_code)]
mod legacy_export;
#[allow(dead_code)]
mod legacy_plans;
mod output_validation;

#[cfg(test)]
#[path = "../tests/common/live_output.rs"]
mod live_output;

/// Errors distinguish invalid arguments from transport and resource failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ticket outcome: {0}")]
    Ticket(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid model output: {0}")]
    InvalidOutput(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("tool execution error: {0}")]
    ToolExecution(String),
    #[error("resource limit: {0}")]
    Budget(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<eal_adapters::Error> for Error {
    fn from(error: eal_adapters::Error) -> Self {
        match error {
            eal_adapters::Error::InvalidInput(message) => Self::InvalidInput(message),
            eal_adapters::Error::Provider(message) => Self::Provider(message),
            eal_adapters::Error::Budget(message) => Self::Budget(message),
            eal_adapters::Error::Io(error) => Self::Io(error),
            eal_adapters::Error::Json(error) => Self::Json(error),
        }
    }
}

impl From<eal_coordination_contracts::Error> for Error {
    fn from(error: eal_coordination_contracts::Error) -> Self {
        match error {
            eal_coordination_contracts::Error::InvalidInput(message) => Self::InvalidInput(message),
            eal_coordination_contracts::Error::Protocol(message) => Self::Provider(message),
        }
    }
}
