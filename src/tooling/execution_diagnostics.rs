//! Retain bounded execution failures without disclosing selected environment values.
use super::{
    contracts::{NeedContract, SourceFile, contract_digest},
    profile::ToolchainProfile,
    runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
    verification,
};
use crate::Error;
use eal_process::CancellationToken;
use serde::Serialize;
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

pub(super) use eal_tool_runtime::execution_diagnostics::{
    bounded, feedback_redactions, redact_feedback,
};

pub(super) fn retain(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

const FAILURE_SCHEMA: &str = "engarg.tool-execution-failure/1";
const FAILURE_FILE: &str = "execution-failure.json";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureClass {
    Timeout,
    OutputLimit,
    Cancelled,
    ContainerExit,
    SupervisorFailure,
    RunnerFailure,
}

impl FailureClass {
    fn label(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::OutputLimit => "output_limit",
            Self::Cancelled => "cancelled",
            Self::ContainerExit => "container_exit",
            Self::SupervisorFailure => "supervisor_failure",
            Self::RunnerFailure => "runner_failure",
        }
    }
}

#[derive(Debug, Serialize)]
struct ExecutionFailure {
    class: FailureClass,
    returncode: Option<i32>,
    execution_error: Option<String>,
    elapsed_ms: u64,
    stderr: String,
    stderr_truncated: bool,
    diagnostic: Option<String>,
    mode: &'static str,
}

#[derive(Serialize)]
struct ExecutionBounds {
    timeout_seconds: u64,
    memory_mb: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

#[derive(Serialize)]
struct FailureReceipt<'a> {
    schema: &'static str,
    stage: &'static str,
    contract_sha256: String,
    attempt: &'a Path,
    runner: &'a RunnerIdentity,
    bounds: ExecutionBounds,
    failure: &'a ExecutionFailure,
}

type Notes = Vec<(String, Option<String>)>;

#[derive(Default)]
struct ExecutionDiagnostics {
    notes: Notes,
    failure: Option<ExecutionFailure>,
}

/// Infrastructure failures stop execution and remain outside coding-repair feedback.
pub(super) struct DiagnosticRunner {
    inner: Arc<dyn FixtureRunner>,
    contract: NeedContract,
    diagnostics: Mutex<ExecutionDiagnostics>,
    pub(super) sensitive: Vec<String>,
}

impl DiagnosticRunner {
    pub(super) fn record_cancellation(&self, elapsed_ms: u64) -> Result<(), Error> {
        let mut diagnostics = self
            .diagnostics
            .lock()
            .map_err(|_| Error::ToolExecution("tool diagnostics unavailable".into()))?;
        if diagnostics.failure.is_none() {
            diagnostics.failure = Some(ExecutionFailure {
                class: FailureClass::Cancelled,
                returncode: None,
                execution_error: Some("cancelled".into()),
                elapsed_ms,
                stderr: String::new(),
                stderr_truncated: false,
                diagnostic: Some("verification cancelled before completion".into()),
                mode: "fixture",
            });
        }
        Ok(())
    }

    pub(super) fn new(
        inner: Arc<dyn FixtureRunner>,
        contract: NeedContract,
        sensitive: Vec<String>,
    ) -> Self {
        Self {
            inner,
            contract,
            diagnostics: Mutex::new(ExecutionDiagnostics::default()),
            sensitive,
        }
    }

    /// Persist before returning so an infrastructure failure always names its receipt.
    pub(super) fn finish(
        &self,
        attempt: &Path,
        profile: &ToolchainProfile,
    ) -> Result<Notes, Error> {
        let diagnostics = self
            .diagnostics
            .lock()
            .map_err(|_| Error::ToolExecution("tool diagnostics unavailable".into()))?;
        if let Some(failure) = &diagnostics.failure {
            let path = attempt.join(FAILURE_FILE);
            retain(
                &path,
                &serde_json::to_vec_pretty(&FailureReceipt {
                    schema: FAILURE_SCHEMA,
                    stage: "verification",
                    contract_sha256: contract_digest(&self.contract)?,
                    attempt,
                    runner: self.inner.identity(),
                    bounds: ExecutionBounds {
                        timeout_seconds: profile.timeout_seconds,
                        memory_mb: profile.memory_mb,
                        max_stdout_bytes: profile.output_bytes,
                        max_stderr_bytes: profile.output_bytes,
                    },
                    failure,
                })?,
            )?;
            let status = failure
                .returncode
                .map(|code| format!(" (exit status {code})"))
                .unwrap_or_default();
            let detail = failure.diagnostic.as_deref().unwrap_or(&failure.stderr);
            let detail = bounded(detail, 512).escape_debug().to_string();
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!("; {detail}")
            };
            return Err(Error::ToolExecution(format!(
                "verification {}{status}{detail}; inspect {}",
                failure.class.label(),
                path.display()
            )));
        }
        Ok(diagnostics.notes.clone())
    }

    fn failure(
        &self,
        outcome: &RunOutcome,
        mode: ExecutionMode,
        cancelled: bool,
    ) -> ExecutionFailure {
        let class = if cancelled {
            FailureClass::Cancelled
        } else {
            match outcome.execution_error.as_deref() {
                Some("timeout") => FailureClass::Timeout,
                Some("output_limit") => FailureClass::OutputLimit,
                Some("cancelled") => FailureClass::Cancelled,
                Some("container_execution_error") => FailureClass::ContainerExit,
                Some(_) => FailureClass::SupervisorFailure,
                None if outcome.returncode == 124 => FailureClass::Timeout,
                None => FailureClass::ContainerExit,
            }
        };
        let stderr = redact_feedback(&String::from_utf8_lossy(&outcome.stderr), &self.sensitive);
        ExecutionFailure {
            class,
            returncode: Some(outcome.returncode),
            execution_error: outcome
                .execution_error
                .as_deref()
                .map(|value| bounded(&redact_feedback(value, &self.sensitive), 128)),
            elapsed_ms: outcome.elapsed_ms,
            stderr_truncated: stderr.len() > 4096,
            stderr: bounded(&stderr, 4096),
            diagnostic: None,
            mode: mode_label(mode),
        }
    }
}

fn mode_label(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Fixture => "fixture",
        ExecutionMode::Runtime => "runtime",
    }
}

impl FixtureRunner for DiagnosticRunner {
    fn identity(&self) -> &RunnerIdentity {
        self.inner.identity()
    }

    fn execute(
        &self,
        root: &Path,
        source: &[SourceFile],
        fixtures: &[SourceFile],
        request: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        {
            let mut diagnostics = self
                .diagnostics
                .lock()
                .map_err(|_| Error::ToolExecution("tool diagnostics unavailable".into()))?;
            if diagnostics.failure.is_some() {
                diagnostics.notes.push((String::new(), None));
                return Err(Error::ToolExecution(
                    "isolated runner already failed".into(),
                ));
            }
        }
        let started = Instant::now();
        let outcome = self
            .inner
            .execute(root, source, fixtures, request, mode, cancellation);
        let mut diagnostics = self
            .diagnostics
            .lock()
            .map_err(|_| Error::ToolExecution("tool diagnostics unavailable".into()))?;
        match outcome {
            Ok(outcome) => {
                let validation = if outcome.returncode == 0 {
                    eal_collector_contracts::strict_json_exact(request, Default::default())
                        .ok()
                        .and_then(|value| {
                            serde_json::from_value::<eal_collector_contracts::AcquisitionRequest>(
                                value,
                            )
                            .ok()
                        })
                        .filter(|request| request.validate().is_ok())
                        .and_then(|request| {
                            verification::validate_observation(
                                &self.contract,
                                &request,
                                &outcome.stdout,
                            )
                            .err()
                        })
                        .map(|error| {
                            bounded(&redact_feedback(&error.to_string(), &self.sensitive), 512)
                        })
                } else {
                    None
                };
                diagnostics.notes.push((
                    bounded(
                        &redact_feedback(
                            &String::from_utf8_lossy(&outcome.stderr),
                            &self.sensitive,
                        ),
                        1024,
                    ),
                    validation,
                ));
                if cancellation.is_cancelled()
                    || outcome.execution_error.is_some()
                    || !(0..124).contains(&outcome.returncode)
                {
                    diagnostics.failure =
                        Some(self.failure(&outcome, mode, cancellation.is_cancelled()));
                }
                Ok(outcome)
            }
            Err(error) => {
                diagnostics.notes.push((String::new(), None));
                diagnostics.failure = Some(ExecutionFailure {
                    class: FailureClass::RunnerFailure,
                    returncode: None,
                    execution_error: None,
                    elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    stderr: String::new(),
                    stderr_truncated: false,
                    diagnostic: Some(bounded(
                        &redact_feedback(&error.to_string(), &self.sensitive),
                        1024,
                    )),
                    mode: mode_label(mode),
                });
                Err(Error::ToolExecution("isolated tool runner failed".into()))
            }
        }
    }
}

#[cfg(test)]
#[path = "execution_diagnostics_tests.rs"]
mod tests;
