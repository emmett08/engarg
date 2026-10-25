//! Optional, isolated generation and verification of evidence collectors.
pub mod capability;
mod capability_report;
pub mod contracts;
pub mod generation;
pub mod package;
pub mod profile;
pub mod runner;
mod runner_snapshot;
mod runtime;
mod saved;
mod stage;
pub mod verification;

pub use profile::ToolchainProfile;
pub use runtime::{execute_registered, execute_registered_with_cancellation};
pub use stage::{
    ImplementationOutcome, Registration, ToolingConfig, ToolingCoverage, ToolingReport,
    ToolingSession, registration,
};
