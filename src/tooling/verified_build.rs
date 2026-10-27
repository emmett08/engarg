//! One lifecycle for retaining, observing and committing generated source packages.
use super::{
    contracts::{NeedContract, contract_digest, validate_candidate, validate_contract},
    execution_diagnostics::{DiagnosticRunner, bounded, retain},
    generation::parse_candidate,
    package::{PackageStore, VerifiedPackage},
    profile::ToolchainProfile,
    runner::FixtureRunner,
    verification,
};
use crate::Error;
use eal_process::CancellationToken;
use serde::Serialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

pub(super) use super::execution_diagnostics::{feedback_redactions, redact_feedback};

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

pub struct VerificationEvent<'a> {
    pub attempt: &'a std::path::Path,
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
            return Err(Error::ToolExecution("tool implementation cancelled".into()));
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
                    attempt: &failure.attempt,
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
        let runner = Arc::new(DiagnosticRunner::new(
            Arc::clone(&self.runner),
            contract.clone(),
            sensitive,
        ));
        let worker_runner = Arc::clone(&runner);
        let task_contract = contract.clone();
        let task_candidate = candidate.clone();
        let task_attempt: PathBuf = attempt.clone();
        let cancellation = self.cancellation.clone();
        let profile = self.profile.clone();
        let (receipt, notes) = tokio::task::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let receipt = verification::verify(
                worker_runner.as_ref(),
                &task_contract,
                &task_candidate,
                &task_attempt,
                &cancellation,
            );
            // The worker completes even if timeout or interruption drops its
            // awaiting future. Retain terminal diagnostics before it returns.
            if cancellation.is_cancelled() {
                worker_runner.record_cancellation(
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                )?;
            }
            let notes = worker_runner.finish(&task_attempt, &profile);
            Ok::<_, Error>((receipt, notes))
        })
        .await
        .map_err(|_| Error::ToolExecution("tool fixture worker failed".into()))??;
        let receipt = receipt?;
        let audit_result = audit(VerificationEvent { attempt: &attempt });
        let notes = notes?;
        audit_result?;
        if self.cancellation.is_cancelled() {
            return Err(Error::ToolExecution("tool implementation cancelled".into()));
        }
        if !receipt.passed {
            let failed_tests = receipt
                .results
                .iter()
                .enumerate()
                .filter(|(_, result)| !result.passed)
                .take(8)
                .map(|(index, result)| {
                    let note = notes.get(index);
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
