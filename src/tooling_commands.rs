//! CLI entry points for optional implementation and verified collector execution.
use clap::Args;
use engarg::{
    Error,
    provider::LanguageModel,
    tooling::{ToolchainProfile, capability::CapabilityImplementationConfig},
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub mod preflight;
pub mod provider;
pub mod replay;

#[derive(Args, Default)]
pub struct ToolOptions {
    /// Implementation language. Must match the operator-owned toolchain profile.
    #[arg(long, value_name = "LANGUAGE")]
    tool_language: Option<String>,
    /// Operator-owned isolated compiler/interpreter and runtime profile.
    #[arg(long, value_name = "JSON")]
    toolchain: Option<PathBuf>,
    /// Persistent verified collector package store; reusable across attempts.
    #[arg(long, value_name = "DIR")]
    tool_store: Option<PathBuf>,
    /// New private directory for this attempt's retained analysis and stage audit.
    #[arg(long, value_name = "NEWDIR")]
    tool_output_dir: Option<PathBuf>,
    /// Independent implementation-call budget. Defaults to 16 when enabled.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=256))]
    tool_max_calls: Option<u32>,
    /// Aggregate implementation deadline. Defaults to 3600 seconds.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=86400))]
    tool_timeout_secs: Option<u64>,
}

impl ToolOptions {
    pub fn deadline(&self) -> u64 {
        self.tool_timeout_secs.unwrap_or(3600)
    }

    pub fn output_dir(&self) -> Result<&Path, Error> {
        self.tool_output_dir.as_deref().ok_or_else(|| {
            Error::InvalidInput("tool implementation requires --tool-output-dir".into())
        })
    }

    /// Admit the supplied toolchain authority without starting Docker or a model.
    pub fn capability_config(
        &self,
        enabled: bool,
        max_input_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Option<CapabilityImplementationConfig>, Error> {
        self.config(enabled, max_input_bytes, max_response_bytes)
    }

    fn config(
        &self,
        enabled: bool,
        max_input_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Option<CapabilityImplementationConfig>, Error> {
        let requested = self.tool_language.is_some()
            || self.toolchain.is_some()
            || self.tool_store.is_some()
            || self.tool_output_dir.is_some()
            || self.tool_max_calls.is_some()
            || self.tool_timeout_secs.is_some();
        if !enabled {
            if requested {
                return Err(Error::InvalidInput(
                    "tool implementation options require --implement-tools during analyse".into(),
                ));
            }
            return Ok(None);
        }
        if max_input_bytes == 0 || max_response_bytes == 0 {
            return Err(Error::InvalidInput(
                "resource limits must be positive".into(),
            ));
        }
        let language = self.tool_language.clone().ok_or_else(|| {
            Error::InvalidInput("tool implementation requires --tool-language".into())
        })?;
        let profile = ToolchainProfile::load(self.toolchain.as_deref().ok_or_else(|| {
            Error::InvalidInput("tool implementation requires --toolchain".into())
        })?)?;
        engarg::tooling::contracts::validate_language(&language)?;
        if profile.language != language {
            return Err(Error::InvalidInput(
                "tool language differs from the operator toolchain profile".into(),
            ));
        }
        let store = self.tool_store.clone().ok_or_else(|| {
            Error::InvalidInput("tool implementation requires --tool-store".into())
        })?;
        let output = self.output_dir()?;
        if output.symlink_metadata().is_ok() {
            return Err(Error::InvalidInput(
                "tool implementation requires a new output directory".into(),
            ));
        }
        let config = CapabilityImplementationConfig {
            language,
            profile,
            store,
            max_calls: self.tool_max_calls.unwrap_or(16),
            max_input_bytes,
            max_response_bytes,
            supervisor: supervisor()?,
            engarg_executable: std::env::current_exe()?,
        };
        Ok(Some(config))
    }
}

pub fn supervisor() -> Result<PathBuf, Error> {
    let current = std::env::current_exe()?.canonicalize()?;
    let name = if cfg!(windows) {
        "engarg-process-supervisor.exe"
    } else {
        "engarg-process-supervisor"
    };
    Ok(current.with_file_name(name))
}

/// The same direct operation stage serves explicit implementation and authoring.
/// Dropping its future cancels active isolated verification through its RAII owner.
pub async fn run_direct_implementation<M: LanguageModel>(
    model: &M,
    original: &Value,
    tools: &Path,
    options: &ToolOptions,
    max_input_bytes: usize,
    max_response_bytes: usize,
) -> Result<Value, Error> {
    let implementation = options
        .capability_config(true, max_input_bytes, max_response_bytes)?
        .ok_or_else(|| Error::InvalidInput("tool implementation was not enabled".into()))?;
    let config = engarg::tooling::direct::DirectImplementationConfig {
        implementation,
        output_dir: options.output_dir()?.to_path_buf(),
    };
    let result = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(options.deadline()),
            engarg::tooling::direct::implement(model, original, tools, config),
        ) => result.map_err(|_| Error::Budget("tool implementation deadline exceeded".into()))?,
        interrupted = tokio::signal::ctrl_c() => {
            interrupted?;
            return Err(Error::Budget("tool implementation interrupted".into()));
        }
        terminated = termination_signal() => {
            terminated?;
            return Err(Error::Budget("tool implementation terminated".into()));
        }
    }?;
    serde_json::to_value(result).map_err(Error::from)
}

#[derive(Args)]
pub struct ImplementArgs {
    /// Saved canonical analysis with complete support elaboration.
    #[arg(long)]
    input: PathBuf,
    /// Selected execution host, checked before implementation model calls.
    #[arg(long, env = "EAL_HOST")]
    eal_host: Option<PathBuf>,
    /// Existing operator-authorised direct bindings; generated tools are added to a new copy.
    #[arg(long = "tools")]
    tools_manifest: PathBuf,
    /// Report phases, model calls and a ten-second waiting heartbeat on stderr.
    #[arg(long)]
    progress: bool,
    #[command(flatten)]
    provider: provider::ProviderArgs,
    #[command(flatten)]
    tools: ToolOptions,
}

pub async fn implement(args: ImplementArgs) -> Result<(), Error> {
    let progress = crate::progress::Progress::new(args.progress);
    progress
        .run(
            "tool implementation",
            implement_with_progress(args, &progress),
        )
        .await
}

async fn implement_with_progress(
    args: ImplementArgs,
    progress: &crate::progress::Progress,
) -> Result<(), Error> {
    progress.phase("tool implementation configuration checks");
    let original: Value = super::read_json(&args.input, 16 * 1_048_576)?;
    let output = args.tools.output_dir()?.join("analysis.with-tools.json");
    if output.exists() {
        return Err(Error::InvalidInput("output file already exists".into()));
    }
    let mut admitted = engarg::report::read_direct(&original)?;
    args.tools.capability_config(
        true,
        args.provider.max_input_bytes,
        args.provider.max_response_bytes,
    )?;
    admitted.target_runtime =
        Some(engarg::runtime_compatibility::preflight(args.eal_host.as_deref()).await?);
    let original = serde_json::to_value(admitted)?;
    let model = progress.model(args.provider.model()?);
    progress.phase("implementing and verifying missing operations");
    let report = run_direct_implementation(
        &model,
        &original,
        &args.tools_manifest,
        &args.tools,
        args.provider.max_input_bytes,
        args.provider.max_response_bytes,
    )
    .await?;
    progress.phase("writing implemented analysis");
    let bytes = serde_json::to_vec_pretty(&report)?;
    let mut file = super::private_file(&output)?;
    if let Err(error) = file
        .write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = std::fs::remove_file(&output);
        return Err(error.into());
    }
    Ok(())
}

#[derive(Args)]
pub struct ToolRunArgs {
    #[arg(long)]
    package: PathBuf,
}

async fn termination_signal() -> Result<(), Error> {
    #[cfg(unix)]
    {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?
            .recv()
            .await;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::future::pending::<()>().await;
        Ok(())
    }
}

struct CancelOnDrop(eal_process::CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub async fn tool_run(args: ToolRunArgs) -> Result<(), Error> {
    let mut bytes = Vec::new();
    std::io::stdin().take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err(Error::Budget("collector input exceeds 1 MiB".into()));
    }
    let supervisor = supervisor()?;
    let cancellation = eal_process::CancellationToken::default();
    let _guard = CancelOnDrop(cancellation.clone());
    let worker_cancellation = cancellation.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        engarg::tooling::execute_registered_with_cancellation(
            &args.package,
            &bytes,
            &supervisor,
            &worker_cancellation,
        )
    });
    let joined = tokio::select! {
        result = &mut worker => result,
        signal = tokio::signal::ctrl_c() => {
            cancellation.cancel();
            let _ = worker.await;
            signal?;
            return Err(Error::Budget("collector interrupted".into()));
        }
        signal = termination_signal() => {
            cancellation.cancel();
            let _ = worker.await;
            signal?;
            return Err(Error::Budget("collector terminated".into()));
        }
    };
    let value =
        joined.map_err(|error| Error::Provider(format!("collector worker failed: {error}")))??;
    let output = std::io::stdout();
    let mut lock = output.lock();
    serde_json::to_writer(&mut lock, &value)?;
    lock.write_all(b"\n")?;
    Ok(())
}

#[derive(Args)]
pub struct ToolExportArgs {
    /// Frozen verified package manifest produced by tool authoring.
    #[arg(long)]
    package: PathBuf,
    /// New directory for source, pinned execution TOML and direct EAL binding.
    #[arg(long)]
    output_dir: PathBuf,
    /// EAL tool identifier to bind; defaults to the verified package ID.
    #[arg(long)]
    tool_id: Option<String>,
}
pub fn tool_export(args: ToolExportArgs) -> Result<(), Error> {
    let result = engarg::tooling::export::export_verified(
        &args.package,
        &args.output_dir,
        args.tool_id.as_deref(),
    )?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &result)?;
    println!();
    Ok(())
}
