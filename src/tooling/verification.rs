//! Observed contract tests gate all generated collector registration.
use eal_collector_contracts::{AcquisitionRequest, ObservationEnvelope};
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write, path::Path};

use super::{
    contracts::{
        CodeCandidate, FixtureKind, NeedContract, contract_digest, digest_json, validate_candidate,
        validate_contract,
    },
    runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
};
use crate::Error;

pub const TEST_SCHEMA: &str = "engarg.tool-tests/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestResult {
    pub name: String,
    pub kind: String,
    pub passed: bool,
    pub returncode: i32,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub elapsed_ms: u64,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReceipt {
    pub schema: String,
    pub language: String,
    pub contract_sha256: String,
    pub candidate_sha256: String,
    pub source_sha256: String,
    pub fixtures_sha256: String,
    pub runner: RunnerIdentity,
    pub passed: bool,
    pub results: Vec<TestResult>,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Preserve correspondence before allowing an observation into the registry path.
pub fn validate_observation(
    contract: &NeedContract,
    request: &AcquisitionRequest,
    bytes: &[u8],
) -> Result<ObservationEnvelope, Error> {
    let value = eal_collector_contracts::strict_json_exact(
        bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidOutput)?;
    let envelope: ObservationEnvelope = serde_json::from_value(value).map_err(|_| {
        Error::InvalidOutput("collector returned an invalid observation envelope".into())
    })?;
    if envelope.context.as_ref() != Some(&request.context)
        || envelope.request.as_ref() != Some(&request.acquisition())
    {
        return Err(Error::InvalidOutput(
            "collector changed request or context correspondence".into(),
        ));
    }
    if envelope.value.get("collected") != Some(&Value::Bool(true)) {
        return Err(Error::InvalidOutput(
            "collector did not record a successful acquisition".into(),
        ));
    }
    let validator = jsonschema::validator_for(&contract.draft.result_schema)
        .map_err(|_| Error::InvalidInput("invalid collector result schema".into()))?;
    if !validator.is_valid(&envelope.value) {
        return Err(Error::InvalidOutput(
            "collector value differs from the frozen result schema".into(),
        ));
    }
    if envelope
        .observed_at
        .as_deref()
        .is_some_and(|value| !valid_timestamp(value))
    {
        return Err(Error::InvalidOutput(
            "collector observed_at requires a valid UTC RFC3339 timestamp".into(),
        ));
    }
    Ok(envelope)
}

fn valid_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.len() > 64
        || !value.is_ascii()
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes.last() != Some(&b'Z')
    {
        return false;
    }
    let number = |start: usize, end: usize| -> Option<u32> {
        bytes[start..end]
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| value[start..end].parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0
        && (1..=days).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59
        && (bytes.len() == 20
            || (bytes[19] == b'.'
                && bytes.len() >= 22
                && bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit)))
}

fn result(
    name: &str,
    kind: &str,
    outcome: RunOutcome,
    passed: bool,
    diagnostic: Option<String>,
) -> TestResult {
    TestResult {
        name: name.into(),
        kind: kind.into(),
        passed: passed && outcome.execution_error.is_none(),
        returncode: outcome.returncode,
        stdout_sha256: digest(&outcome.stdout),
        stderr_sha256: digest(&outcome.stderr),
        elapsed_ms: outcome.elapsed_ms,
        diagnostic: if outcome.execution_error.is_some() {
            Some("isolated execution did not complete normally".into())
        } else {
            diagnostic
        },
    }
}

fn failed_run(name: &str, kind: &str) -> TestResult {
    TestResult {
        name: name.into(),
        kind: kind.into(),
        passed: false,
        returncode: -1,
        stdout_sha256: digest(&[]),
        stderr_sha256: digest(&[]),
        elapsed_ms: 0,
        diagnostic: Some("isolated runner failed before an observed result".into()),
    }
}

/// This executes source; model-authored claims about tests never enter a receipt.
pub fn verify<R: FixtureRunner + ?Sized>(
    runner: &R,
    contract: &NeedContract,
    candidate: &CodeCandidate,
    attempt: &Path,
    cancellation: &CancellationToken,
) -> Result<VerificationReceipt, Error> {
    validate_contract(contract)?;
    validate_candidate(contract, candidate)?;
    let mut receipt = VerificationReceipt {
        schema: TEST_SCHEMA.into(),
        language: contract.language.clone(),
        contract_sha256: contract_digest(contract)?,
        candidate_sha256: digest_json(candidate)?,
        source_sha256: digest_json(&candidate.files)?,
        fixtures_sha256: digest_json(&contract.draft.fixtures)?,
        runner: runner.identity().clone(),
        passed: false,
        results: Vec::new(),
    };
    let precision: Value = serde_json::from_str(
        r#"{"integer":90071992547409931234567890,"decimal":1.234567890123456789,"escaped":"quote \" slash \\ CRLF\r\n\u2028"}"#,
    )?;
    for fixture in &contract.draft.fixtures {
        if cancellation.is_cancelled() {
            receipt.results.push(failed_run(&fixture.name, "cancelled"));
            break;
        }
        let request = AcquisitionRequest {
            evidence_id: contract.need.id.clone(),
            environment: "engarg_tool_fixture".into(),
            tool: "engarg_generated_fixture".into(),
            tool_version: "1".into(),
            input: fixture.input.clone(),
            context: json!({"precision":precision,"fixture":fixture.name}),
        };
        let input = serde_json::to_vec(&request)?;
        let outcome = runner.execute(
            attempt,
            &candidate.files,
            &fixture.files,
            &input,
            ExecutionMode::Fixture,
            cancellation,
        );
        let kind = match &fixture.kind {
            FixtureKind::Positive => "positive",
            FixtureKind::Negative => "negative",
            FixtureKind::AcquisitionError => "acquisition_error",
        };
        let record = match outcome {
            Err(_) => failed_run(&fixture.name, kind),
            Ok(outcome) => {
                let passed = if matches!(&fixture.kind, FixtureKind::AcquisitionError) {
                    outcome.returncode != 0 && outcome.stdout.is_empty()
                } else if outcome.returncode == 0 {
                    validate_observation(contract, &request, &outcome.stdout)
                        .is_ok_and(|envelope| Some(envelope.value) == fixture.expected_value)
                } else {
                    false
                };
                let diagnostic =
                    (!passed).then(|| "observed result differs from the frozen fixture".into());
                result(&fixture.name, kind, outcome, passed, diagnostic)
            }
        };
        receipt.results.push(record);
    }
    let first = contract
        .draft
        .fixtures
        .iter()
        .find(|fixture| !matches!(&fixture.kind, FixtureKind::AcquisitionError))
        .ok_or_else(|| Error::InvalidInput("contract lacks a successful fixture".into()))?;
    let valid_request = json!({
        "evidence_id":contract.need.id,"environment":"engarg_tool_fixture",
        "tool":"engarg_generated_fixture","tool_version":"1",
        "input":first.input,"context":{"precision":precision}
    });
    let mut missing_identity = valid_request.clone();
    missing_identity
        .as_object_mut()
        .expect("object")
        .remove("evidence_id");
    let mut invalid_context = valid_request.clone();
    invalid_context["context"] = json!("invalid");
    let probes = [
        ("malformed_json", b"{".to_vec()),
        ("missing_identity", serde_json::to_vec(&missing_identity)?),
        ("invalid_context", serde_json::to_vec(&invalid_context)?),
    ];
    for (name, input) in probes {
        if cancellation.is_cancelled() {
            receipt.results.push(failed_run(name, "cancelled"));
            break;
        }
        let record = match runner.execute(
            attempt,
            &candidate.files,
            &first.files,
            &input,
            ExecutionMode::Fixture,
            cancellation,
        ) {
            Err(_) => failed_run(name, "protocol"),
            Ok(outcome) => {
                let passed = outcome.returncode != 0 && outcome.stdout.is_empty();
                result(
                    name,
                    "protocol",
                    outcome,
                    passed,
                    (!passed).then(|| "collector accepted an invalid protocol request".into()),
                )
            }
        };
        receipt.results.push(record);
    }
    receipt.passed = !cancellation.is_cancelled()
        && receipt.results.len() == contract.draft.fixtures.len() + 3
        && receipt.results.iter().all(|entry| entry.passed);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(attempt.join("test-results.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()?;
    Ok(receipt)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        acquisition::ToolArgument,
        domain::ArgumentCase,
        tooling::contracts::{CONTRACT_SCHEMA, ContractDraft, ContractFixture, SourceFile},
    };

    pub(crate) fn contract() -> NeedContract {
        let case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/claim-only-case.json")).unwrap();
        NeedContract {
            schema: CONTRACT_SCHEMA.into(),
            language: "python".into(),
            need: case.evidence_plan[0].clone(),
            request_context: r#"{"claim":"test","context":"","evidence":[]}"#.into(),
            toolchain_description: "synthetic fixture runner".into(),
            draft: ContractDraft {
                purpose: "Measure a synthetic present flag".into(),
                parameters: vec!["mode".into()],
                input_schema: json!({
                    "type":"object","properties":{"mode":{"type":"string","enum":["positive","negative","error"]}},
                    "required":["mode"],"additionalProperties":false
                }),
                result_schema: json!({
                    "type":"object","properties":{"collected":{"type":"boolean"},"present":{"type":"boolean"}},
                    "required":["collected","present"],"additionalProperties":false
                }),
                arguments: vec![ToolArgument {
                    name: "mode".into(),
                    value: "positive".into(),
                }],
                unresolved_arguments: Vec::new(),
                fixtures: vec![
                    ContractFixture {
                        name: "positive".into(),
                        kind: FixtureKind::Positive,
                        input: json!({"mode":"positive"}),
                        expected_value: Some(json!({"collected":true,"present":true})),
                        files: Vec::new(),
                    },
                    ContractFixture {
                        name: "negative".into(),
                        kind: FixtureKind::Negative,
                        input: json!({"mode":"negative"}),
                        expected_value: Some(json!({"collected":true,"present":false})),
                        files: Vec::new(),
                    },
                    ContractFixture {
                        name: "acquisition_error".into(),
                        kind: FixtureKind::AcquisitionError,
                        input: json!({"mode":"error"}),
                        expected_value: None,
                        files: Vec::new(),
                    },
                ],
            },
        }
    }

    pub(crate) fn candidate() -> CodeCandidate {
        CodeCandidate {
            language: "python".into(),
            files: vec![SourceFile {
                path: "collector.py".into(),
                content: "# synthetic source used by a verification test".into(),
            }],
        }
    }

    pub(crate) fn identity() -> RunnerIdentity {
        RunnerIdentity {
            profile_sha256: "0".repeat(64),
            image_id: format!("sha256:{}", "0".repeat(64)),
            docker_sha256: "1".repeat(64),
            runner_sha256: "2".repeat(64),
            supervisor_sha256: "3".repeat(64),
        }
    }

    #[derive(Clone, Copy)]
    enum Fault {
        None,
        Negative,
        Context,
        Timeout,
    }

    struct FakeRunner {
        identity: RunnerIdentity,
        fault: Fault,
    }
    impl FixtureRunner for FakeRunner {
        fn identity(&self) -> &RunnerIdentity {
            &self.identity
        }
        fn execute(
            &self,
            _package_root: &Path,
            _source_files: &[SourceFile],
            _fixture_files: &[SourceFile],
            request_bytes: &[u8],
            _mode: ExecutionMode,
            _cancellation: &CancellationToken,
        ) -> Result<RunOutcome, Error> {
            let fail = |execution_error| RunOutcome {
                stdout: Vec::new(),
                stderr: Vec::new(),
                returncode: 2,
                execution_error,
                elapsed_ms: 1,
            };
            if matches!(self.fault, Fault::Timeout) {
                return Ok(fail(Some("timeout".into())));
            }
            let parsed =
                eal_collector_contracts::strict_json_exact(request_bytes, Default::default())
                    .ok()
                    .and_then(|value| serde_json::from_value::<AcquisitionRequest>(value).ok());
            let Some(request) = parsed.filter(|request| request.validate().is_ok()) else {
                return Ok(fail(None));
            };
            let mode = request
                .input
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("error");
            if mode == "error" {
                return Ok(fail(None));
            }
            let mut context = request.context.clone();
            if matches!(self.fault, Fault::Context) {
                context["precision"]["integer"] = json!(1);
            }
            let envelope = json!({
                "value":{"collected":true,"present":mode == "positive" || matches!(self.fault, Fault::Negative)},
                "context":context,"request":request.acquisition()
            });
            Ok(RunOutcome {
                stdout: serde_json::to_vec(&envelope)?,
                stderr: Vec::new(),
                returncode: 0,
                execution_error: None,
                elapsed_ms: 1,
            })
        }
    }

    fn tested(fault: Fault) -> VerificationReceipt {
        let directory = tempfile::tempdir().unwrap();
        let runner = FakeRunner {
            identity: identity(),
            fault,
        };
        verify(
            &runner,
            &contract(),
            &candidate(),
            directory.path(),
            &CancellationToken::default(),
        )
        .unwrap()
    }

    #[test]
    fn measured_negative_and_protocol_failures_can_pass_their_exact_contracts() {
        let receipt = tested(Fault::None);
        assert!(receipt.passed);
        assert_eq!(receipt.results.len(), 6);
        assert!(receipt.results.iter().all(|result| result.passed));
    }

    #[test]
    fn fabricated_negative_values_cannot_register_as_passing_measurements() {
        let receipt = tested(Fault::Negative);
        assert!(!receipt.passed);
        assert!(
            !receipt
                .results
                .iter()
                .find(|result| result.name == "negative")
                .unwrap()
                .passed
        );
    }

    #[test]
    fn changed_exact_context_cannot_pass_despite_a_correct_value() {
        let receipt = tested(Fault::Context);
        assert!(!receipt.passed);
        assert!(!receipt.results[0].passed);
    }

    #[test]
    fn infrastructure_timeouts_are_not_expected_acquisition_errors() {
        let receipt = tested(Fault::Timeout);
        assert!(!receipt.passed);
        assert!(receipt.results.iter().all(|result| !result.passed));
    }

    #[test]
    fn observation_timestamps_require_real_dates_and_utc() {
        assert!(valid_timestamp("2024-02-29T23:59:59.123Z"));
        assert!(!valid_timestamp("2023-02-29T23:59:59Z"));
        assert!(!valid_timestamp("2024-01-01T24:00:00Z"));
        assert!(!valid_timestamp("2024-01-01T00:00:00+00:00"));
        assert!(!valid_timestamp("2024-01-01T00:00:00.Z"));
    }
}
