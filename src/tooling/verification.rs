//! Observed contract tests gate all generated collector registration.
use std::{fs, path::Path};
use eal_collector_contracts::{AcquisitionRequest, ObservationEnvelope};
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::Error;
use super::{
    contracts::{CodeCandidate, FixtureKind, NeedContract, contract_digest, digest_json, validate_candidate, validate_contract},
    runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
};

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
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
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
    ).map_err(Error::InvalidOutput)?;
    let envelope: ObservationEnvelope = serde_json::from_value(value)
        .map_err(|_| Error::InvalidOutput("collector returned an invalid observation envelope".into()))?;
    if envelope.context.as_ref() != Some(&request.context)
        || envelope.request.as_ref() != Some(&request.acquisition())
    {
        return Err(Error::InvalidOutput("collector changed request or context correspondence".into()));
    }
    if envelope.value.get("collected") != Some(&Value::Bool(true)) {
        return Err(Error::InvalidOutput("collector did not record a successful acquisition".into()));
    }
    let validator = jsonschema::validator_for(&contract.draft.result_schema)
        .map_err(|_| Error::InvalidInput("invalid collector result schema".into()))?;
    if !validator.is_valid(&envelope.value) {
        return Err(Error::InvalidOutput("collector value differs from the frozen result schema".into()));
    }
    if envelope.observed_at.as_deref().is_some_and(|value| !valid_timestamp(value)) {
        return Err(Error::InvalidOutput("collector observed_at requires a valid UTC RFC3339 timestamp".into()));
    }
    Ok(envelope)
}

fn valid_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20 || bytes.len() > 64 || !value.is_ascii()
        || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T'
        || bytes[13] != b':' || bytes[16] != b':' || bytes.last() != Some(&b'Z')
    {
        return false;
    }
    let number = |start: usize, end: usize| -> Option<u32> {
        bytes[start..end].iter().all(u8::is_ascii_digit)
            .then(|| value[start..end].parse().ok()).flatten()
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4), number(5, 7), number(8, 10), number(11, 13), number(14, 16), number(17, 19),
    ) else { return false; };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day) && hour <= 23 && minute <= 59 && second <= 59
        && (bytes.len() == 20
            || (bytes[19] == b'.' && bytes.len() >= 22
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
        } else { diagnostic },
    }
}

fn failed_run(name: &str, kind: &str) -> TestResult {
    TestResult {
        name: name.into(), kind: kind.into(), passed: false, returncode: -1,
        stdout_sha256: digest(&[]), stderr_sha256: digest(&[]), elapsed_ms: 0,
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
            attempt, &candidate.files, &fixture.files, &input, ExecutionMode::Fixture, cancellation,
        );
        let kind = match fixture.kind {
            FixtureKind::Positive => "positive",
            FixtureKind::Negative => "negative",
            FixtureKind::AcquisitionError => "acquisition_error",
        };
        let record = match outcome {
            Err(_) => failed_run(&fixture.name, kind),
            Ok(outcome) => {
                let passed = if matches!(fixture.kind, FixtureKind::AcquisitionError) {
                    outcome.returncode != 0 && outcome.stdout.is_empty()
                } else if outcome.returncode == 0 {
                    validate_observation(contract, &request, &outcome.stdout)
                        .is_ok_and(|envelope| Some(envelope.value) == fixture.expected_value)
                } else { false };
                let diagnostic = (!passed).then(|| "observed result differs from the frozen fixture".into());
                result(&fixture.name, kind, outcome, passed, diagnostic)
            }
        };
        receipt.results.push(record);
    }
    let first = contract.draft.fixtures.iter().find(|fixture| !matches!(fixture.kind, FixtureKind::AcquisitionError))
        .ok_or_else(|| Error::InvalidInput("contract lacks a successful fixture".into()))?;
    let valid_request = json!({
        "evidence_id":contract.need.id,"environment":"engarg_tool_fixture",
        "tool":"engarg_generated_fixture","tool_version":"1",
        "input":first.input,"context":{"precision":precision}
    });
    let mut missing_identity = valid_request.clone();
    missing_identity.as_object_mut().expect("object").remove("evidence_id");
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
            attempt, &candidate.files, &first.files, &input, ExecutionMode::Fixture, cancellation,
        ) {
            Err(_) => failed_run(name, "protocol"),
            Ok(outcome) => {
                let passed = outcome.returncode != 0 && outcome.stdout.is_empty();
                result(name, "protocol", outcome, passed,
                    (!passed).then(|| "collector accepted an invalid protocol request".into()))
            }
        };
        receipt.results.push(record);
    }
    receipt.passed = !cancellation.is_cancelled()
        && receipt.results.len() == contract.draft.fixtures.len() + 3
        && receipt.results.iter().all(|entry| entry.passed);
    fs::write(attempt.join("test-results.json"), serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_timestamps_require_real_dates_and_utc() {
        assert!(valid_timestamp("2024-02-29T23:59:59.123Z"));
        assert!(!valid_timestamp("2023-02-29T23:59:59Z"));
        assert!(!valid_timestamp("2024-01-01T24:00:00Z"));
        assert!(!valid_timestamp("2024-01-01T00:00:00+00:00"));
        assert!(!valid_timestamp("2024-01-01T00:00:00.Z"));
    }
}
