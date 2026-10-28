//! Observed contract tests gate all generated collector registration.
use eal_collector_contracts::{AcquisitionRequest, ObservationEnvelope};
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write, path::Path};

use super::{
    contracts::{
        CodeCandidate, FixtureKind, NeedContract, contract_digest, digest_json, validate_candidate,
        validate_contract,
    },
    protocol::{REJECTION_CASES, rejected},
    runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
};
use crate::Error;

pub use super::protocol::{TEST_SCHEMA, protocol_fixture_context, protocol_precision_context};

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

/// Capability contracts retain their time obligation in the frozen request.
/// Earlier flat collector contexts have no such contract and keep their
/// established optional observation-time behavior.
pub(super) fn requires_original_observation_time(contract: &NeedContract) -> bool {
    serde_json::from_str::<super::capability::CapabilityImplementationRequest>(
        &contract.request_context,
    )
    .is_ok_and(|request| {
        request
            .support_contract
            .time
            .require_original_observation_time
    })
}

/// Preserve correspondence using the same admission as a deployed tool package.
pub fn validate_observation(
    contract: &NeedContract,
    request: &AcquisitionRequest,
    bytes: &[u8],
) -> Result<ObservationEnvelope, Error> {
    Ok(eal_tool_runtime::observation::validate_observation(
        &contract.draft.result_schema,
        requires_original_observation_time(contract),
        request,
        bytes,
        1_048_576,
    )?)
}
#[cfg(test)]
use eal_tool_runtime::observation::valid_timestamp;

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
            context: protocol_fixture_context(&fixture.name)?,
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
        let kind = fixture.kind.label();
        let record = match outcome {
            Err(_) => failed_run(&fixture.name, kind),
            Ok(outcome) => {
                let passed = if matches!(&fixture.kind, FixtureKind::AcquisitionError) {
                    rejected(outcome.returncode, outcome.stdout.is_empty())
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
        "input":first.input,"context":protocol_fixture_context(&first.name)?
    });
    for probe in REJECTION_CASES {
        let name = probe.name();
        let input = probe.request(&valid_request)?;
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
                let passed = rejected(outcome.returncode, outcome.stdout.is_empty());
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
        && receipt.results.len() == contract.draft.fixtures.len() + REJECTION_CASES.len()
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
        OpaqueNumber,
        OpaqueRawValue,
        CheckFixtureIdentity,
        AcceptInvalidInput,
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
            if matches!(self.fault, Fault::CheckFixtureIdentity)
                && request
                    .context
                    .get("fixture")
                    .is_some_and(|selector| Some(selector) != request.input.get("fixture"))
            {
                return Ok(fail(None));
            }
            let mode = request
                .input
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("error");
            if mode == "error" && !matches!(self.fault, Fault::AcceptInvalidInput) {
                return Ok(fail(None));
            }
            let mut context = request.context.clone();
            if matches!(self.fault, Fault::Context) {
                context["engarg_verification"]["precision"]["integer"] = json!(1);
            }
            if matches!(self.fault, Fault::OpaqueNumber) {
                context["engarg_verification"]["precision"]["opaque_numeric_number"] = json!(123);
            }
            if matches!(self.fault, Fault::OpaqueRawValue) {
                context["engarg_verification"]["precision"]["opaque_raw_value"] =
                    json!({"literal":true});
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

    pub(crate) fn passing_receipt(identity: RunnerIdentity) -> VerificationReceipt {
        let directory = tempfile::tempdir().unwrap();
        verify(
            &FakeRunner {
                identity,
                fault: Fault::None,
            },
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
    fn schema_rejection_fixtures_require_an_observed_rejection_from_the_runner() {
        let mut total = contract();
        total.draft.input_schema["properties"]["mode"]["enum"] = json!(["positive", "negative"]);
        total.draft.fixtures[2].input = json!({});
        for (fault, expected) in [(Fault::None, true), (Fault::AcceptInvalidInput, false)] {
            let directory = tempfile::tempdir().unwrap();
            let receipt = verify(
                &FakeRunner {
                    identity: identity(),
                    fault,
                },
                &total,
                &candidate(),
                directory.path(),
                &CancellationToken::default(),
            )
            .unwrap();
            assert_eq!(receipt.passed, expected);
            assert_eq!(receipt.results[2].passed, expected);
        }
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
    fn verification_case_names_do_not_replace_operation_fixture_selectors() {
        let context = protocol_fixture_context("verification-case-name").unwrap();
        assert!(context.get("fixture").is_none());
        assert_eq!(
            context["engarg_verification"]["fixture"],
            "verification-case-name"
        );
        assert_eq!(
            context["engarg_verification"]["precision"],
            protocol_precision_context().unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let mut contract = contract();
        contract.draft.parameters.push("fixture".into());
        contract.draft.unresolved_arguments.push("fixture".into());
        contract.draft.input_schema["properties"]["fixture"] = json!({"type":"string"});
        contract.draft.input_schema["required"] = json!(["mode", "fixture"]);
        for fixture in &mut contract.draft.fixtures {
            fixture.input["fixture"] = json!("operator-snapshot-v1");
        }
        let receipt = verify(
            &FakeRunner {
                identity: identity(),
                fault: Fault::CheckFixtureIdentity,
            },
            &contract,
            &candidate(),
            directory.path(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(receipt.passed, "{:#?}", receipt.results);
    }

    #[test]
    fn opaque_reserved_keys_are_objects_and_cannot_be_reinterpreted() {
        let precision = protocol_precision_context().unwrap();
        assert_eq!(
            precision["opaque_number"]["$serde_json::private::Number"],
            "literal object"
        );
        assert_eq!(
            precision["opaque_raw_value"]["$serde_json::private::RawValue"],
            r#"{"literal":true}"#
        );
        for fault in [Fault::OpaqueNumber, Fault::OpaqueRawValue] {
            let receipt = tested(fault);
            assert_eq!(receipt.schema, "engarg.tool-tests/3");
            assert!(!receipt.passed);
            assert!(!receipt.results[0].passed);
        }
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

    fn capability_contract(require_original_time: bool) -> NeedContract {
        let mut contract = contract();
        contract.request_context = json!({
            "support_contract": {
                "schema":"eal.support-contract/1","support_type":"calculation",
                "scope":{"kind":"synthetic_flag","subject":"One supplied fixture mode",
                    "selectors":{},"identity_dimensions":["mode"]},
                "acceptable_methods":["synthetic_flag/1"],"quantities":[],
                "logical_strength":"exact_calculation","population":null,
                "time":{"historical_interval":null,"max_age_seconds":null,
                    "max_uncertainty_seconds":null,
                    "require_original_observation_time":require_original_time},
                "provenance":{"required_authorities":[],"transformation_history":false},
                "expected_output_schema_sha256":null
            },
            "input_schema":contract.draft.input_schema,
            "output_schema":contract.draft.result_schema,
            "operation_purpose":"Return whether the supplied synthetic mode is positive",
            "declared_limits":{}
        })
        .to_string();
        contract
    }

    fn successful_observation() -> (AcquisitionRequest, Value) {
        let request = AcquisitionRequest {
            evidence_id: "time_fixture".into(),
            environment: "engarg_tool_fixture".into(),
            tool: "engarg_generated_fixture".into(),
            tool_version: "1".into(),
            input: json!({"mode":"positive"}),
            context: json!({"fixture":"required_original_time"}),
        };
        let envelope = json!({
            "value":{"collected":true,"present":true},
            "context":request.context,"request":request.acquisition()
        });
        (request, envelope)
    }

    #[test]
    fn required_original_time_rejects_absent_and_invalid_times_but_accepts_valid_time() {
        let contract = capability_contract(true);
        assert!(requires_original_observation_time(&contract));
        let (request, mut envelope) = successful_observation();
        let rejected =
            validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                .unwrap_err();
        assert!(
            rejected
                .to_string()
                .contains("requires original observed_at")
        );
        for invalid in ["2023-02-29T12:00:00Z", "2026-10-25T12:00:00+00:00", ""] {
            envelope["observed_at"] = json!(invalid);
            let error =
                validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                    .unwrap_err();
            assert!(error.to_string().contains("valid UTC RFC3339"));
        }
        envelope["observed_at"] = json!("2026-10-25T12:00:00.123Z");
        let admitted =
            validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                .unwrap();
        assert_eq!(
            admitted.observed_at.as_deref(),
            Some("2026-10-25T12:00:00.123Z")
        );
    }

    #[test]
    fn legacy_and_explicit_optional_time_contracts_keep_optional_time_behavior() {
        let (request, envelope) = successful_observation();
        for contract in [contract(), capability_contract(false)] {
            assert!(!requires_original_observation_time(&contract));
            validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                .unwrap();
        }
    }

    #[test]
    fn frozen_verification_rejects_missing_required_time_before_package_registration() {
        let directory = tempfile::tempdir().unwrap();
        let contract = capability_contract(true);
        let receipt = verify(
            &FakeRunner {
                identity: identity(),
                fault: Fault::None,
            },
            &contract,
            &candidate(),
            directory.path(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(!receipt.passed);
        for name in ["positive", "negative"] {
            assert!(
                !receipt
                    .results
                    .iter()
                    .find(|result| result.name == name)
                    .unwrap()
                    .passed
            );
        }
        let prompt = super::super::generation::code_request(&contract);
        let supplied: Value = serde_json::from_str(&prompt.user).unwrap();
        assert_eq!(supplied["original_observation_time_required"], true);
        assert!(prompt.system.contains("observed_at is mandatory"));
        let optional = super::super::generation::code_request(&capability_contract(false));
        let supplied: Value = serde_json::from_str(&optional.user).unwrap();
        assert_eq!(supplied["original_observation_time_required"], false);
    }

    #[test]
    fn library_metadata_preserves_required_source_time_in_observation_and_coding_contracts() {
        use crate::tooling::{
            capability::CapabilityImplementationRequest, library_policy::LibraryPolicy,
        };

        let model_fallback: LibraryPolicy = serde_json::from_value(json!({
            "use_defaults":false,"fallback":"model","tasks":{}
        }))
        .unwrap();
        for policy in [LibraryPolicy::default(), model_fallback] {
            let mut contract = capability_contract(true);
            let mut operation: CapabilityImplementationRequest =
                serde_json::from_str(&contract.request_context).unwrap();
            operation.library_policy = Some(policy.resolve("python").unwrap());
            contract.request_context = serde_json::to_string(&operation).unwrap();
            assert!(requires_original_observation_time(&contract));

            let (request, mut envelope) = successful_observation();
            let error =
                validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                    .unwrap_err();
            assert!(error.to_string().contains("requires original observed_at"));
            envelope["observed_at"] = json!("2026-10-25T12:00:00.123000Z");
            let admitted =
                validate_observation(&contract, &request, &serde_json::to_vec(&envelope).unwrap())
                    .unwrap();
            assert_eq!(
                admitted.observed_at.as_deref(),
                Some("2026-10-25T12:00:00.123000Z")
            );

            let prompt = super::super::generation::code_request(&contract);
            let supplied: Value = serde_json::from_str(&prompt.user).unwrap();
            assert_eq!(supplied["original_observation_time_required"], true);
            let context: Value =
                serde_json::from_str(supplied["request_context"].as_str().unwrap()).unwrap();
            assert_eq!(context["library_policy"]["language"], "python");
            assert_eq!(context["library_policy"]["fallback"], "model");
            assert_eq!(
                context["support_contract"]["time"]["require_original_observation_time"],
                true
            );
        }
    }
}
