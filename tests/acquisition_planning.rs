use std::sync::Mutex;

use engarg::{
    Error,
    acquisition::{AcquisitionPlan, PlannedArgument, parse_completion, validate},
    domain::{ArgumentCase, Request},
    engine::{Mode, RunConfig, run, run_with_tools},
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde_json::{Value, json};

struct Model {
    calls: Mutex<Vec<CompletionRequest>>,
    omit_plan_until_repair: bool,
    legacy_only: bool,
}

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/claim-only-case.json")).unwrap(),
    )
}

impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let planned = request.schema["properties"].get("argument").is_some();
        let direct_case = request.schema["properties"].get("current_claim").is_some();
        let data: Value = serde_json::from_str(&request.user)?;
        self.calls.lock().unwrap().push(request);
        let content = if planned || direct_case {
            let (_, case) = fixture();
            if planned && !self.legacy_only {
                let mut plans = case
                    .evidence_plan
                    .iter()
                    .map(|need| AcquisitionPlan {
                        input_preparation: None,
                        need_id: need.id.clone(),
                        tool_id: "custom".into(),
                        arguments: vec![],
                        unresolved_arguments: vec!["adapter".into()],
                    })
                    .collect::<Vec<_>>();
                if self.omit_plan_until_repair && data.get("repair_attempt").is_none() {
                    plans.pop();
                }
                serde_json::to_string(&PlannedArgument {
                    argument: case,
                    acquisition_plan: plans,
                })?
            } else {
                serde_json::to_string(&case)?
            }
        } else {
            json!({"findings":[],"objections":[],"evidence_gaps":[]}).to_string()
        };
        Ok(Completion {
            content,
            model: "fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

fn model(omit_plan_until_repair: bool, legacy_only: bool) -> Model {
    Model {
        calls: Mutex::new(vec![]),
        omit_plan_until_repair,
        legacy_only,
    }
}

#[tokio::test]
async fn tools_are_planned_with_the_graph_without_extra_calls_or_a_new_report_schema() {
    let (request, case) = fixture();
    let model = model(false, false);
    let config = RunConfig::default();
    let report = run_with_tools(&model, &request, &config, &|_| Ok(()))
        .await
        .unwrap();
    assert_eq!(report.transcript.len(), 6);
    let mut expected = case.clone();
    engarg::engine::reconcile_retained_argument(
        &request,
        &mut expected,
        &report.argument,
        &report.transcript,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&report.argument).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        report
            .argument
            .dissent_revisions
            .as_ref()
            .unwrap()
            .origins
            .len(),
        case.defeaters.len()
    );
    assert!(
        serde_json::to_value(&report)
            .unwrap()
            .get("acquisition_plan")
            .is_none()
    );
    let parsed = parse_completion(&report.transcript.last().unwrap().response.content).unwrap();
    assert_eq!(parsed.acquisition_plan.len(), case.evidence_plan.len());
    assert!(validate(&case, &parsed.acquisition_plan).is_empty());
    let calls = model.calls.lock().unwrap();
    for call in &calls[..2] {
        assert!(
            serde_json::from_str::<Value>(&call.user)
                .unwrap()
                .get("acquisition_tools")
                .is_none()
        );
    }
    assert!(
        serde_json::from_str::<Value>(&calls[2].user)
            .unwrap()
            .get("acquisition_tools")
            .is_some()
    );
    for call in calls.iter().filter(|call| {
        serde_json::from_str::<Value>(&call.user)
            .unwrap()
            .get("acquisition_tools")
            .is_some()
    }) {
        assert!(call.system.contains("Prefer an implemented collector"));
        assert!(
            call.system
                .contains("Keep missing named inputs in unresolved_arguments")
        );
        assert!(call.system.contains("cannot supply independent acceptance"));
    }
}

#[tokio::test]
async fn a_missing_binding_uses_the_existing_repair_budget_and_keeps_the_case() {
    let (request, case) = fixture();
    let model = model(true, false);
    let config = RunConfig {
        mode: Mode::Single,
        max_calls: 2,
        max_repairs: 1,
        ..RunConfig::default()
    };
    let report = run_with_tools(&model, &request, &config, &|_| Ok(()))
        .await
        .unwrap();
    assert_eq!(report.transcript.len(), 2);
    let mut expected = case.clone();
    engarg::engine::reconcile_retained_argument(
        &request,
        &mut expected,
        &report.argument,
        &report.transcript,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&report.argument).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        report
            .argument
            .dissent_revisions
            .as_ref()
            .unwrap()
            .origins
            .len(),
        case.defeaters.len()
    );
    let repair: Value = serde_json::from_str(&report.transcript[1].prompt.user).unwrap();
    assert!(
        repair["validation_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error
                .as_str()
                .unwrap()
                .contains("requires an acquisition plan"))
    );
    assert!(repair.get("candidate_acquisition_plan").is_some());
}

#[tokio::test]
async fn legacy_direct_cases_remain_valid_for_legacy_api_and_fail_closed_in_planned_runs() {
    let (request, _) = fixture();
    let config = RunConfig {
        mode: Mode::Single,
        max_calls: 1,
        ..RunConfig::default()
    };
    assert!(run(&model(false, true), &request, &config).await.is_ok());
    let error = run_with_tools(&model(false, true), &request, &config, &|_| Ok(()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("planned case is invalid JSON"));
}
