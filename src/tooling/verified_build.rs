//! One lifecycle for retaining, observing and committing generated source packages.
use super::{
    contracts::{NeedContract, SourceFile, contract_digest, validate_candidate, validate_contract},
    generation::parse_candidate,
    package::{PackageStore, VerifiedPackage},
    profile::ToolchainProfile,
    runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
    verification,
};
use crate::Error;
use eal_process::CancellationToken;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const MAX_CANDIDATE_RESPONSE_BYTES: usize = 8 * 1_048_576;

/// Feedback contains bounded diagnostics, never frozen expected observations.
#[derive(Debug, Serialize)]
pub struct BuildFailure {
    pub schema: &'static str,
    pub stage: &'static str,
    pub contract_sha256: String,
    pub message: String,
    pub failed_tests: Vec<FailureDiagnostic>,
    pub attempt: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct FailureDiagnostic {
    pub name: String,
    pub kind: String,
    pub returncode: i32,
    pub diagnostic: Option<String>,
    pub stderr: String,
}

impl BuildFailure {
    pub fn feedback(&self) -> Value {
        json!({"schema":self.schema,"stage":self.stage,
            "contract_sha256":self.contract_sha256,"message":self.message,
            "failed_tests":self.failed_tests})
    }

    pub(super) fn error(self) -> Error {
        Error::InvalidOutput(format!(
            "{}; inspect {}",
            self.message,
            self.attempt.join("build-failure.json").display()
        ))
    }
}

pub enum BuildAttempt {
    Verified(Box<VerifiedPackage>),
    Rejected(Box<BuildFailure>),
}

fn bounded(text: &str, maximum_bytes: usize) -> String {
    let mut end = text.len().min(maximum_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

pub(super) fn feedback_redactions(profile: &ToolchainProfile) -> Vec<String> {
    let mut values: Vec<_> = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.to_string_lossy();
            let upper = name.to_ascii_uppercase();
            (profile
                .inherit_env
                .iter()
                .any(|selected| selected == name.as_ref())
                || [
                    "SECRET",
                    "TOKEN",
                    "PASSWORD",
                    "CREDENTIAL",
                    "AUTH",
                    "API_KEY",
                ]
                .iter()
                .any(|marker| upper.contains(marker))
                || upper.ends_with("_KEY"))
            .then(|| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
        })
        .collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    values
}

/// Redact before bounding so a token crossing the truncation boundary cannot
/// leave its visible prefix in a subsequent model request.
pub(super) fn redact_feedback(text: &str, sensitive: &[String]) -> String {
    sensitive
        .iter()
        .filter(|value| !value.is_empty())
        .fold(text.to_owned(), |text, value| {
            let escaped = serde_json::to_string(value).expect("string JSON");
            text.replace(value, "[redacted]")
                .replace(&escaped[1..escaped.len() - 1], "[redacted]")
        })
}

fn retain(path: &Path, bytes: &[u8]) -> Result<(), Error> {
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

#[derive(Default)]
struct ExecutionDiagnostics {
    notes: Vec<(String, Option<String>)>,
    error: Option<Error>,
}

/// Verification intentionally records failed executions as failed tests. This
/// wrapper also preserves infrastructure failures, so the repair loop cannot
/// mistake an unavailable runner or timeout for faulty generated source.
struct DiagnosticRunner {
    inner: Arc<dyn FixtureRunner>,
    contract: NeedContract,
    diagnostics: Mutex<ExecutionDiagnostics>,
    sensitive: Vec<String>,
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
                .map_err(|_| Error::Provider("tool diagnostics unavailable".into()))?;
            if diagnostics.error.is_some() {
                diagnostics.notes.push((String::new(), None));
                return Err(Error::Provider("isolated runner already failed".into()));
            }
        }
        let outcome = self
            .inner
            .execute(root, source, fixtures, request, mode, cancellation);
        let mut diagnostics = self
            .diagnostics
            .lock()
            .map_err(|_| Error::Provider("tool diagnostics unavailable".into()))?;
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
                if diagnostics.error.is_none()
                    && (outcome.execution_error.is_some()
                        || !(0..124).contains(&outcome.returncode))
                {
                    diagnostics.error = Some(Error::Provider(
                        "isolated tool execution failed or exhausted its execution bound".into(),
                    ));
                }
                Ok(outcome)
            }
            Err(error) => {
                diagnostics.notes.push((String::new(), None));
                if diagnostics.error.is_none() {
                    diagnostics.error = Some(error);
                }
                Err(Error::Provider("isolated tool runner failed".into()))
            }
        }
    }
}

pub struct VerificationEvent<'a> {
    pub need_id: &'a str,
    pub attempt: &'a std::path::Path,
    pub passed: bool,
}
pub struct VerifiedBuild<'a> {
    pub store: &'a PackageStore,
    pub key: &'a str,
    pub profile: &'a ToolchainProfile,
    pub runner: Arc<dyn FixtureRunner>,
    pub cancellation: &'a CancellationToken,
}
impl VerifiedBuild<'_> {
    pub async fn commit<A>(
        &self,
        contract: NeedContract,
        response: &[u8],
        audit: A,
    ) -> Result<VerifiedPackage, Error>
    where
        A: Fn(VerificationEvent<'_>) -> Result<(), Error>,
    {
        match self.attempt(contract, response, audit).await? {
            BuildAttempt::Verified(package) => Ok(*package),
            BuildAttempt::Rejected(failure) => Err(failure.error()),
        }
    }

    pub async fn attempt<A>(
        &self,
        contract: NeedContract,
        response: &[u8],
        audit: A,
    ) -> Result<BuildAttempt, Error>
    where
        A: Fn(VerificationEvent<'_>) -> Result<(), Error>,
    {
        validate_contract(&contract)?;
        if self.cancellation.is_cancelled() {
            return Err(Error::Provider("tool implementation cancelled".into()));
        }
        if response.len() > MAX_CANDIDATE_RESPONSE_BYTES {
            return Err(Error::Budget(
                "tool candidate exceeds retained response bound".into(),
            ));
        }
        let attempt = self.store.new_attempt(self.key)?;
        retain(&attempt.join("candidate-response.json"), response)?;
        let sensitive = feedback_redactions(self.profile);
        let candidate = match parse_candidate(response).and_then(|candidate| {
            validate_candidate(&contract, &candidate)?;
            Ok(candidate)
        }) {
            Ok(candidate) => candidate,
            Err(Error::InvalidOutput(message) | Error::InvalidInput(message)) => {
                let failure = BuildFailure {
                    schema: "engarg.tool-build-failure/1",
                    stage: "candidate",
                    contract_sha256: contract_digest(&contract)?,
                    message: bounded(&redact_feedback(&message, &sensitive), 1024),
                    failed_tests: Vec::new(),
                    attempt,
                };
                retain(
                    &failure.attempt.join("build-failure.json"),
                    &serde_json::to_vec_pretty(&failure)?,
                )?;
                audit(VerificationEvent {
                    need_id: &contract.need.id,
                    attempt: &failure.attempt,
                    passed: false,
                })?;
                return Ok(BuildAttempt::Rejected(Box::new(failure)));
            }
            Err(error) => return Err(error),
        };
        self.store.save_candidate(
            &attempt,
            &contract,
            &candidate,
            self.profile,
            self.runner.identity(),
        )?;
        let runner = Arc::new(DiagnosticRunner {
            inner: Arc::clone(&self.runner),
            contract: contract.clone(),
            diagnostics: Mutex::new(ExecutionDiagnostics::default()),
            sensitive,
        });
        let worker_runner = Arc::clone(&runner);
        let task_contract = contract.clone();
        let task_candidate = candidate.clone();
        let task_attempt: PathBuf = attempt.clone();
        let cancellation = self.cancellation.clone();
        let receipt = tokio::task::spawn_blocking(move || {
            verification::verify(
                worker_runner.as_ref(),
                &task_contract,
                &task_candidate,
                &task_attempt,
                &cancellation,
            )
        })
        .await
        .map_err(|_| Error::Provider("tool fixture worker failed".into()))??;
        if self.cancellation.is_cancelled() {
            return Err(Error::Provider("tool implementation cancelled".into()));
        }
        audit(VerificationEvent {
            need_id: &contract.need.id,
            attempt: &attempt,
            passed: receipt.passed,
        })?;
        let mut diagnostics = runner
            .diagnostics
            .lock()
            .map_err(|_| Error::Provider("tool diagnostics unavailable".into()))?;
        if let Some(error) = diagnostics.error.take() {
            return Err(error);
        }
        if !receipt.passed {
            let failed_tests = receipt
                .results
                .iter()
                .enumerate()
                .filter(|(_, result)| !result.passed)
                .take(8)
                .map(|(index, result)| {
                    let note = diagnostics.notes.get(index);
                    FailureDiagnostic {
                        name: bounded(&redact_feedback(&result.name, &runner.sensitive), 128),
                        kind: bounded(&redact_feedback(&result.kind, &runner.sensitive), 64),
                        returncode: result.returncode,
                        diagnostic: if result.kind == "protocol" {
                            result
                                .diagnostic
                                .as_deref()
                                .map(|text| bounded(&redact_feedback(text, &runner.sensitive), 512))
                        } else {
                            note.and_then(|(_, validation)| validation.clone())
                                .or_else(|| {
                                    result.diagnostic.as_deref().map(|text| {
                                        bounded(&redact_feedback(text, &runner.sensitive), 512)
                                    })
                                })
                        },
                        // Protocol probes include host-held-out request variants.
                        // Their streams may echo those inputs, so only the static
                        // failed-invariant diagnostic is suitable repair feedback.
                        stderr: if result.kind == "protocol" {
                            String::new()
                        } else {
                            note.map(|(stderr, _)| stderr.clone()).unwrap_or_default()
                        },
                    }
                })
                .collect();
            let failure = BuildFailure {
                schema: "engarg.tool-build-failure/1",
                stage: "verification",
                contract_sha256: contract_digest(&contract)?,
                message: "generated collector failed the unchanged frozen contract".into(),
                failed_tests,
                attempt,
            };
            retain(
                &failure.attempt.join("build-failure.json"),
                &serde_json::to_vec_pretty(&failure)?,
            )?;
            return Ok(BuildAttempt::Rejected(Box::new(failure)));
        }
        drop(diagnostics);
        let package = self.store.commit_verified(
            self.key,
            &attempt,
            contract,
            candidate,
            self.profile.clone(),
            self.runner.identity().clone(),
            receipt,
        )?;
        Ok(BuildAttempt::Verified(Box::new(package)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_redacts_supplied_secrets_before_truncation_and_in_encoded_code() {
        let token = "fixture-provider-token-123456".to_owned();
        let text = format!("{}{} trailing diagnostic", "x".repeat(1020), token);
        let redacted = bounded(&redact_feedback(&text, &[token]), 1024);
        assert!(!redacted.contains("fixture-provider"));
        assert!(redacted.ends_with("[red"));
        let secret = "quoted\"provider\ncredential".to_owned();
        let encoded = serde_json::to_string(&json!({"source":format!("token={secret}")})).unwrap();
        let redacted = redact_feedback(&encoded, &[secret]);
        assert!(!redacted.contains("provider"));
        assert!(redacted.contains("[redacted]"));
        assert_eq!(
            redact_feedback(
                "compiler error independent of secret",
                &["not-present".into()]
            ),
            "compiler error independent of secret"
        );
    }
}
