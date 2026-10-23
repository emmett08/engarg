//! Construct and challenge engineering arguments from a claim and optional evidence.
//!
//! Validation checks references and explicit status rules. It cannot establish
//! empirical truth, logical soundness or the completeness of explored alternatives.

pub mod domain;
pub mod eal_export;
pub mod eal_host;
pub mod engine;
pub mod presentations;
pub mod prompts;
pub mod provider;
pub mod render;

mod argument_depth;
mod output_validation;

/// Errors distinguish invalid arguments from transport and resource failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid model output: {0}")]
    InvalidOutput(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("resource limit: {0}")]
    Budget(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
