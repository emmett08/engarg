//! Bounded, dependency-ordered orchestration with independent first-pass agents.

use std::{collections::HashMap, time::Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    Error,
    acquisition::{self, PlannedArgument},
    domain::{ArgumentCase, Request, case_schema, validate_request},
    output_validation::{self, RetainedObjection},
    prompts,
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};

mod context;
mod repair;
pub mod support;
pub use support::{RouteResolver, SupportPlanning, SupportRunReport, run_with_support};

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
    /// Optional maximum number of warrant layers on every root-to-ground route.
    #[serde(default)]
    pub max_argument_depth: Option<u8>,
    /// Extra validation-guided calls, reserved within max_calls before execution.
    #[serde(default)]
    pub max_repairs: u8,
    /// Optional operator-selected collector descriptions, retained for reproducible planning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_catalogue: Option<crate::collector_catalogue::CollectorCatalogue>,
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
            max_argument_depth: None,
            max_repairs: 0,
            collector_catalogue: None,
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
    #[serde(default)]
    pub requested_reasoning_effort: Option<String>,
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
    /// Missing in reports produced before generation-depth and repair controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_config: Option<RunConfig>,
    pub request: Request,
    pub argument: ArgumentCase,
    pub transcript: Vec<CallRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooling: Option<crate::tooling::ToolingReport>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Assessment {
    findings: Vec<String>,
    objections: Vec<String>,
    evidence_gaps: Vec<String>,
}

/// The same finite list bound applies to all specialist and final-critic assessments.
pub(crate) const MAX_ASSESSMENT_ENTRIES: usize = 32;

// Spell out Unicode White_Space rather than using \S: ECMAScript and Rust differ
// on NEL and BOM. This supported JSON Schema pattern matches str::trim exactly.
const ASSESSMENT_NONBLANK_PATTERN: &str =
    r"[^\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]";

fn assessment_schema() -> Value {
    let entries = json!({
        "type": "array",
        "maxItems": MAX_ASSESSMENT_ENTRIES,
        "items": {"type": "string", "pattern": ASSESSMENT_NONBLANK_PATTERN}
    });
    json!({
        "type": "object",
        "properties": {
            "findings": entries,
            "objections": entries,
            "evidence_gaps": entries
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
    for (field, values) in [
        ("findings", &result.findings),
        ("objections", &result.objections),
        ("evidence_gaps", &result.evidence_gaps),
    ] {
        if values.len() > MAX_ASSESSMENT_ENTRIES {
            return Err(Error::InvalidOutput(format!(
                "{} assessment.{field} contains {} entries (maximum {MAX_ASSESSMENT_ENTRIES})",
                record.role.as_str(),
                values.len()
            )));
        }
        if let Some(index) = values.iter().position(|value| value.trim().is_empty()) {
            return Err(Error::InvalidOutput(format!(
                "{} assessment.{field}[{index}] is blank (entry {} of {})",
                record.role.as_str(),
                index + 1,
                values.len()
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

fn planned_argument(record: &CallRecord) -> Result<PlannedArgument, Error> {
    serde_json::from_str(&record.response.content).map_err(|error| {
        Error::InvalidOutput(format!(
            "{} planned case is invalid JSON: {error}",
            record.role.as_str()
        ))
    })
}

fn tool_prompt(
    role: Role,
    mut data: Value,
    schema: Value,
    config: &RunConfig,
    tool_planning: bool,
) -> Result<CompletionRequest, Error> {
    let produces_case = matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator);
    let plans_tools = tool_planning
        && (produces_case || matches!(role, Role::EvidenceAuditor | Role::FinalCritic));
    if plans_tools {
        data["acquisition_tools"] =
            acquisition::catalogue_with(config.collector_catalogue.as_ref());
    }
    let schema = if tool_planning && produces_case {
        acquisition::planned_case_schema_with(config.collector_catalogue.as_ref())
    } else {
        schema
    };
    let mut request = prompt(role, data, schema, config)?;
    if plans_tools {
        request.system.push_str("\n\n");
        request.system.push_str(acquisition::PLANNING_INSTRUCTIONS);
    }
    if tool_planning && produces_case {
        request.system.push_str("\n\n");
        request.system.push_str(acquisition::ENVELOPE_INSTRUCTIONS);
    }
    Ok(request)
}

fn prompt(
    role: Role,
    mut data: Value,
    schema: Value,
    config: &RunConfig,
) -> Result<CompletionRequest, Error> {
    let mut system = prompts::system(role);
    if context::uses_references(&data) {
        system.push_str("\n\n");
        system.push_str(prompts::REFERENCE_CONTEXT);
    }
    if context::compact_target_contexts(&mut data, prompts::TARGET_CONTEXT_REFERENCES)? {
        system.push_str("\n\n");
        system.push_str(prompts::TARGET_CONTEXT_REFERENCES);
    }
    if let Some(depth) = config.max_argument_depth {
        system.push_str(&format!(
            "\n\nCONSTRUCTION LIMIT: Use at most {depth} warrant layers on every support path from current_claim or stronger_target to a ground, including unselected candidate routes. A direct claim-to-warrant-to-ground argument has depth 1; an intermediate subclaim adds one layer. Use assumed or missing grounds with concrete evidence obligations at the boundary. Keep every consequential condition and challenge explicit. This limit bounds decomposition, not evidential strength. Do not invent support, weaken validation, or conceal a necessary inference to meet it. Specialists must focus on the decisive arguments at this depth rather than recursively expanding remote possibilities."
        ));
    }
    Ok(CompletionRequest {
        role,
        system,
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
        "This is an LLM-constructed argument and assessment of its explicit assumptions, available evidence and explored alternatives. Required observations remain obligations until acquired and checked. Global optimality, premise truth and deductive soundness remain unverified.".to_owned(),
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
/// Repairs are disabled by default. Explicit repair allowances reserve calls
/// inside max_calls, and every returned candidate receives the same validation.
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
    run_internal(model, request, config, observer, false).await
}

/// Construct collector proposals with the argument, retaining the envelope in
/// the existing transcript while preserving the public report and case formats.
pub async fn run_with_tools<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
) -> Result<RunReport, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    run_internal(model, request, config, observer, true).await
}

async fn run_internal<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    tool_planning: bool,
) -> Result<RunReport, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    validate_request(request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    if let Some(catalogue) = &config.collector_catalogue {
        catalogue.validate()?;
    }
    if config
        .max_argument_depth
        .is_some_and(|depth| !(1..=16).contains(&depth))
        || config.max_repairs > 3
    {
        return Err(Error::InvalidInput(
            "argument-depth must be 1–16 and max-repairs must be 0–3".into(),
        ));
    }
    let required_calls = match config.mode {
        Mode::Hybrid => 6,
        Mode::Single => 1,
    } + u32::from(config.max_repairs);
    if config.max_calls < required_calls {
        return Err(Error::Budget(format!(
            "{} mode including the repair allowance requires {required_calls} calls but max_calls is {}",
            config.mode, config.max_calls
        )));
    }
    if config.max_input_bytes == 0 || config.max_response_bytes == 0 {
        return Err(Error::Budget("byte budgets must be positive".to_owned()));
    }

    let (mut transcript, mut retained_objections) = match config.mode {
        Mode::Single => {
            let call = execute(
                model,
                tool_prompt(
                    Role::Single,
                    json!({"request": request}),
                    case_schema(),
                    config,
                    tool_planning,
                )?,
                config,
                observer,
            )
            .await?;
            (vec![call], Vec::new())
        }
        Mode::Hybrid => hybrid(model, request, config, observer, tool_planning).await?,
    };
    let final_case = repair::validate_or_repair(
        model,
        request,
        config,
        observer,
        &mut transcript,
        &mut retained_objections,
        tool_planning,
    )
    .await?;
    Ok(RunReport {
        schema_version: "engarg.run.v2".to_owned(),
        execution_mode: "provider_calls".to_owned(),
        input_sha256: sha256_hex(serde_json::to_vec(request)?),
        provider_config: None,
        mode: config.mode,
        run_config: Some(config.clone()),
        request: request.clone(),
        argument: final_case,
        transcript,
        tooling: None,
        limitations: limitations(config.mode),
    })
}

async fn hybrid<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    tool_planning: bool,
) -> Result<(Vec<CallRecord>, Vec<RetainedObjection>), Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    let data = json!({"request": request});
    let schema = assessment_schema();
    // Construct every first-pass request before starting any call. No specialist
    // can observe another specialist's response through this data flow.
    let constructor = prompt(Role::Constructor, data.clone(), schema.clone(), config)?;
    let sceptic = prompt(Role::Sceptic, data.clone(), schema.clone(), config)?;
    let auditor = tool_prompt(
        Role::EvidenceAuditor,
        data,
        schema.clone(),
        config,
        tool_planning,
    )?;
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
    let mut assessments = Vec::new();
    for (record, prefix) in initial.iter().zip(["constructor", "sceptic", "auditor"]) {
        assessments.push(context::assessment(
            record.role,
            prefix,
            parse_assessment(record)?,
            &mut retained_objections,
        ));
    }
    let synthesis = execute(
        model,
        tool_prompt(
            Role::Synthesiser,
            json!({"request": request, "independent_assessments": assessments, "retained_objections": retained_objections}),
            case_schema(),
            config,
            tool_planning,
        )?,
        config,
        observer,
    )
    .await?;
    let (draft, draft_acquisition_plan) = if tool_planning {
        let planned = planned_argument(&synthesis)?;
        (planned.argument, planned.acquisition_plan)
    } else {
        (argument(&synthesis)?, Vec::new())
    };
    let mut draft_validation_errors = output_validation::errors(
        request,
        &draft,
        &retained_objections,
        config.max_argument_depth,
    );
    if tool_planning {
        draft_validation_errors.extend(acquisition::validate_selected(
            &draft,
            &draft_acquisition_plan,
            config.collector_catalogue.as_ref(),
        ));
    }
    output_validation::retain_objections(&draft, synthesis.role, &mut retained_objections);
    let draft_view = context::argument(request, &draft, &retained_objections)?;
    let mut critique_data = json!({"request": request, "independent_assessments": assessments, "draft": draft_view, "retained_objections": retained_objections, "draft_validation_errors": draft_validation_errors});
    if tool_planning {
        critique_data["draft_acquisition_plan"] = serde_json::to_value(&draft_acquisition_plan)?;
    }
    let critique = execute(
        model,
        tool_prompt(
            Role::FinalCritic,
            critique_data,
            schema,
            config,
            tool_planning,
        )?,
        config,
        observer,
    )
    .await?;
    let final_critique = context::assessment(
        Role::FinalCritic,
        "critic",
        parse_assessment(&critique)?,
        &mut retained_objections,
    );
    let mut adjudication_data = json!({
        "request": request,
        "independent_assessments": assessments,
        "draft": draft_view,
        "draft_validation_errors": draft_validation_errors,
        "retained_objections": retained_objections,
        "final_critique": final_critique["assessment"]
    });
    if tool_planning {
        adjudication_data["draft_acquisition_plan"] =
            serde_json::to_value(&draft_acquisition_plan)?;
    }
    let adjudication = execute(
        model,
        tool_prompt(
            Role::Adjudicator,
            adjudication_data,
            case_schema(),
            config,
            tool_planning,
        )?,
        config,
        observer,
    )
    .await?;
    let mut transcript = initial;
    transcript.extend([synthesis, critique, adjudication]);
    Ok((transcript, retained_objections))
}

#[cfg(test)]
mod tests {
    use crate::domain::validate_case;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use super::*;

    const ASSESSMENT_ROLES: [Role; 4] = [
        Role::Constructor,
        Role::Sceptic,
        Role::EvidenceAuditor,
        Role::FinalCritic,
    ];
    const ASSESSMENT_FIELDS: [&str; 3] = ["findings", "objections", "evidence_gaps"];

    fn assessment_record(role: Role, value: &Value) -> CallRecord {
        let content = value.to_string();
        CallRecord {
            role,
            prompt: CompletionRequest {
                role,
                system: String::new(),
                user: String::new(),
                schema: assessment_schema(),
            },
            response_content_bytes: content.len(),
            response_content_sha256: sha256_hex(content.as_bytes()),
            response_content_omitted: false,
            response: Completion {
                content,
                model: "assessment-regression-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            },
            elapsed_ms: 0,
        }
    }

    #[test]
    fn assessment_schema_and_validator_agree_at_each_list_boundary_for_every_role() {
        let schema = assessment_schema();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for role in ASSESSMENT_ROLES {
            for field in ASSESSMENT_FIELDS {
                for count in [0, MAX_ASSESSMENT_ENTRIES, MAX_ASSESSMENT_ENTRIES + 1] {
                    let mut value = json!({"findings": [], "objections": [], "evidence_gaps": []});
                    value[field] = json!(
                        (0..count)
                            .map(|index| format!(
                                "  Complete obligation {index} with its qualification.\n"
                            ))
                            .collect::<Vec<_>>()
                    );
                    let accepted = count <= MAX_ASSESSMENT_ENTRIES;
                    assert_eq!(
                        validator.is_valid(&value),
                        accepted,
                        "{role} {field} {count}"
                    );
                    let record = assessment_record(role, &value);
                    let parsed = parse_assessment(&record);
                    assert_eq!(parsed.is_ok(), accepted, "{role} {field} {count}");
                    if let Ok(parsed) = parsed {
                        // Admission preserves every entry and its exact spelling.
                        for (name, entries) in [
                            ("findings", parsed.findings),
                            ("objections", parsed.objections),
                            ("evidence_gaps", parsed.evidence_gaps),
                        ] {
                            assert_eq!(json!(entries), value[name]);
                        }
                    } else {
                        assert_eq!(
                            parsed.unwrap_err().to_string(),
                            format!(
                                "invalid model output: {} assessment.{field} contains {count} entries (maximum {MAX_ASSESSMENT_ENTRIES})",
                                role.as_str()
                            )
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn final_critic_rejects_41_evidence_gaps_with_the_field_count_and_bound() {
        // Reproduce the reported list counts without retaining the private argument.
        let value = json!({
            "findings": vec!["A qualified finding."; 11],
            "objections": vec!["A material challenge with its target."; 28],
            "evidence_gaps": vec!["A distinct evidence obligation."; 41],
        });
        let validator = jsonschema::validator_for(&assessment_schema()).unwrap();
        assert!(!validator.is_valid(&value));
        let record = assessment_record(Role::FinalCritic, &value);
        assert_eq!(
            parse_assessment(&record).unwrap_err().to_string(),
            "invalid model output: final_critic assessment.evidence_gaps contains 41 entries (maximum 32)"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&record.response.content).unwrap(),
            value
        );
    }

    #[test]
    fn assessment_nonblank_pattern_matches_unicode_whitespace_admission() {
        let validator = jsonschema::validator_for(&assessment_schema()).unwrap();
        // Unicode White_Space includes NEL. BOM, zero-width space and the
        // Mongolian vowel separator remain nonblank under the existing rule.
        let whitespace: String = [
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x20, 0x85, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003,
            0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f,
            0x3000,
        ]
        .into_iter()
        .map(|code| char::from_u32(code).unwrap())
        .collect();
        let mut examples = vec![(String::new(), false), (whitespace.clone(), false)];
        examples.extend(
            whitespace
                .chars()
                .map(|character| (character.to_string(), false)),
        );
        for text in [
            "A complete finding.",
            "line one\nline two",
            "\u{feff}",
            "\u{200b}",
            "\u{180e}",
            "\u{1f50e}",
        ] {
            examples.push((format!("{whitespace}{text}{whitespace}"), true));
        }
        for field in ASSESSMENT_FIELDS {
            for (text, accepted) in &examples {
                let mut value = json!({"findings": [], "objections": [], "evidence_gaps": []});
                value[field] = json!([text]);
                assert_eq!(!text.trim().is_empty(), *accepted, "{text:?}");
                assert_eq!(validator.is_valid(&value), *accepted, "{field} {text:?}");
                assert_eq!(
                    parse_assessment(&assessment_record(Role::FinalCritic, &value)).is_ok(),
                    *accepted,
                    "{field} {text:?}"
                );
            }
        }
    }

    #[test]
    fn blank_assessment_errors_identify_the_role_field_and_entry_without_echoing_content() {
        for role in ASSESSMENT_ROLES {
            for field in ASSESSMENT_FIELDS {
                let mut value = json!({"findings": [], "objections": [], "evidence_gaps": []});
                value[field] = json!([
                    "Private argument content.",
                    " \u{0085}\n",
                    "Retained qualification."
                ]);
                let error = parse_assessment(&assessment_record(role, &value))
                    .unwrap_err()
                    .to_string();
                assert_eq!(
                    error,
                    format!(
                        "invalid model output: {} assessment.{field}[1] is blank (entry 2 of 3)",
                        role.as_str()
                    )
                );
                assert!(!error.contains("Private argument"));
            }
        }
    }

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
                reasoning_effort: None,
            })
        }
    }

    fn request() -> Request {
        serde_json::from_str(include_str!("../examples/request.json")).unwrap()
    }

    struct ConstructedCaseModel {
        case: ArgumentCase,
        inner: MockModel,
    }

    impl LanguageModel for ConstructedCaseModel {
        async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
            let role = request.role;
            let mut completion = self.inner.complete(request).await?;
            if matches!(role, Role::Synthesiser | Role::Adjudicator | Role::Single) {
                completion.content = serde_json::to_string(&self.case)?;
            }
            Ok(completion)
        }
    }

    fn claim_only_candidate() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap(),
            serde_json::from_str(include_str!("../examples/claim-only-case.json")).unwrap(),
        )
    }

    fn assessment_without_argument(mut case: ArgumentCase) -> ArgumentCase {
        case.status = crate::domain::CaseStatus::Unsupported;
        case.current_claim.text =
            "An argument for the supplied claim remains to be constructed.".into();
        case.current_claim.qualifier = "An assessment and investigation plan only.".into();
        case.grounds.clear();
        case.warrants.clear();
        case.subclaims.clear();
        case.compositions.clear();
        case.assumptions.clear();
        case.defeaters.clear();
        case.alternatives.clear();
        case.evidence_plan.truncate(1);
        case.evidence_plan[0].target_id = "current_claim".into();
        case.evidence_plan[0].enables_claim = "current_claim".into();
        case.reading
            .retain(|section| section.claim_id == "current_claim");
        for paragraph in &mut case.reading[0].paragraphs {
            paragraph.text =
                "The requested claim requires an argument and investigation before reliance."
                    .into();
            paragraph.reference_ids = vec!["current_claim".into()];
        }
        case
    }

    #[tokio::test]
    async fn generation_rejects_an_empty_assessment_graph_in_both_modes() {
        let (request, candidate) = claim_only_candidate();
        let case = assessment_without_argument(candidate);
        // Assessment-only cases remain valid for import and presentation.
        assert_eq!(validate_case(&request, &case), Ok(()));
        for mode in [Mode::Single, Mode::Hybrid] {
            let model = ConstructedCaseModel {
                case: case.clone(),
                inner: MockModel::new(false),
            };
            let config = RunConfig {
                mode,
                ..RunConfig::default()
            };
            assert!(matches!(run(&model, &request, &config).await,
                    Err(Error::InvalidOutput(message)) if message.contains("generation must construct at least one argument")));
            assert_eq!(
                model.inner.calls.lock().unwrap().len(),
                if mode == Mode::Single { 1 } else { 6 }
            );
        }
    }

    #[tokio::test]
    async fn generation_accepts_a_substantive_claim_only_conditional_argument() {
        let (request, case) = claim_only_candidate();
        assert!(request.evidence.is_empty());
        for mode in [Mode::Single, Mode::Hybrid] {
            let model = ConstructedCaseModel {
                case: case.clone(),
                inner: MockModel::new(false),
            };
            let report = run(
                &model,
                &request,
                &RunConfig {
                    mode,
                    ..RunConfig::default()
                },
            )
            .await
            .unwrap();
            assert_eq!(
                report.argument.status,
                crate::domain::CaseStatus::Conditional
            );
            assert!(!report.argument.grounds.is_empty());
            assert!(!report.argument.warrants.is_empty());
            assert!(!report.argument.evidence_plan.is_empty());
        }
    }

    #[tokio::test]
    async fn a_deeper_rerun_changes_the_generation_limit_without_changing_evidence_status() {
        let (request, case) = claim_only_candidate();
        let model = ConstructedCaseModel {
            case,
            inner: MockModel::new(false),
        };
        let shallow = RunConfig {
            mode: Mode::Single,
            max_calls: 1,
            max_argument_depth: Some(1),
            ..RunConfig::default()
        };
        assert!(matches!(run(&model, &request, &shallow).await,
            Err(Error::InvalidOutput(message)) if message.contains("argument depth is 2")
        ));
        let deeper = RunConfig {
            max_argument_depth: Some(2),
            ..shallow
        };
        let report = run(&model, &request, &deeper).await.unwrap();
        assert_eq!(
            report.argument.status,
            crate::domain::CaseStatus::Conditional
        );
        assert!(report.request.evidence.is_empty());
        assert_eq!(report.run_config.unwrap().max_argument_depth, Some(2));
    }

    #[tokio::test]
    async fn invalid_depth_and_repair_limits_fail_before_model_calls() {
        let model = MockModel::new(false);
        for config in [
            RunConfig {
                max_argument_depth: Some(0),
                ..RunConfig::default()
            },
            RunConfig {
                max_argument_depth: Some(17),
                ..RunConfig::default()
            },
            RunConfig {
                max_repairs: 4,
                ..RunConfig::default()
            },
        ] {
            assert!(matches!(
                run(&model, &request(), &config).await,
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(model.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unused_repair_allowance_makes_no_extra_calls_and_old_configs_keep_defaults() {
        let config: RunConfig = serde_json::from_value(json!({
            "mode": "hybrid", "max_calls": 6,
            "max_input_bytes": 1_048_576, "max_response_bytes": 131_072,
        }))
        .unwrap();
        assert_eq!(config.max_argument_depth, None);
        assert_eq!(config.max_repairs, 0);
        let model = MockModel::new(false);
        let report = run(
            &model,
            &request(),
            &RunConfig {
                max_repairs: 3,
                max_calls: 9,
                ..config
            },
        )
        .await
        .unwrap();
        assert_eq!(report.transcript.len(), 6);
        assert_eq!(model.calls.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn construction_allows_a_reasoned_qualification_of_an_overstated_original_claim() {
        let (mut request, mut case) = claim_only_candidate();
        request.claim = "Parallel scheduling always shortens every CI pipeline.".into();
        case.original_claim = request.claim.clone();
        case.current_claim.text = "A speedup remains conditional on useful critical-path overlap and equivalent execution of the required checks.".into();
        case.current_claim.qualifier = "The universal original claim requires qualification; the represented mechanism applies only when its explicit conditions hold.".into();
        assert_eq!(validate_case(&request, &case), Ok(()));
        let model = ConstructedCaseModel {
            case,
            inner: MockModel::new(false),
        };
        let report = run(
            &model,
            &request,
            &RunConfig {
                mode: Mode::Single,
                max_calls: 1,
                ..RunConfig::default()
            },
        )
        .await
        .unwrap();
        assert_ne!(report.argument.current_claim.text, request.claim);
        assert_eq!(
            report.argument.status,
            crate::domain::CaseStatus::Conditional
        );
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
        for call in calls
            .iter()
            .filter(|call| ASSESSMENT_ROLES.contains(&call.role))
        {
            assert_eq!(call.schema, assessment_schema(), "{}", call.role);
        }
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
    fn repeated_contexts_shrink_the_typed_request_with_actual_primer_and_escaping() {
        let snapshot = json!({
            "scope":{"configuration":"Quoted \"scope\" \\ path\r\nα\n".repeat(100)},
            "target":{"id":"W1","rule":"Preserve the exact inference."},
            "related_targets":{"G1":{"statement":"An exact premise."}}
        });
        let original = json!({
            "request":{"context":{"retained_target_contexts":"literal source data"}},
            "retained_objections":[
                {"id":"critic-1","identity":{"target_id":"W1","target_context":snapshot}},
                {"id":"critic-2","identity":{"target_id":"W1","target_context":snapshot}}
            ]
        });
        let compact = prompt(
            Role::Adjudicator,
            original.clone(),
            case_schema(),
            &RunConfig {
                max_argument_depth: Some(2),
                ..RunConfig::default()
            },
        )
        .unwrap();
        let instruction = format!("\n\n{}", prompts::TARGET_CONTEXT_REFERENCES);
        assert_eq!(compact.system.matches(&instruction).count(), 1);
        let expanded_request = CompletionRequest {
            role: compact.role,
            system: compact.system.replacen(&instruction, "", 1),
            user: serde_json::to_string(&original).unwrap(),
            schema: compact.schema.clone(),
        };
        assert!(
            serde_json::to_vec(&compact).unwrap().len()
                < serde_json::to_vec(&expanded_request).unwrap().len()
        );
        let mut data: Value = serde_json::from_str(&compact.user).unwrap();
        let table = data
            .as_object_mut()
            .unwrap()
            .remove("retained_target_contexts")
            .unwrap();
        for objection in data["retained_objections"].as_array_mut().unwrap() {
            let identity = objection["identity"].as_object_mut().unwrap();
            let reference = identity.remove("target_context_ref").unwrap();
            identity.insert(
                "target_context".into(),
                table[reference.as_str().unwrap()].clone(),
            );
        }
        assert_eq!(data, original);
    }
    #[test]
    fn audit_hash_matches_sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
