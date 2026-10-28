//! Optional, isolated generation and verification of evidence collectors.
pub mod capability;
pub mod completion;
pub mod contracts;
pub mod direct;
mod execution_diagnostics;
pub mod export;
pub mod generation;
pub mod library_policy;
pub mod package;
pub mod profile;
mod protocol;
pub mod replay;
pub mod runner;
pub mod runner_preflight;
mod runtime;
pub mod verification;
mod verified_build;

pub use profile::ToolchainProfile;
pub use runtime::{execute_registered, execute_registered_with_cancellation};
