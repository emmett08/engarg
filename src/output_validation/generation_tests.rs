//! Live authoring requires explicit reasoning methods while historical imports remain valid.

use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::{
    Error,
    acquisition::{AcquisitionPlan, PlannedArgument},
    domain::{ReasoningMethodBinding, ReasoningMethodKind, validate_case},
    engine::{Mode, RouteResolver, RunConfig, run, run_with_support, run_with_tools},
    provider::{Completion, CompletionRequest, LanguageModel},
    support_requirements::{SupportElaboration, tests::pending},
};

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../../examples/request.json")).unwrap(),
        crate::live_output::case(include_str!("../../examples/case.json")),
    )
}

#[test]
fn historical_methods_remain_importable_but_live_computational_methods_are_required() {
    let (request, current) = fixture();
    let historical: ArgumentCase =
        serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
    assert!(
        historical
            .warrants
            .iter()
            .all(|warrant| warrant.reasoning_method.is_none())
    );
    assert_eq!(validate_case(&request, &historical), Ok(()));

    for method in [
        ReasoningMethodKind::Deductive,
        ReasoningMethodKind::Inductive,
        ReasoningMethodKind::Abductive,
        ReasoningMethodKind::Causal,
        ReasoningMethodKind::Counterfactual,
    ] {
        let mut candidate = current.clone();
        candidate.warrants[0].inference = method.inference();
        candidate.warrants[0].reasoning_method = None;
        assert_eq!(validate_case(&request, &candidate), Ok(()));
        let messages = errors(&request, &candidate, &[], None);
        assert_eq!(messages.len(), 1, "{method:?}: {messages:?}");
        assert!(
            messages[0]
                .contains("warrant W1 requires an explicit bound or unresolved reasoning_method")
        );

        candidate.warrants[0].reasoning_method = Some(ReasoningMethodBinding::Unresolved {
            method,
            reasons: vec!["The required bounded model has not been supplied.".into()],
            evidence_need_ids: vec![],
        });
        assert!(errors(&request, &candidate, &[], None).is_empty());
    }
    for inference in [InferenceKind::Analogical, InferenceKind::Practical] {
        let mut candidate = current.clone();
        candidate.warrants[0].inference = inference;
        candidate.warrants[0].reasoning_method = None;
        assert!(errors(&request, &candidate, &[], None).is_empty());
    }
}

#[derive(Clone, Copy, Debug)]
enum Envelope {
    Case,
    Tools,
    Support,
}

struct Model {
    envelope: Envelope,
    repair: bool,
    calls: Mutex<Vec<CompletionRequest>>,
}

impl LanguageModel for Model {
    async fn complete(&self, prompt: CompletionRequest) -> Result<Completion, Error> {
        let data: Value = serde_json::from_str(&prompt.user)?;
        let role = prompt.role;
        self.calls.lock().unwrap().push(prompt);
        let content = if matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator) {
            let (_, mut argument) = fixture();
            if !self.repair || data.get("repair_attempt").is_none() {
                argument.warrants[0].reasoning_method = None;
            }
            match self.envelope {
                Envelope::Case => serde_json::to_string(&argument)?,
                Envelope::Tools => {
                    let acquisition_plan = argument
                        .evidence_plan
                        .iter()
                        .map(|need| AcquisitionPlan {
                            input_preparation: None,
                            need_id: need.id.clone(),
                            tool_id: "custom".into(),
                            arguments: vec![],
                            unresolved_arguments: vec!["adapter".into()],
                        })
                        .collect();
                    serde_json::to_string(&PlannedArgument {
                        argument,
                        acquisition_plan,
                    })?
                }
                Envelope::Support => {
                    let elaboration = pending(&argument);
                    json!({"argument":argument,"support_elaboration_json":serde_json::to_string(&elaboration)?}).to_string()
                }
            }
        } else {
            json!({"findings":[],"objections":[],"evidence_gaps":[]}).to_string()
        };
        Ok(Completion {
            content,
            model: "live-method-contract-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

struct Resolver;

impl RouteResolver for Resolver {
    type Plan = Value;

    async fn resolve(&self, _: &ArgumentCase, _: &SupportElaboration) -> Result<Value, Error> {
        Ok(json!({"status":"unresolved"}))
    }
}

async fn author(model: &Model, config: &RunConfig) -> Result<ArgumentCase, Error> {
    let request = fixture().0;
    let observer = |_: &crate::engine::CallRecord| Ok(());
    match model.envelope {
        Envelope::Case => run(model, &request, config)
            .await
            .map(|report| report.argument),
        Envelope::Tools => run_with_tools(model, &request, config, &observer)
            .await
            .map(|report| report.argument),
        Envelope::Support => run_with_support(model, &request, config, &observer, &Resolver)
            .await
            .map(|report| report.report.argument),
    }
}

#[tokio::test]
async fn every_live_envelope_rejects_missing_methods_and_repairs_require_an_explicit_choice() {
    for mode in [Mode::Single, Mode::Hybrid] {
        let base_calls = if mode == Mode::Single { 1 } else { 6 };
        for envelope in [Envelope::Case, Envelope::Tools, Envelope::Support] {
            let model = Model {
                envelope,
                repair: false,
                calls: Mutex::new(vec![]),
            };
            let mut config = RunConfig {
                mode,
                max_calls: base_calls,
                ..RunConfig::default()
            };
            let error = author(&model, &config).await.unwrap_err();
            assert!(
                error.to_string().contains(
                    "warrant W1 requires an explicit bound or unresolved reasoning_method"
                ),
                "{mode:?} {envelope:?}: {error}"
            );
            assert_eq!(model.calls.lock().unwrap().len(), base_calls as usize);

            // Exhausting the allowance still rejects a replacement which omits the method.
            config.max_repairs = 1;
            config.max_calls += 1;
            let error = author(&model, &config).await.unwrap_err();
            assert!(error.to_string().contains("reasoning_method"));

            let model = Model {
                envelope,
                repair: true,
                calls: Mutex::new(vec![]),
            };
            let argument = author(&model, &config).await.unwrap();
            assert!(argument.warrants[0].reasoning_method.is_some());
            let calls = model.calls.lock().unwrap();
            assert_eq!(calls.len(), config.max_calls as usize);
            let repair: Value = serde_json::from_str(&calls.last().unwrap().user).unwrap();
            assert!(
                repair["validation_errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|error| error.as_str().unwrap().contains("reasoning_method"))
            );
        }
    }
}
