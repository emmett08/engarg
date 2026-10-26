//! Dissent-only repairs reject unrelated edits and retain the boundary across attempts.
use engarg::{
    Error,
    domain::{ArgumentCase, Request},
    engine::{Mode, RunConfig, run},
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};
use serde_json::{Value, json};
use std::sync::Mutex;

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/claim-only-case.json")).unwrap(),
    )
}

struct Model {
    drift_once: bool,
    calls: Mutex<Vec<CompletionRequest>>,
}

impl LanguageModel for Model {
    async fn complete(&self, prompt: CompletionRequest) -> Result<Completion, Error> {
        let data: Value = serde_json::from_str(&prompt.user)?;
        self.calls.lock().unwrap().push(prompt.clone());
        let mut case = fixture().1;
        let content = if matches!(prompt.role, Role::Synthesiser | Role::Adjudicator) {
            if prompt.role == Role::Adjudicator {
                case.warrants[0]
                    .rule
                    .push_str(" Preserve the specified runner configuration.");
            }
            if let Some(attempt) = data["repair_attempt"].as_u64() {
                assert_eq!(data["repair_scope"]["schema"], "engarg.repair-scope/1");
                let applications: Vec<_> = data["dissent_revision_candidates"]
                    .as_array().unwrap().iter()
                    .filter(|choice| !choice["changes"].as_array().unwrap().is_empty())
                    .map(|choice| json!({"objection_id":choice["objection_id"],
                        "origin_context_sha256":choice["original_context_sha256"],
                        "effect":"still_applies",
                        "rationale":"The applicability challenge remains open for the revised rule."}))
                    .collect();
                case.dissent_revisions = Some(serde_json::from_value(json!({
                    "schema":"engarg.dissent-revisions/1","applications":applications,
                }))?);
                if self.drift_once && attempt == 1 {
                    case.alternatives[0]
                        .distinguishing_test
                        .push_str(" An unrelated edit.");
                }
                if attempt == 2 {
                    assert!(
                        data["validation_errors"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|error| error.as_str().unwrap().contains("REPAIR_SCOPE_CHANGED"))
                    );
                    assert!(
                        !data["repair_scope"]["protected_restorations"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
            }
            serde_json::to_string(&case)?
        } else {
            json!({"findings":[],"objections":[],"evidence_gaps":[]}).to_string()
        };
        Ok(Completion {
            content,
            model: "scoped-repair-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

async fn analyse(drift_once: bool, max_repairs: u8) -> Result<engarg::engine::RunReport, Error> {
    let model = Model {
        drift_once,
        calls: Mutex::new(Vec::new()),
    };
    let result = run(
        &model,
        &fixture().0,
        &RunConfig {
            mode: Mode::Hybrid,
            max_repairs,
            max_calls: 6 + u32::from(max_repairs),
            ..RunConfig::default()
        },
    )
    .await;
    assert_eq!(
        model.calls.lock().unwrap().len(),
        6 + usize::from(max_repairs)
    );
    result
}

#[tokio::test]
async fn explicit_revision_with_open_dissent_is_accepted_within_scope() {
    let report = analyse(false, 1).await.unwrap();
    assert_eq!(
        report.argument.alternatives[0].distinguishing_test,
        fixture().1.alternatives[0].distinguishing_test
    );
}

#[tokio::test]
async fn valid_but_unrelated_edit_is_rejected() {
    assert!(
        matches!(analyse(true, 1).await, Err(Error::InvalidOutput(message))
        if message.contains("REPAIR_SCOPE_CHANGED"))
    );
}

#[tokio::test]
async fn second_repair_restores_protected_fields_without_waiving_dissent() {
    let report = analyse(true, 2).await.unwrap();
    assert_eq!(
        report.argument.alternatives[0].distinguishing_test,
        fixture().1.alternatives[0].distinguishing_test
    );
    assert!(
        report
            .argument
            .defeaters
            .iter()
            .any(|objection| objection.material
                && objection.status == engarg::domain::DefeaterStatus::Open)
    );
}
