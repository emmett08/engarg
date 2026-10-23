//! Bounded, dependency-ordered orchestration with independent first-pass agents.

use std::{collections::HashMap, time::Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    Error,
    domain::{ArgumentCase, Request, case_schema, validate_case, validate_request},
    prompts,
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};

/// Architecture selected for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Three independent specialists, synthesis, final critique and adjudication.
    Hybrid,
    /// A one-call baseline for evaluation and inexpensive straightforward cases.
    Single,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hybrid" => Ok(Self::Hybrid),
            "single" => Ok(Self::Single),
            _ => Err("mode must be hybrid or single".to_owned()),
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Hybrid => "hybrid",
            Self::Single => "single",
        })
    }
}

/// Logical-call and byte budgets. HTTP time and output-token budgets belong to the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub mode: Mode,
    pub max_calls: u32,
    /// Maximum encoded bytes of any complete provider request, including its schema.
    pub max_input_bytes: usize,
    /// Maximum bytes in any returned model content.
    pub max_response_bytes: usize,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Hybrid,
            max_calls: 6,
            max_input_bytes: 1_048_576,
            max_response_bytes: 131_072,
        }
    }
}

/// A completed logical call without API credentials. Oversized content is omitted
/// explicitly while its original size and digest are retained for the audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallRecord {
    pub role: Role,
    pub prompt: CompletionRequest,
    pub response: Completion,
    pub response_content_bytes: usize,
    pub response_content_sha256: String,
    pub response_content_omitted: bool,
    pub elapsed_ms: u64,
}

/// Requested provider settings, kept separately from actual completion metadata.
/// Credentials and confidential remote error bodies are deliberately excluded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRunConfig {
    pub endpoint: String,
    pub api: String,
    pub default_model: String,
    pub requested_service_tier: Option<String>,
    pub role_models: HashMap<Role, String>,
    pub max_output_tokens: u32,
    pub request_timeout_secs: u64,
    pub timeout_secs: u64,
}

/// A reproducible argument assessment and the provider exchanges used to produce it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunReport {
    pub schema_version: String,
    pub execution_mode: String,
    pub input_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_config: Option<ProviderRunConfig>,
    pub mode: Mode,
    pub request: Request,
    pub argument: ArgumentCase,
    pub transcript: Vec<CallRecord>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Assessment {
    findings: Vec<String>,
    objections: Vec<String>,
    evidence_gaps: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RetainedObjection {
    id: String,
    challenge: String,
    source_role: Role,
}

fn assessment_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "findings": {"type":"array", "items":{"type":"string"}},
            "objections": {"type":"array", "items":{"type":"string"}},
            "evidence_gaps": {"type":"array", "items":{"type":"string"}}
        },
        "required": ["findings", "objections", "evidence_gaps"],
        "additionalProperties": false
    })
}

fn parse_assessment(record: &CallRecord) -> Result<Assessment, Error> {
    let result: Assessment = serde_json::from_str(&record.response.content).map_err(|err| {
        Error::InvalidOutput(format!(
            "{} assessment is invalid JSON: {err}",
            record.role.as_str()
        ))
    })?;
    for values in [&result.findings, &result.objections, &result.evidence_gaps] {
        if values.len() > 32 || values.iter().any(|value| value.trim().is_empty()) {
            return Err(Error::InvalidOutput(format!(
                "{} assessment lists must contain at most 32 nonblank entries",
                record.role.as_str()
            )));
        }
    }
    Ok(result)
}

fn argument(record: &CallRecord) -> Result<ArgumentCase, Error> {
    serde_json::from_str(&record.response.content).map_err(|err| {
        Error::InvalidOutput(format!(
            "{} case is invalid JSON: {err}",
            record.role.as_str()
        ))
    })
}

fn prompt(role: Role, data: Value, schema: Value) -> Result<CompletionRequest, Error> {
    Ok(CompletionRequest {
        role,
        system: prompts::system(role),
        user: serde_json::to_string(&data)?,
        schema,
    })
}

fn check_prompt(request: &CompletionRequest, config: &RunConfig) -> Result<(), Error> {
    let bytes = serde_json::to_vec(request)?.len();
    if bytes > config.max_input_bytes {
        return Err(Error::Budget(format!(
            "{} prompt uses {bytes} encoded bytes, exceeding {}",
            request.role.as_str(),
            config.max_input_bytes
        )));
    }
    Ok(())
}

async fn execute<M: LanguageModel, O>(
    model: &M,
    request: CompletionRequest,
    config: &RunConfig,
    observer: &O,
) -> Result<CallRecord, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    check_prompt(&request, config)?;
    let started = Instant::now();
    let mut response = model.complete(request.clone()).await?;
    let response_content_bytes = response.content.len();
    let response_content_sha256 = sha256_hex(response.content.as_bytes());
    let response_content_omitted = response_content_bytes > config.max_response_bytes;
    if response_content_omitted {
        // A metadata-only receipt preserves the paid completion without retaining
        // unbounded content or passing incomplete JSON into later reasoning.
        response.content.clear();
    }
    let record = CallRecord {
        role: request.role,
        prompt: request,
        response,
        response_content_bytes,
        response_content_sha256,
        response_content_omitted,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    };
    observer(&record)?;
    if response_content_omitted {
        return Err(Error::Budget(format!(
            "{} response uses {response_content_bytes} content bytes, exceeding {}; audit content omitted",
            record.role.as_str(),
            config.max_response_bytes
        )));
    }
    Ok(record)
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn limitations(mode: Mode) -> Vec<String> {
    let mut values = vec![
        "This is an LLM assessment relative to supplied evidence and explored alternatives. It does not establish global optimality, premise truth or deductive soundness.".to_owned(),
        "No source retrieval, independent evidence verification, experiment execution or proof checking was performed by this workflow.".to_owned(),
        "Deterministic validation checks structure and reference integrity. It does not establish semantic sufficiency or the quality of the proposed tests.".to_owned(),
        "Agent agreement is not evidence. Related models can share errors and the adjudicator can retain bias.".to_owned(),
        "Token counts are recorded when the provider supplies them; monetary cost and calibrated confidence are not inferred.".to_owned(),
        "Untrusted data is separated from fixed instructions, but model compliance and prompt-injection resistance remain best-effort.".to_owned(),
    ];
    if mode == Mode::Single {
        values.push("Single mode has no independently executed specialist review.".to_owned());
    }
    values
}

/// Run the fixed workflow. Insufficient call budgets fail before any provider call.
///
/// There are no automatic content repairs, retries or extra debate rounds. Failure
/// is explicit rather than silently consuming another paid model call.
pub async fn run<M: LanguageModel>(
    model: &M,
    request: &Request,
    config: &RunConfig,
) -> Result<RunReport, Error> {
    run_with_observer(model, request, config, &|_: &CallRecord| Ok(())).await
}

/// Run with a completion observer for incremental audit persistence.
///
/// The observer executes immediately after each completion and before parsing or
/// later calls. Oversized response content is omitted with size/digest metadata. It can retain completed exchanges even if
/// adjudication or validation fails. The observer must be concurrency-safe.
pub async fn run_with_observer<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
) -> Result<RunReport, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    let required_calls = match config.mode {
        Mode::Hybrid => 6,
        Mode::Single => 1,
    };
    if config.max_calls < required_calls {
        return Err(Error::Budget(format!(
            "{} mode requires {required_calls} calls but max_calls is {}",
            config.mode, config.max_calls
        )));
    }
    if config.max_input_bytes == 0 || config.max_response_bytes == 0 {
        return Err(Error::Budget("byte budgets must be positive".to_owned()));
    }

    let (final_case, transcript) = match config.mode {
        Mode::Single => {
            let call = execute(
                model,
                prompt(Role::Single, json!({"request": request}), case_schema())?,
                config,
                observer,
            )
            .await?;
            (argument(&call)?, vec![call])
        }
        Mode::Hybrid => hybrid(model, request, config, observer).await?,
    };
    validate_case(request, &final_case)
        .map_err(|errors| Error::InvalidOutput(errors.join("; ")))?;
    Ok(RunReport {
        schema_version: "engarg.run.v2".to_owned(),
        execution_mode: "provider_calls".to_owned(),
        input_sha256: sha256_hex(serde_json::to_vec(request)?),
        provider_config: None,
        mode: config.mode,
        request: request.clone(),
        argument: final_case,
        transcript,
        limitations: limitations(config.mode),
    })
}

async fn hybrid<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
) -> Result<(ArgumentCase, Vec<CallRecord>), Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    let data = json!({"request": request});
    let schema = assessment_schema();
    // Construct every first-pass request before starting any call. No specialist
    // can observe another specialist's response through this data flow.
    let constructor = prompt(Role::Constructor, data.clone(), schema.clone())?;
    let sceptic = prompt(Role::Sceptic, data.clone(), schema.clone())?;
    let auditor = prompt(Role::EvidenceAuditor, data, schema.clone())?;
    for first_pass in [&constructor, &sceptic, &auditor] {
        check_prompt(first_pass, config)?;
    }
    let (constructor, sceptic, auditor) = tokio::join!(
        execute(model, constructor, config, observer),
        execute(model, sceptic, config, observer),
        execute(model, auditor, config, observer),
    );
    let initial = vec![constructor?, sceptic?, auditor?];
    let mut retained_objections = Vec::<RetainedObjection>::new();
    for (record, prefix) in initial.iter().zip(["constructor", "sceptic", "auditor"]) {
        let assessment = parse_assessment(record)?;
        for (index, challenge) in assessment.objections.into_iter().enumerate() {
            retained_objections.push(RetainedObjection {
                id: format!("{prefix}-{}", index + 1),
                challenge,
                source_role: record.role,
            });
        }
    }
    let assessments: Vec<Value> = initial
        .iter()
        .map(|record| {
            Ok(json!({
                "role": record.role,
                "assessment": serde_json::from_str::<Value>(&record.response.content)?
            }))
        })
        .collect::<Result<_, serde_json::Error>>()?;
    let synthesis = execute(
        model,
        prompt(
            Role::Synthesiser,
            json!({"request": request, "independent_assessments": assessments, "retained_objections": retained_objections}),
            case_schema(),
        )?,
        config,
        observer,
    )
    .await?;
    let draft = argument(&synthesis)?;
    let critique = execute(
        model,
        prompt(
            Role::FinalCritic,
            json!({"request": request, "independent_assessments": assessments, "draft": draft, "retained_objections": retained_objections}),
            schema,
        )?,
        config,
        observer,
    )
    .await?;
    let final_critique = parse_assessment(&critique)?;
    for (index, challenge) in final_critique.objections.into_iter().enumerate() {
        retained_objections.push(RetainedObjection {
            id: format!("critic-{}", index + 1),
            challenge,
            source_role: Role::FinalCritic,
        });
    }
    let adjudication = execute(
        model,
        prompt(
            Role::Adjudicator,
            json!({
                "request": request,
                "independent_assessments": assessments,
                "draft": draft,
                "retained_objections": retained_objections,
                "final_critique": serde_json::from_str::<Value>(&critique.response.content)?
            }),
            case_schema(),
        )?,
        config,
        observer,
    )
    .await?;
    let final_case = argument(&adjudication)?;
    for objection in retained_objections {
        let id = objection.id;
        let challenge = objection.challenge;
        if !final_case
            .defeaters
            .iter()
            .any(|defeater| defeater.id == id && defeater.challenge == challenge)
        {
            return Err(Error::InvalidOutput(format!(
                "adjudicator omitted or altered retained objection {id}"
            )));
        }
    }
    let mut transcript = initial;
    transcript.extend([synthesis, critique, adjudication]);
    Ok((final_case, transcript))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use super::*;

    struct MockModel {
        calls: Mutex<Vec<CompletionRequest>>,
        active: AtomicUsize,
        peak: AtomicUsize,
        critic_objection: bool,
        initial_objection: bool,
    }

    impl MockModel {
        fn new(critic_objection: bool) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                active: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                critic_objection,
                initial_objection: false,
            }
        }
    }

    impl LanguageModel for MockModel {
        async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
            let role = request.role;
            self.calls.lock().unwrap().push(request);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(5)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            let content = if matches!(role, Role::Synthesiser | Role::Adjudicator | Role::Single) {
                include_str!("../examples/case.json").to_owned()
            } else {
                json!({
                    "findings": ["No independently verified evidence was supplied."],
                    "objections": if role == Role::FinalCritic && self.critic_objection {
                        vec!["The proposed test omits a comparator."]
                    } else if role == Role::Sceptic && self.initial_objection {
                        vec!["The source evidence covers only one workload."]
                    } else {
                        Vec::<&str>::new()
                    },
                    "evidence_gaps": ["A bounded comparative test is needed."]
                })
                .to_string()
            };
            Ok(Completion {
                content,
                model: "deterministic-test-fixture".to_owned(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
            })
        }
    }

    fn request() -> Request {
        serde_json::from_str(include_str!("../examples/request.json")).unwrap()
    }

    #[tokio::test]
    async fn hybrid_executes_six_calls_with_independent_first_passes_and_complete_adjudication() {
        let model = MockModel::new(false);
        let result = run(&model, &request(), &RunConfig::default())
            .await
            .unwrap();
        assert_eq!(result.transcript.len(), 6);
        assert_eq!(result.schema_version, "engarg.run.v2");
        let graph = serde_json::to_value(&result.argument).unwrap();
        assert!(!graph["reading"].as_array().unwrap().is_empty());
        assert!(!graph["compositions"].as_array().unwrap().is_empty());
        assert_eq!(model.peak.load(Ordering::SeqCst), 3);
        let calls = model.calls.lock().unwrap();
        for call in &calls[..3] {
            let data: Value = serde_json::from_str(&call.user).unwrap();
            assert!(data.get("request").is_some());
            assert!(data.get("independent_assessments").is_none());
            assert!(data.get("draft").is_none());
        }
        let last = calls.last().unwrap();
        assert_eq!(last.role, Role::Adjudicator);
        let data: Value = serde_json::from_str(&last.user).unwrap();
        assert_eq!(data["independent_assessments"].as_array().unwrap().len(), 3);
        assert_eq!(data["request"], serde_json::to_value(request()).unwrap());
        assert!(data.get("draft").is_some());
        assert!(data.get("final_critique").is_some());
        assert_eq!(data["draft"]["reading"], graph["reading"]);
        assert_eq!(data["draft"]["compositions"], graph["compositions"]);
        for property in ["reading", "compositions"] {
            assert!(last.schema["properties"].get(property).is_some());
            assert!(
                last.schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value == property)
            );
        }
    }

    #[tokio::test]
    async fn call_budget_is_checked_before_any_provider_call() {
        let model = MockModel::new(false);
        let config = RunConfig {
            max_calls: 5,
            ..RunConfig::default()
        };
        assert!(matches!(
            run(&model, &request(), &config).await,
            Err(Error::Budget(_))
        ));
        assert!(model.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn byte_budget_prevents_provider_call() {
        let model = MockModel::new(false);
        let config = RunConfig {
            max_input_bytes: 1,
            ..RunConfig::default()
        };
        assert!(matches!(
            run(&model, &request(), &config).await,
            Err(Error::Budget(_))
        ));
        assert!(model.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn omitted_critic_objection_is_rejected_without_repair_calls() {
        let model = MockModel::new(true);
        let result = run(&model, &request(), &RunConfig::default()).await;
        assert!(
            matches!(result, Err(Error::InvalidOutput(message)) if message.contains("critic-1"))
        );
        assert_eq!(model.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn single_mode_uses_only_one_call() {
        let model = MockModel::new(false);
        let config = RunConfig {
            mode: Mode::Single,
            max_calls: 1,
            ..RunConfig::default()
        };
        let report = run(&model, &request(), &config).await.unwrap();
        assert_eq!(report.transcript.len(), 1);
        assert_eq!(report.transcript[0].role, Role::Single);
        assert_eq!(model.peak.load(Ordering::SeqCst), 1);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value.get("provider_config").is_none());
        let restored: RunReport = serde_json::from_value(value).unwrap();
        assert!(restored.provider_config.is_none());
    }
    #[tokio::test]
    async fn initial_dissent_cannot_disappear_when_final_critic_is_silent() {
        let mut model = MockModel::new(false);
        model.initial_objection = true;
        let result = run(&model, &request(), &RunConfig::default()).await;
        assert!(
            matches!(result, Err(Error::InvalidOutput(message)) if message.contains("sceptic-1"))
        );
        assert_eq!(model.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn observer_retains_completed_calls_when_final_validation_fails() {
        let model = MockModel::new(true);
        let observed = Mutex::new(Vec::new());
        let observer = |record: &CallRecord| {
            observed.lock().unwrap().push(record.clone());
            Ok(())
        };
        let result = run_with_observer(&model, &request(), &RunConfig::default(), &observer).await;
        assert!(matches!(result, Err(Error::InvalidOutput(_))));
        let records = observed.lock().unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(records.last().unwrap().role, Role::Adjudicator);
        assert!(!records.last().unwrap().response.content.is_empty());
    }
    #[tokio::test]
    async fn overlarge_completions_emit_metadata_only_audit_receipts() {
        let model = MockModel::new(false);
        let config = RunConfig {
            max_response_bytes: 1,
            ..RunConfig::default()
        };
        let observed = Mutex::new(Vec::new());
        let observer = |record: &CallRecord| {
            observed.lock().unwrap().push(record.clone());
            Ok(())
        };
        let result = run_with_observer(&model, &request(), &config, &observer).await;
        assert!(matches!(result, Err(Error::Budget(_))));
        let records = observed.lock().unwrap();
        assert_eq!(records.len(), 3);
        for record in records.iter() {
            assert!(record.response_content_omitted);
            assert!(record.response_content_bytes > 1);
            assert!(record.response.content.is_empty());
            assert_eq!(record.response_content_sha256.len(), 64);
        }
    }
    #[test]
    fn audit_hash_matches_sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
