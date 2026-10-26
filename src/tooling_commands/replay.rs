//! Expose candidate replay as a local command with no provider configuration.
use clap::Args;
use engarg::{Error, tooling::replay::replay_attempt};
use std::{io::Write, path::PathBuf, time::Duration};

#[derive(Args)]
pub struct ToolReplayArgs {
    /// Failed retained attempt directory inside a tool store.
    #[arg(long)]
    attempt: PathBuf,
    /// Store for the newly verified package. The original attempt is preserved.
    #[arg(long)]
    tool_store: PathBuf,
    /// New private receipt file. No model credentials are needed.
    #[arg(long)]
    output: PathBuf,
    /// Aggregate replay deadline, including all frozen fixtures and protocol probes.
    #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86400))]
    tool_timeout_secs: u64,
}

pub async fn replay(args: ToolReplayArgs) -> Result<(), Error> {
    let mut output = crate::private_file(&args.output)?;
    let cancellation = eal_process::CancellationToken::default();
    let _guard = super::CancelOnDrop(cancellation.clone());
    let supervisor = super::supervisor()?;
    let receipt = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(args.tool_timeout_secs),
            replay_attempt(&args.attempt, &args.tool_store, &supervisor, &cancellation),
        ) => result.unwrap_or_else(|_| Err(Error::Budget("tool replay deadline exceeded".into()))),
        signal = tokio::signal::ctrl_c() => { signal?; Err(Error::Budget("tool replay interrupted".into())) },
        signal = super::termination_signal() => { signal?; Err(Error::Budget("tool replay terminated".into())) },
    };
    match receipt {
        Ok(receipt) => {
            serde_json::to_writer_pretty(&mut output, &receipt)?;
            output.write_all(b"\n")?;
            output.sync_all()?;
            Ok(())
        }
        Err(error) => {
            cancellation.cancel();
            drop(output);
            let _ = std::fs::remove_file(&args.output);
            Err(error)
        }
    }
}
