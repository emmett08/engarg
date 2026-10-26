//! Optional, isolated generation and verification of evidence collectors.
pub mod capability;
mod capability_report;
pub mod completion;
pub mod contracts;
mod execution_diagnostics;
pub mod generated_registration;
pub mod generation;
pub mod package;
pub mod profile;
mod protocol;
pub mod replay;
pub mod runner;
pub mod runner_preflight;
mod runner_snapshot;
mod runtime;
mod saved;
mod stage;
pub mod verification;
mod verified_build;

pub use profile::ToolchainProfile;
pub use runtime::{execute_registered, execute_registered_with_cancellation};
pub use stage::{
    ImplementationOutcome, Registration, ToolingConfig, ToolingCoverage, ToolingReport,
    ToolingSession, registration,
};
