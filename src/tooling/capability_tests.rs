//! The generation path is exercised with observed fake execution, not model test claims.
use super::*;
use crate::{
    provider::{Completion, CompletionRequest, Role},
    tooling::{
        contracts::{SourceFile, contract_digest},
        runner::{ExecutionMode, RunOutcome, RunnerIdentity, executable_digest},
    },
};
use eal_collector_contracts::{
    AcquisitionRequest,
    capability::{
        LogicalStrength, ProvenanceRequirement, SUPPORT_CONTRACT_SCHEMA, SubjectScope, SupportType,
        TimeRequirement,
    },
};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn operation() -> CapabilityImplementationRequest {
    let original = verification::tests::contract();
    CapabilityImplementationRequest {
        support_contract: SupportContract {
            schema: SUPPORT_CONTRACT_SCHEMA.into(),
            support_type: SupportType::Calculation,
            scope: SubjectScope {
                kind: "synthetic_flag".into(),
                subject: "Caller-supplied mode".into(),
                selectors: BTreeMap::new(),
                identity_dimensions: vec!["mode".into()],
            },
            acceptable_methods: vec!["synthetic_flag/1".into()],
            quantities: Vec::new(),
            logical_strength: LogicalStrength::ExactCalculation,
            population: None,
            time: TimeRequirement {
                historical_interval: None,
                max_age_seconds: None,
                max_uncertainty_seconds: None,
                require_original_observation_time: false,
            },
            provenance: ProvenanceRequirement {
                required_authorities: Vec::new(),
                transformation_history: false,
            },
            expected_output_schema_sha256: None,
        },
        input_schema: original.draft.input_schema,
        output_schema: original.draft.result_schema,
        operation_purpose: "Return whether the supplied synthetic mode is positive".into(),
        declared_limits: BTreeMap::from([(
            "allowed_modes".into(),
            json!(["positive", "negative", "error"]),
        )]),
    }
}

fn config(store: &Path) -> CapabilityImplementationConfig {
    CapabilityImplementationConfig {
        language: "rust".into(),
        profile: ToolchainProfile {
            schema: super::super::profile::TOOLCHAIN_SCHEMA.into(),
            language: "rust".into(),
            docker: "docker".into(),
            image: "rust:1.99".into(),
            build_argv: vec![
                "rustc".into(),
                "/src/main.rs".into(),
                "-o".into(),
                "/build/collector".into(),
            ],
            run_argv: vec!["/build/collector".into()],
            inherit_env: Vec::new(),
            runtime_network: "none".into(),
            timeout_seconds: 30,
            memory_mb: 128,
            output_bytes: 1_048_576,
        },
        store: store.to_owned(),
        max_calls: 2,
        max_input_bytes: 1_048_576,
        max_response_bytes: 1_048_576,
        supervisor: PathBuf::from("unused-fake-supervisor"),
        engarg_executable: std::env::current_exe().unwrap(),
    }
}

fn draft_response(request: &CapabilityImplementationRequest) -> String {
    let mut draft = verification::tests::contract().draft;
    draft.purpose = request.operation_purpose.clone();
    draft.arguments.clear();
    draft.unresolved_arguments = vec!["mode".into()];
    json!({
        "purpose":draft.purpose,"parameters":draft.parameters,
        "input_schema_json":request.input_schema.to_string(),
        "result_schema_json":request.output_schema.to_string(),
        "arguments":[],"unresolved_arguments":draft.unresolved_arguments,
        "fixtures":draft.fixtures.iter().map(|fixture| json!({
            "name":fixture.name,"kind":fixture.kind,"input_json":fixture.input.to_string(),
            "expected_value_json":fixture.expected_value.as_ref().map(Value::to_string),
            "files":fixture.files
        })).collect::<Vec<_>>()
    })
    .to_string()
}

fn code_response(content: &str) -> String {
    json!({"language":"rust","files":[{"path":"main.rs","content":content}]}).to_string()
}

struct Model {
    responses: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
    calls: AtomicUsize,
}
impl Model {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        }
    }
}
impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request);
        let content = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model call; cache must be keyless");
        Ok(Completion {
            content,
            model: "observed-test-model".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

struct FakeRunner {
    identity: RunnerIdentity,
    executions: AtomicUsize,
}
impl FakeRunner {
    fn new(config: &CapabilityImplementationConfig) -> Self {
        Self {
            identity: RunnerIdentity {
                profile_sha256: digest_json(&config.profile).unwrap(),
                image_id: format!("sha256:{}", "0".repeat(64)),
                docker_sha256: "1".repeat(64),
                runner_sha256: executable_digest(&config.engarg_executable).unwrap(),
                supervisor_sha256: "3".repeat(64),
            },
            executions: AtomicUsize::new(0),
        }
    }
}
impl FixtureRunner for FakeRunner {
    fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }
    fn execute(
        &self,
        _package_root: &Path,
        source: &[SourceFile],
        _fixtures: &[SourceFile],
        bytes: &[u8],
        _mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        let fail = || RunOutcome {
            stdout: Vec::new(),
            stderr: Vec::new(),
            returncode: 2,
            execution_error: None,
            elapsed_ms: 1,
        };
        if cancellation.is_cancelled() {
            return Ok(fail());
        }
        let parsed = eal_collector_contracts::strict_json_exact(bytes, Default::default())
            .ok()
            .and_then(|value| serde_json::from_value::<AcquisitionRequest>(value).ok());
        let Some(request) = parsed.filter(|request| request.validate().is_ok()) else {
            return Ok(fail());
        };
        let mode = request.input["mode"].as_str().unwrap_or("error");
        if mode == "error" {
            return Ok(fail());
        }
        let mut context = request.context.clone();
        if source[0].content == "change_context" {
            context["mutated"] = json!(true);
        }
        if source[0].content == "opaque_number" {
            context["precision"]["opaque_numeric_number"] = json!(123);
        }
        if source[0].content == "opaque_raw_value" {
            context["precision"]["opaque_raw_value"] = json!({"literal":true});
        }
        let mut acquisition = request.acquisition();
        acquisition["context"] = context.clone();
        Ok(RunOutcome {
            stdout: serde_json::to_vec(&json!({
                "value":{"collected":true,"present":mode == "positive" || source[0].content == "lie"},
                "context":context,"request":acquisition
            }))?,
            stderr: Vec::new(),
            returncode: 0,
            execution_error: None,
            elapsed_ms: 1,
        })
    }
}

#[tokio::test]
async fn two_calls_freeze_contract_then_observe_tests_before_registration() {
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let model = Model::new(vec![draft_response(&request), code_response("correct")]);
    let result = implement_with_runner(
        &request,
        ImplementationGapKind::Operation,
        selected,
        &model,
        runner.clone(),
    )
    .await
    .unwrap();
    assert_eq!(result.model_calls, 2);
    assert!(!result.reused);
    assert_eq!(model.requests.lock().unwrap()[0].role, Role::Constructor);
    assert_eq!(model.requests.lock().unwrap()[1].role, Role::Single);
    assert!(runner.executions.load(Ordering::SeqCst) >= 6);
    assert!(result.qualification.receipt.is_file());
    assert!(result.capability.semantics.is_none());
    revalidate_implemented_capability(&result, &std::env::current_exe().unwrap()).unwrap();
    let mut forged = result.clone();
    forged.qualification.limitations.clear();
    assert!(revalidate_implemented_capability(&forged, &std::env::current_exe().unwrap()).is_err());
    assert!(
        result
            .qualification
            .limitations
            .iter()
            .any(|value| value.contains("scientific method"))
    );
    let package = super::super::package::load_verified(&result.package_manifest).unwrap();
    assert!(package.receipt().passed);
    assert!(package.contract().draft.arguments.is_empty());
    assert_eq!(package.contract().draft.unresolved_arguments, ["mode"]);
    assert!(
        result
            .collector
            .command
            .argv
            .iter()
            .any(|value| value == "tool-run")
    );
    // The coding call sees inputs and files, never the fixture oracle.
    assert!(
        !model.requests.lock().unwrap()[1]
            .user
            .contains("expected_value")
    );
}

#[tokio::test]
async fn generated_registration_keeps_source_interface_and_requires_separate_qualification() {
    use crate::tooling::generated_registration::*;
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let model = Model::new(vec![draft_response(&request), code_response("correct")]);
    let record = implement_with_runner(
        &request,
        ImplementationGapKind::Operation,
        selected,
        &model,
        runner,
    )
    .await
    .unwrap();
    let executable = std::env::current_exe().unwrap();
    let template =
        native_provider_template(&record, &executable, &executable, &Default::default()).unwrap();
    assert!(template["capability"]["semantics"].is_null());
    assert!(validate_qualified_provider(&template, &template).is_err());
    assert_eq!(
        operation_contract(&record).unwrap().identity().unwrap(),
        request.identity().unwrap()
    );
    let mut reviewed = template.clone();
    reviewed["capability"]["semantics"] = json!({
        "qualification_reference":"independent-synthetic-function-review/1",
        "support_types":["calculation"],"subject_kinds":["synthetic_flag"],"methods":["synthetic_flag/1"],
        "identity_dimensions":["mode"],"scope_constraints":{},"scope_bindings":{"mode":"/mode"},
        "quantities":[],"coverage":[{"strength":"exact_calculation","population":null,"completeness_pointer":null}],
        "time":{"historical_intervals":false,"historical_input":null,"original_observation_time":false,"uncertainty_seconds":0},
        "provenance":{"authorities":["synthetic-review"],"transformation_history":true},
        "limitations":["Synthetic function qualification only; no domain evidence acquired."]
    });
    reviewed["mapping"]["observation_time"] = json!({"kind":"collection_time"});
    reviewed["mapping"]["scope_pointers"] = json!({"mode":"/request/input/mode"});
    validate_qualified_provider(&template, &reviewed).unwrap();
    // A pending registration can be replaced by the reviewed contract without
    // authorizing a different executable, package, protocol or function schema.
    validate_provider_binding(&template, &template).unwrap();
    for pointer in [
        "/transport/program",
        "/pinned_files/0/sha256",
        "/capability/input_schema_sha256",
        "/mapping/value_pointer",
    ] {
        let mut forged = reviewed.clone();
        *forged.pointer_mut(pointer).unwrap() = json!("changed");
        assert!(
            validate_qualified_provider(&template, &forged).is_err(),
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn different_claims_reuse_one_parameterised_package_without_model_calls() {
    let directory = tempfile::tempdir().unwrap();
    let operation = operation();
    let mut selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let model = Model::new(vec![draft_response(&operation), code_response("correct")]);
    let first = implement_with_runner(
        &operation,
        ImplementationGapKind::Operation,
        selected.clone(),
        &model,
        runner.clone(),
    )
    .await
    .unwrap();
    selected.max_calls = 0;
    let keyless = Model::new(Vec::new());
    let second = implement_with_runner(
        &operation,
        ImplementationGapKind::Operation,
        selected,
        &keyless,
        runner.clone(),
    )
    .await
    .unwrap();
    assert_eq!(first.package_manifest, second.package_manifest);
    assert_eq!(first.capability_sha256, second.capability_sha256);
    assert!(second.reused);
    assert_eq!(second.model_calls, 0);
    assert_eq!(keyless.calls.load(Ordering::SeqCst), 0);
    let package = super::super::package::load_verified(&first.package_manifest).unwrap();
    for (evidence_id, tool, mode, claim, threshold) in [
        (
            "claim_a_need",
            "first_alias",
            "positive",
            "A fast service",
            "p99 < 5",
        ),
        (
            "claim_b_need",
            "second_alias",
            "negative",
            "A complete population",
            "count == 80",
        ),
    ] {
        let context = eal_collector_contracts::strict_json_exact(
            format!(r#"{{"claim":"{claim}","criterion":"{threshold}","integer":1234567890123456789012345678901234567890,"decimal":0.123456789012345678901234567890}}"#).as_bytes(), Default::default()
        ).unwrap();
        let runtime = AcquisitionRequest {
            evidence_id: evidence_id.into(),
            environment: "development".into(),
            tool: tool.into(),
            tool_version: "1".into(),
            input: json!({"mode":mode}),
            context,
        };
        assert_ne!(runtime.evidence_id, package.contract().need.id);
        let output = runner
            .execute(
                first.package_manifest.parent().unwrap(),
                &package.candidate().files,
                &[],
                &serde_json::to_vec(&runtime).unwrap(),
                ExecutionMode::Runtime,
                &CancellationToken::default(),
            )
            .unwrap();
        let envelope =
            verification::validate_observation(package.contract(), &runtime, &output.stdout)
                .unwrap();
        assert_eq!(envelope.context.as_ref(), Some(&runtime.context));
        assert_eq!(envelope.request.as_ref(), Some(&runtime.acquisition()));
        assert_eq!(envelope.value["present"], mode == "positive");
    }
    let frozen_json = serde_json::to_string(package.contract()).unwrap();
    assert!(!frozen_json.contains("claim_a_need"));
    assert!(!frozen_json.contains("p99 < 5"));
    assert!(!frozen_json.contains("claim_b_need"));
    assert!(!frozen_json.contains("count == 80"));
}

#[tokio::test]
async fn retry_uses_frozen_fixtures_and_only_one_coding_call() {
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let mut selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let failing = Model::new(vec![draft_response(&request), code_response("lie")]);
    assert!(
        implement_with_runner(
            &request,
            ImplementationGapKind::Transformation,
            selected.clone(),
            &failing,
            runner.clone()
        )
        .await
        .is_err()
    );
    let store = PackageStore::new(directory.path()).unwrap();
    let key = PackageStore::key_for(
        &request.need().unwrap(),
        &request.context().unwrap(),
        "rust",
        runner.identity(),
    )
    .unwrap();
    assert!(store.lookup(&key, runner.identity()).unwrap().is_none());
    let frozen = store
        .frozen_contract(&key, runner.identity())
        .unwrap()
        .unwrap();
    let digest = contract_digest(&frozen).unwrap();
    selected.max_calls = 1;
    let retry = Model::new(vec![code_response("correct")]);
    let result = implement_with_runner(
        &request,
        ImplementationGapKind::Transformation,
        selected,
        &retry,
        runner,
    )
    .await
    .unwrap();
    assert_eq!(result.model_calls, 1);
    assert_eq!(result.qualification.contract_sha256, digest);
    assert_eq!(retry.requests.lock().unwrap()[0].role, Role::Single);
}

#[tokio::test]
async fn rejected_gaps_do_not_admit_docker_store_or_model() {
    let directory = tempfile::tempdir().unwrap();
    let mut selected = config(&directory.path().join("must-not-exist"));
    selected.profile.docker = "/nonexistent/docker".into();
    let model = Model::new(Vec::new());
    for gap in [
        ImplementationGapKind::Configuration,
        ImplementationGapKind::Credentials,
        ImplementationGapKind::SourceUnavailable,
        ImplementationGapKind::HistoricalUnavailable,
        ImplementationGapKind::Judgement,
        ImplementationGapKind::Qualification,
        ImplementationGapKind::Clarification,
    ] {
        let error = implement_capability(&operation(), gap, selected.clone(), &model)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("only operation or transformation")
        );
    }
    assert!(!selected.store.exists());
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn model_cannot_rewrite_schemas_or_bind_consumer_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let mut response: Value = serde_json::from_str(&draft_response(&request)).unwrap();
    response["arguments"] = json!([{"name":"mode","value":"positive"}]);
    response["unresolved_arguments"] = json!([]);
    let model = Model::new(vec![response.to_string()]);
    assert!(
        implement_with_runner(
            &request,
            ImplementationGapKind::Operation,
            selected,
            &model,
            runner.clone()
        )
        .await
        .is_err()
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(runner.executions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn fixed_schema_changes_are_rejected_before_code_generation() {
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let mut response: Value = serde_json::from_str(&draft_response(&request)).unwrap();
    let mut weaker = request.output_schema.clone();
    weaker["properties"]["extra"] = json!({"type":"string"});
    response["result_schema_json"] = json!(weaker.to_string());
    let model = Model::new(vec![response.to_string()]);
    assert!(
        implement_with_runner(
            &request,
            ImplementationGapKind::Operation,
            selected,
            &model,
            runner.clone()
        )
        .await
        .is_err()
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(runner.executions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_fixed_interface_fails_before_docker_admission() {
    let directory = tempfile::tempdir().unwrap();
    let mut request = operation();
    let mut selected = config(directory.path());
    selected.profile.docker = "/nonexistent/docker".into();
    request.output_schema["required"] = json!(["present"]);
    let model = Model::new(Vec::new());
    let error = implement_capability(&request, ImplementationGapKind::Operation, selected, &model)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("boolean collected property"));
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn fresh_call_budget_is_admitted_before_drafting() {
    let directory = tempfile::tempdir().unwrap();
    let mut selected = config(directory.path());
    selected.max_calls = 1;
    let runner = Arc::new(FakeRunner::new(&selected));
    let model = Model::new(Vec::new());
    assert!(matches!(
        implement_with_runner(
            &operation(),
            ImplementationGapKind::Operation,
            selected,
            &model,
            runner
        )
        .await,
        Err(Error::Budget(_))
    ));
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn changed_context_and_rehashed_source_are_registration_gates() {
    let directory = tempfile::tempdir().unwrap();
    let request = operation();
    let selected = config(directory.path());
    let runner = Arc::new(FakeRunner::new(&selected));
    let bad = Model::new(vec![
        draft_response(&request),
        code_response("change_context"),
    ]);
    assert!(
        implement_with_runner(
            &request,
            ImplementationGapKind::Operation,
            selected.clone(),
            &bad,
            runner.clone()
        )
        .await
        .is_err()
    );
    let good = Model::new(vec![code_response("correct")]);
    let result = implement_with_runner(
        &request,
        ImplementationGapKind::Operation,
        selected.clone(),
        &good,
        runner.clone(),
    )
    .await
    .unwrap();
    std::fs::write(
        result
            .package_manifest
            .parent()
            .unwrap()
            .join("source/main.rs"),
        "tampered",
    )
    .unwrap();
    let keyless = Model::new(Vec::new());
    assert!(
        implement_with_runner(
            &request,
            ImplementationGapKind::Operation,
            selected,
            &keyless,
            runner
        )
        .await
        .is_err()
    );
    assert_eq!(keyless.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonical_operation_identity_ignores_object_order_but_keeps_semantic_limits() {
    let first = operation();
    let mut second = first.clone();
    second.input_schema = serde_json::from_str(r#"{"additionalProperties":false,"required":["mode"],"properties":{"mode":{"enum":["positive","negative","error"],"type":"string"}},"type":"object"}"#).unwrap();
    assert_eq!(first.identity().unwrap(), second.identity().unwrap());
    assert_eq!(first.context().unwrap(), second.context().unwrap());
    second.declared_limits.insert("max_items".into(), json!(7));
    assert_ne!(first.identity().unwrap(), second.identity().unwrap());
}

#[test]
fn reserved_schema_keys_and_declared_limits_survive_context_and_frozen_package_transport() {
    let directory = tempfile::tempdir().unwrap();
    let mut request = operation();
    let opaque_schema = json!({"type":"object","properties":{
        "$serde_json::private::Number":{"type":"string"},
        "$serde_json::private::RawValue":{"type":"object","properties":{"literal":{"type":"boolean"}},"additionalProperties":false}
    },"additionalProperties":false});
    request.input_schema["properties"]["payload"] = opaque_schema.clone();
    request.output_schema["properties"]["payload"] = opaque_schema;
    request.input_schema["properties"]["mode"]["x-engarg-source-contract"] = json!({"quantity":{"name":"latency","unit":"ms","shape":"array"},"method":"instrument.latency/1"});
    request.output_schema["properties"]["present"]["x-engarg-quantity"] =
        json!({"name":"acceptable","unit":"boolean"});
    request.operation_purpose=serde_json::to_string(&json!({"schema":"engarg.operation-spec/1","definition":"Compare each latency in milliseconds against the supplied bound; preserve sample order","input_conditions":["Every sample has unit ms"],"output_guarantees":["Return one boolean for each input sample"]})).unwrap();
    request.declared_limits.insert(
        "opaque".into(),
        verification::protocol_precision_context().unwrap(),
    );
    request.validate().unwrap();
    let context = request.context().unwrap();
    let decoded: CapabilityImplementationRequest = serde_json::from_str(&context).unwrap();
    assert_eq!(decoded.input_schema, request.input_schema);
    assert_eq!(decoded.output_schema, request.output_schema);
    assert_eq!(decoded.declared_limits, request.declared_limits);
    assert_eq!(decoded.identity().unwrap(), request.identity().unwrap());
    let selected = config(directory.path());
    let runner = FakeRunner::new(&selected);
    let mut draft = verification::tests::contract().draft;
    draft.purpose = request.operation_purpose.clone();
    draft.parameters = request.parameters().unwrap();
    draft.arguments.clear();
    draft.unresolved_arguments = draft.parameters.clone();
    draft.input_schema = request.input_schema.clone();
    draft.result_schema = request.output_schema.clone();
    let contract = NeedContract {
        schema: CONTRACT_SCHEMA.into(),
        language: "rust".into(),
        need: request.need().unwrap(),
        request_context: context,
        toolchain_description: selected.profile.description().unwrap(),
        draft,
    };
    let store = PackageStore::new(directory.path()).unwrap();
    let key = PackageStore::key_for(
        &contract.need,
        &contract.request_context,
        &contract.language,
        runner.identity(),
    )
    .unwrap();
    store
        .freeze_contract(&key, &contract, runner.identity())
        .unwrap();
    let retained = store
        .frozen_contract(&key, runner.identity())
        .unwrap()
        .unwrap();
    ensure_fixed_contract(&request, &retained).unwrap();
    assert_eq!(retained.draft.input_schema, request.input_schema);
    assert_eq!(retained.draft.result_schema, request.output_schema);
    assert_eq!(retained.request_context, request.context().unwrap());
}

#[tokio::test]
async fn opaque_context_corruption_cannot_register_a_generated_package() {
    for fault in ["opaque_number", "opaque_raw_value"] {
        let directory = tempfile::tempdir().unwrap();
        let request = operation();
        let selected = config(directory.path());
        let runner = Arc::new(FakeRunner::new(&selected));
        let model = Model::new(vec![draft_response(&request), code_response(fault)]);
        let result = implement_with_runner(
            &request,
            ImplementationGapKind::Operation,
            selected,
            &model,
            runner.clone(),
        )
        .await;
        assert!(result.is_err());
        let store = PackageStore::new(directory.path()).unwrap();
        let key = PackageStore::key_for(
            &request.need().unwrap(),
            &request.context().unwrap(),
            "rust",
            runner.identity(),
        )
        .unwrap();
        assert!(store.lookup(&key, runner.identity()).unwrap().is_none());
        assert!(
            store
                .frozen_contract(&key, runner.identity())
                .unwrap()
                .is_some()
        );
    }
}
