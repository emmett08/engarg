//! Bounded validation-guided repair of the workflow’s final candidate.

use super::{CallRecord, RunConfig, argument, context, execute, planned_argument, tool_prompt};
use crate::{
    Error, acquisition,
    domain::{ArgumentCase, Request, case_schema},
    output_validation::{self, RetainedObjection},
    provider::LanguageModel,
};
use serde_json::json;

/// Correct only invalid model content. Transport failures, refusals, truncation
/// and exhausted resource bounds never trigger another model request here.
pub(super) async fn validate_or_repair<M: LanguageModel, O>(
    model: &M,
    request: &Request,
    config: &RunConfig,
    observer: &O,
    transcript: &mut Vec<CallRecord>,
    retained_objections: &mut Vec<RetainedObjection>,
    tool_planning: bool,
) -> Result<ArgumentCase, Error>
where
    O: Fn(&CallRecord) -> Result<(), Error> + Send + Sync,
{
    let mut repair_scope = None;
    for attempt in 0..=config.max_repairs {
        let mut scope_feedback = serde_json::Value::Null;
        let candidate = transcript.last().expect("workflow has a final call");
        let parsed = if tool_planning {
            planned_argument(candidate).map(|planned| (planned.argument, planned.acquisition_plan))
        } else {
            argument(candidate).map(|case| (case, Vec::new()))
        };
        let (errors, candidate_json, candidate_acquisition_plan) = match parsed {
            Ok((case, plans)) => {
                let mut errors = output_validation::errors(
                    request,
                    &case,
                    retained_objections,
                    config.max_argument_depth,
                );
                if tool_planning {
                    errors.extend(acquisition::validate_selected(
                        &case,
                        &plans,
                        config.collector_catalogue.as_ref(),
                    ));
                }
                scope_feedback = super::repair_scope::constrain(
                    &mut repair_scope,
                    &case,
                    retained_objections,
                    &mut errors,
                    serde_json::to_value(&plans)?,
                )?;
                if errors.is_empty() {
                    return Ok(case);
                }
                // Repairs may revise disposition with a justified response, but
                // cannot make dissent disappear simply to pass validation.
                output_validation::retain_objections(&case, candidate.role, retained_objections);
                (
                    errors,
                    context::argument(request, &case, retained_objections)?,
                    serde_json::to_value(plans)?,
                )
            }
            // Preserve malformed input verbatim: parsing arbitrary JSON into a
            // generic value could hide duplicate keys or the offending syntax.
            Err(error) => (
                vec![error.to_string()],
                json!(candidate.response.content),
                json!(null),
            ),
        };
        if attempt == config.max_repairs {
            return Err(Error::InvalidOutput(format!(
                "{}; validation failed after {attempt} repair calls (max-repairs {})",
                errors.join("; "),
                config.max_repairs,
            )));
        }
        let mut data = json!({
            "request": request,
            "candidate_json": candidate_json,
            "validation_errors": errors,
            "retained_objections": retained_objections,
            "repair_attempt": attempt + 1,
            "repair_scope": scope_feedback,
        });
        if tool_planning {
            data["candidate_acquisition_plan"] = candidate_acquisition_plan;
        }
        let repair = tool_prompt(candidate.role, data, case_schema(), config, tool_planning)?;
        transcript.push(execute(model, repair, config, observer).await?);
    }
    unreachable!("validation returns or exhausts the bounded repair allowance")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        domain::validate_case,
        engine::{Mode, run, run_with_observer},
        provider::{Completion, CompletionRequest, Role},
    };

    #[derive(Clone, Copy)]
    enum RepairOutcome {
        Correct,
        StillInvalid,
        DropObjection,
        DowngradeMateriality,
        ProviderFailure,
        Malformed,
    }

    struct Model {
        calls: Mutex<Vec<CompletionRequest>>,
        outcome: RepairOutcome,
    }

    fn fixture() -> (Request, ArgumentCase) {
        let request =
            serde_json::from_str(include_str!("../../examples/claim-only-request.json")).unwrap();
        let mut case: ArgumentCase =
            serde_json::from_str(include_str!("../../examples/claim-only-case.json")).unwrap();
        for index in 1..=5 {
            let mut defeater = case.defeaters[0].clone();
            defeater.id = format!("auditor-{index}");
            defeater.challenge = format!("Untested material concern {index}.");
            defeater.target_id = "current_claim".into();
            case.defeaters.push(defeater);
            let mut need = case.evidence_plan[0].clone();
            need.id = format!("audit-plan-{index}");
            need.target_id = format!("auditor-{index}");
            need.enables_claim = "current_claim".into();
            case.evidence_plan.push(need);
        }
        assert_eq!(validate_case(&request, &case), Ok(()));
        (request, case)
    }

    impl LanguageModel for Model {
        async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
            let role = request.role;
            let data: serde_json::Value = serde_json::from_str(&request.user).unwrap();
            let is_repair = data.get("repair_attempt").is_some();
            self.calls.lock().unwrap().push(request);
            if is_repair && matches!(self.outcome, RepairOutcome::ProviderFailure) {
                return Err(Error::Provider("fixture refusal".into()));
            }
            let content = if !is_repair && matches!(self.outcome, RepairOutcome::Malformed) {
                "{\"original_claim\":\"first\",\"original_claim\":\"second\"}".into()
            } else if matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator) {
                let (_, mut case) = fixture();
                if !is_repair || matches!(self.outcome, RepairOutcome::StillInvalid) {
                    case.evidence_plan
                        .retain(|need| need.target_id != "auditor-5");
                } else if matches!(self.outcome, RepairOutcome::DropObjection) {
                    case.defeaters.retain(|defeater| defeater.id != "auditor-5");
                    case.evidence_plan
                        .retain(|need| need.target_id != "auditor-5");
                } else if matches!(self.outcome, RepairOutcome::DowngradeMateriality) {
                    case.defeaters
                        .iter_mut()
                        .find(|d| d.id == "auditor-5")
                        .unwrap()
                        .material = false;
                    case.evidence_plan
                        .retain(|need| need.target_id != "auditor-5");
                }
                serde_json::to_string(&case)?
            } else {
                json!({"findings": [], "evidence_gaps": [], "objections": if role == Role::EvidenceAuditor {
                    (1..=5).map(|i| format!("Untested material concern {i}.")).collect::<Vec<_>>()
                } else {vec![]}}).to_string()
            };
            Ok(Completion {
                content,
                model: "deterministic-repair-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }

    fn model(outcome: RepairOutcome) -> Model {
        Model {
            calls: Mutex::new(Vec::new()),
            outcome,
        }
    }

    #[tokio::test]
    async fn the_reported_auditor_5_failure_reaches_adjudication_and_a_bounded_repair() {
        let model = model(RepairOutcome::Correct);
        let (request, _) = fixture();
        let observed = Mutex::new(Vec::new());
        let report = run_with_observer(
            &model,
            &request,
            &RunConfig {
                max_calls: 7,
                max_repairs: 1,
                max_argument_depth: Some(2),
                ..RunConfig::default()
            },
            &|record: &CallRecord| {
                observed.lock().unwrap().push(record.clone());
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(report.transcript.len(), 7);
        assert_eq!(observed.lock().unwrap().len(), 7);
        assert!(
            report
                .argument
                .evidence_plan
                .iter()
                .any(|need| need.target_id == "auditor-5")
        );
        let calls = model.calls.lock().unwrap();
        for call in &*calls {
            assert!(call.system.contains("Use at most 2 warrant layers"));
        }
        let final_data: serde_json::Value = serde_json::from_str(&calls[5].user).unwrap();
        let repair_data: serde_json::Value = serde_json::from_str(&calls[6].user).unwrap();
        for errors in [
            &final_data["draft_validation_errors"],
            &repair_data["validation_errors"],
        ] {
            assert!(
                errors.as_array().unwrap().iter().any(|error| error
                    == "unresolved material defeater auditor-5 needs an evidence plan")
            );
        }
        assert!(
            repair_data["retained_objections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|objection| objection["id"] == "auditor-5"
                    && objection["must_remain_material"] == true)
        );
        assert_eq!(report.run_config.unwrap().max_argument_depth, Some(2));
    }

    #[tokio::test]
    async fn repair_is_opt_in_and_cannot_drop_or_downgrade_the_objection() {
        let (request, _) = fixture();
        let disabled = model(RepairOutcome::Correct);
        assert!(
            matches!(run(&disabled, &request, &RunConfig::default()).await, Err(Error::InvalidOutput(message)) if message.contains("auditor-5"))
        );
        assert_eq!(disabled.calls.lock().unwrap().len(), 6);
        for outcome in [
            RepairOutcome::StillInvalid,
            RepairOutcome::DropObjection,
            RepairOutcome::DowngradeMateriality,
        ] {
            let model = model(outcome);
            let error = run(
                &model,
                &request,
                &RunConfig {
                    max_repairs: 1,
                    max_calls: 7,
                    ..RunConfig::default()
                },
            )
            .await
            .unwrap_err();
            assert!(
                matches!(error, Error::InvalidOutput(message) if message.contains("auditor-5"))
            );
            assert_eq!(model.calls.lock().unwrap().len(), 7);
        }
    }

    #[tokio::test]
    async fn single_mode_can_repair_and_provider_errors_do_not_retry() {
        let (request, _) = fixture();
        let corrected = model(RepairOutcome::Correct);
        assert_eq!(
            run(
                &corrected,
                &request,
                &RunConfig {
                    mode: Mode::Single,
                    max_calls: 2,
                    max_repairs: 1,
                    ..RunConfig::default()
                }
            )
            .await
            .unwrap()
            .transcript
            .len(),
            2
        );
        let refused = model(RepairOutcome::ProviderFailure);
        assert!(matches!(
            run(
                &refused,
                &request,
                &RunConfig {
                    mode: Mode::Single,
                    max_calls: 4,
                    max_repairs: 3,
                    ..RunConfig::default()
                }
            )
            .await,
            Err(Error::Provider(_))
        ));
        assert_eq!(refused.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn repair_allowance_requires_a_sufficient_call_budget_before_any_call() {
        let model = model(RepairOutcome::Correct);
        assert!(
            matches!(run(&model, &fixture().0, &RunConfig {max_repairs: 1, ..RunConfig::default()}).await, Err(Error::Budget(message)) if message.contains("7 calls"))
        );
        assert!(model.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_candidates_retain_the_complete_original_string_in_repair_context() {
        let model = model(RepairOutcome::Malformed);
        let report = run(
            &model,
            &fixture().0,
            &RunConfig {
                mode: Mode::Single,
                max_calls: 2,
                max_repairs: 1,
                ..RunConfig::default()
            },
        )
        .await
        .unwrap();
        let data: serde_json::Value =
            serde_json::from_str(&report.transcript[1].prompt.user).unwrap();
        assert_eq!(
            data["candidate_json"],
            report.transcript[0].response.content
        );
        assert!(data["candidate_json"].is_string());
    }
}
