//! CLI entry points for optional implementation and verified collector execution.
use clap::Args;
use engarg::{
    Error,
    provider::LanguageModel,
    tooling::{ToolchainProfile, ToolingConfig, ToolingSession},
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub mod capability;
mod native_provider;
pub mod provider;
pub mod registration;

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

    pub fn admit(
        &self,
        enabled: bool,
        max_input_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Option<ToolingSession>, Error> {
        self.config(enabled, max_input_bytes, max_response_bytes)?
            .map(ToolingSession::admit)
            .transpose()
    }

    /// Reuse operator-owned options while keeping capability coding separate from
    /// legacy per-need overrides. Calling this method does not admit Docker or a model.
    pub fn capability_config(
        &self,
        enabled: bool,
        max_input_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Option<engarg::tooling::capability::CapabilityImplementationConfig>, Error> {
        Ok(self
            .config(enabled, max_input_bytes, max_response_bytes)?
            .map(
                |config| engarg::tooling::capability::CapabilityImplementationConfig {
                    language: config.language,
                    profile: config.profile,
                    store: config.store,
                    max_calls: config.max_calls,
                    max_input_bytes: config.max_input_bytes,
                    max_response_bytes: config.max_response_bytes,
                    supervisor: config.supervisor,
                    engarg_executable: config.engarg_executable,
                },
            ))
    }

    fn config(
        &self,
        enabled: bool,
        max_input_bytes: usize,
        max_response_bytes: usize,
    ) -> Result<Option<ToolingConfig>, Error> {
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
        let config = ToolingConfig {
            language,
            profile,
            store,
            output_dir: self.output_dir()?.to_path_buf(),
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

/// Dropping an implementation future cancels its active verification runner.
pub async fn run_stage<M: LanguageModel>(
    session: &ToolingSession,
    model: &M,
    original: &Value,
    timeout_seconds: u64,
) -> Result<Value, Error> {
    let result = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(timeout_seconds),
            session.implement(model, original),
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
    result.with_tooling(original)
}

#[derive(Args)]
pub struct ImplementArgs {
    /// Saved canonical analysis or supported EAL export bundle.
    #[arg(long)]
    input: PathBuf,
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
    engarg::report::validate_replanning(&original, &std::env::current_exe()?)?;
    let graph = matches!(
        engarg::report::classify_evidence_planning(&original)?,
        engarg::report::EvidencePlanningReport::Graph(_)
    );
    // Graph routes classify genuine implementation gaps before admitting Docker.
    // The flat stage retains its existing image/runner/directory preflight.
    let session = if graph {
        args.tools.capability_config(
            true,
            args.provider.max_input_bytes,
            args.provider.max_response_bytes,
        )?;
        None
    } else {
        args.tools.admit(
            true,
            args.provider.max_input_bytes,
            args.provider.max_response_bytes,
        )?
    };
    let model = progress.model(args.provider.model()?);
    progress.phase("implementing and verifying missing tools");
    let report = if graph {
        capability::run_capability_fallback(
            &model,
            &original,
            &args.tools,
            args.provider.max_input_bytes,
            args.provider.max_response_bytes,
        )
        .await?
    } else {
        let session = session
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("implementation stage was not enabled".into()))?;
        run_stage(session, &model, &original, args.tools.deadline()).await?
    };
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
