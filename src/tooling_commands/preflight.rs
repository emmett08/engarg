//! CLI receipt for a model-free check of the selected isolated toolchain.
use clap::Args;
use engarg::{
    Error,
    tooling::{profile::ToolchainConfiguration, runner_preflight},
};
use std::{io::Write, path::PathBuf};

#[derive(Args)]
pub struct ToolchainCheckArgs {
    /// Operator-owned execution profile and optional generation library preferences.
    #[arg(long)]
    toolchain: PathBuf,
    /// New private receipt, retained for both successful and failed execution.
    #[arg(long)]
    output: PathBuf,
}

pub fn check(args: ToolchainCheckArgs) -> Result<(), Error> {
    let configuration = ToolchainConfiguration::load(&args.toolchain)?;
    let supervisor = super::supervisor()?;
    // Reserve the destination before executing so a failed check always has a receipt.
    let mut output = crate::private_file(&args.output)?;
    let receipt = runner_preflight::check(configuration.profile, &supervisor)?;
    output.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    receipt.ensure_passed().map_err(|error| match error {
        Error::ToolExecution(message) => {
            Error::ToolExecution(format!("{message}; inspect {}", args.output.display()))
        }
        other => other,
    })
}
