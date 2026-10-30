//! Independent synthetic investigations exercise generation, repair and eligibility.
use super::*;
use crate::{
    direct_planning::DirectPlan,
    domain::ArgumentCase,
    engine::{Mode, support::revision::revise_with_support},
    provider::{Completion, CompletionRequest, Role},
    support_requirements::{SUPPORT_ELABORATION_SCHEMA, SupportElaboration, material_targets},
};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn support(case: &ArgumentCase) -> SupportElaboration {
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in material_targets(case) {
        let mut needs: Vec<_> = case
            .evidence_plan
            .iter()
            .filter(|need| need.target_id == target)
            .collect();
        needs.sort_by_key(|need| &need.id);
        let id = format!("support_{target}");
        let criterion = json!({"kind":"unavailable","reason":"Synthetic orchestration fixture; no external observation is asserted."});
        requirements.push(json!({"id":id,"role":"support","target_id":target,
            "evidence_need_ids":needs.iter().map(|need|&need.id).collect::<Vec<_>>(),
            "original_acceptance_criteria":needs.iter().map(|need|&need.acceptance_criterion).collect::<Vec<_>>(),
            "contract":null,"discriminating_evidence":[],"acceptance":criterion,
            "acceptance_rationale":"Preserve the independently specified synthetic obligation.",
            "gaps":[{"kind":"clarification","description":"The orchestration test uses a synthetic investigator."}]}));
        evaluations.push(json!({"id":format!("check_{target}"),"requirement_id":id,
            "inputs":[],"criterion":criterion,"max_age_seconds":null,"required_coverage":"unspecified"}));
    }
    serde_json::from_value(json!({"schema":SUPPORT_ELABORATION_SCHEMA,
        "requirements":requirements,"acquisitions":[],"derivations":[],"evaluations":evaluations,
        "revisions":[],"acquisition_revisions":[],"alternative_support":[]}))
    .unwrap()
}

fn report() -> RunReport {
    let statement =
        "The declared synthetic premise supports q under the separately checked fixture contract.";
    let request = json!({"claim":statement});
    let need = |id: &str, target: &str| {
        json!({"id":id,"target_id":target,
        "question":"Does the independently checked synthetic contract support this node?",
        "method":"Use the declared synthetic orchestration investigator.",
        "measurement":"The identified fixture predicate.","comparator":"The independently authored fixture contract.",
        "acceptance_criterion":"The exact fixture's specified obligation is satisfied.",
        "if_failed":"Keep this requirement unresolved.","enables_claim":"current_claim",
        "priority":1,"cost":"One synthetic fixture check."})
    };
    let warrant = |id: &str, conclusion: &str, premise: &str| {
        json!({
        "id":id,"rule":"Compose the declared premise relation under the same synthetic scope.",
        "conclusion_id":conclusion,"premise_ids":[premise],"backing_evidence_ids":[],
        "inference":"deductive","applicability":"Only the independently specified synthetic orchestration contract.",
        "reasoning_method":{"status":"unresolved","method":"deductive",
            "reasons":["A synthetic investigator checks orchestration, not a scientific proposition."],"evidence_need_ids":[]}})
    };
    let case:ArgumentCase = serde_json::from_value(json!({
        "original_claim":statement,
        "current_claim":{"text":statement,"qualifier":"Conditional synthetic fixture; no empirical assertion."},
        "stronger_target":{"text":"The result also holds outside the fixture.","qualifier":"Unobserved and unsupported."},
        "status":"conditional",
        "scope":{"system_boundary":"A synthetic fixture.","configuration":"Fixed fixture v1.",
            "operating_conditions":"The declared fixture only.","units":"1","time_horizon":"This fixture execution.",
            "consequences":"Exercise orchestration without claiming empirical correctness."},
        "subclaims":[{"id":"S1","text":"The intermediate synthetic premise relation holds.","qualifier":"Within the same fixture."}],
        "grounds":[{"id":"G1","statement":"The declared synthetic premise holds.","evidence_ids":[],"status":"missing"}],
        "warrants":[warrant("W1","S1","G1"),warrant("W2","current_claim","S1")],
        "compositions":[
            {"conclusion_id":"S1","relationship":"jointly_required","selected_warrant_ids":["W1"],"rationale":"The intermediate fixture relation requires this route."},
            {"conclusion_id":"current_claim","relationship":"jointly_required","selected_warrant_ids":["W2"],"rationale":"This original route composes the intermediate relation."}],
        "assumptions":[],"defeaters":[],"alternatives":[],
        "evidence_plan":[need("N1","G1"),need("N2","W1"),need("N3","W2")],
        "reading":[{"claim_id":"S1","title":"Synthetic intermediate relation",
            "paragraphs":[{"purpose":"answer","text":"The intermediate synthetic relation follows from the same fixture premise under W1.","reference_ids":["S1","G1","W1","N1","N2"]}]},
            {"claim_id":"current_claim","title":"Synthetic implication",
            "paragraphs":[{"purpose":"answer","text":statement,"reference_ids":["current_claim","S1","G1","W1","W2","N1","N2","N3"]}]}],
        "residual_risks":["This fixture supplies no empirical support."],
        "decision_limits":["Use only to test the development protocol."],
        "adjudication":"The synthetic investigator supplies a declared orchestration outcome."})).unwrap();
    let support = support(&case);
    let report:RunReport = serde_json::from_value(json!({
        "schema_version":"engarg.run.v3","execution_mode":"offline_synthetic_fixture",
        "input_sha256":eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        "mode":Mode::Single,"request":request,"argument":case,"transcript":[],
        "support_elaboration":support,"direct_bindings":DirectPlan::unbound(&support),
        "limitations":["Synthetic protocol test; verification outcomes are stipulated independently of model output."]})).unwrap();
    crate::report::validate(&report).unwrap();
    report
}

fn shorter(original: &RunReport) -> SupportedArgument {
    let mut argument = original.argument.clone();
    argument
        .warrants
        .iter_mut()
        .find(|warrant| warrant.id == "W2")
        .unwrap()
        .premise_ids = vec!["G1".into()];
    argument.subclaims.retain(|claim| claim.id != "S1");
    argument.warrants.retain(|warrant| warrant.id != "W1");
    argument
        .compositions
        .retain(|composition| composition.conclusion_id != "S1");
    argument.evidence_plan.retain(|need| need.target_id != "W1");
    argument.reading.retain(|section| section.claim_id != "S1");
    for section in &mut argument.reading {
        for paragraph in &mut section.paragraphs {
            paragraph
                .reference_ids
                .retain(|id| !matches!(id.as_str(), "S1" | "W1" | "N2"));
        }
    }
    let mut support = original.support_elaboration.clone().unwrap();
    let removed = support
        .requirements
        .iter()
        .find(|requirement| requirement.target_id == "W1")
        .unwrap()
        .clone();
    support
        .requirements
        .retain(|requirement| requirement.id != removed.id);
    support
        .evaluations
        .retain(|evaluation| evaluation.requirement_id != removed.id);
    support.revisions.push(serde_json::from_value(json!({
        "requirement_id":removed.id,
        "previous_requirement_sha256":digest(&removed).unwrap(),
        "reason":"Remove the represented intermediate warrant after replacing its two-step synthetic route with the directly specified relation; the parent report remains in the explicit argument revision."
    })).unwrap());
    SupportedArgument {
        argument,
        support_elaboration: support,
    }
}

fn envelope(proposal: &SupportedArgument) -> String {
    json!({"argument":crate::live_output::generation_value(&proposal.argument),
        "support_elaboration_json":serde_json::to_string(&proposal.support_elaboration).unwrap()})
    .to_string()
}

fn clear_critic() -> String {
    json!({"critical_issues":[],"observations":[]}).to_string()
}

struct MockModel {
    replies: Mutex<VecDeque<Result<String, String>>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl MockModel {
    fn new(replies: Vec<Result<String, String>>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl LanguageModel for MockModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.requests.lock().unwrap().push(request);
        let content = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("declared model reply")
            .map_err(Error::Provider)?;
        Ok(Completion {
            content,
            model: "synthetic-test-model".into(),
            input_tokens: Some(2),
            output_tokens: Some(3),
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

struct Investigator {
    calls: AtomicUsize,
    supported: bool,
}
impl Investigator {
    fn supported() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            supported: true,
        }
    }
}
impl CandidateInvestigator for Investigator {
    async fn assess(
        &self,
        parent: &RunReport,
        proposal: SupportedArgument,
        index: usize,
    ) -> Result<CandidateOutcome, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let report = if digest(&proposal.argument)? == digest(&parent.argument)?
            && digest(&proposal.support_elaboration)?
                == digest(parent.support_elaboration.as_ref().unwrap())?
        {
            parent.clone()
        } else {
            revise_with_support(
                parent,
                proposal.argument,
                Some(proposal.support_elaboration),
                None,
            )?
        };
        Ok(CandidateOutcome {report:Some(report),assessment:super::super::Assessment {
            supported:self.supported,status:if self.supported {"supported"} else {"incomplete"}.into(),
            artifact:format!("candidates/{index:04}"),reasons:vec!["Independent synthetic orchestration oracle; not a native scientific assessment.".into()]},
            verification:json!({"mode":"synthetic_test","premise_checks":"fixture-only","formalisation_checks":"fixture-only"})})
    }
}

fn config(rounds: u8, max_calls: u16) -> DevelopmentConfig {
    DevelopmentConfig {
        rounds,
        max_calls,
        beam_width: 1,
        ..DevelopmentConfig::default()
    }
}

#[tokio::test]
async fn one_route_source_gains_a_shorter_checked_candidate_and_preserves_origin() {
    let original = report();
    let original_value = serde_json::to_value(&original).unwrap();
    let model = MockModel::new(vec![Ok(envelope(&shorter(&original))), Ok(clear_critic())]);
    let investigator = Investigator::supported();
    let events = Mutex::new(Vec::<Value>::new());
    let observe = |event: &Value| {
        events.lock().unwrap().push(event.clone());
        Ok(())
    };
    let result = develop(
        &model,
        &original,
        config(1, 2),
        &json!({"operations":[]}),
        &investigator,
        &observe,
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 2);
    assert_eq!(result.best, Some(1));
    assert_eq!(result.candidates[0].length, Some(5));
    assert_eq!(result.candidates[1].length, Some(3));
    assert_eq!(investigator.calls.load(Ordering::SeqCst), 2);
    assert!(!result.search_complete);
    assert_eq!(
        result.candidates[1]
            .outcome
            .report
            .as_ref()
            .unwrap()
            .argument_revisions
            .len(),
        1
    );
    assert_eq!(serde_json::to_value(&original).unwrap(), original_value);
    assert!(
        result.candidates[1]
            .outcome
            .report
            .as_ref()
            .unwrap()
            .transcript
            .is_empty()
    );
    let encoded = serde_json::to_value(&result).unwrap();
    assert!(encoded["candidates"][1]["outcome"].get("report").is_none());
    let requests = model.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.role)
            .collect::<Vec<_>>(),
        [Role::Constructor, Role::Sceptic]
    );
    let construction_context: Value = serde_json::from_str(&requests[0].user).unwrap();
    assert_eq!(
        construction_context["objective"]["metric"],
        "unique_selected_dependency_nodes"
    );
    assert_eq!(
        construction_context["objective"]["target_id"],
        "current_claim"
    );
    assert_eq!(construction_context["objective"]["parent_count"], 5);
    assert_eq!(
        construction_context["objective"]["shared_nodes_counted_once"],
        true
    );
    assert_eq!(
        construction_context["objective"]["unselected_canonical_nodes_counted"],
        false
    );
    assert!(
        requests[0]
            .system
            .contains("that warrant's formal_premises and evidence_requirements")
    );
    assert!(
        requests[0]
            .system
            .contains("Preserve shared or referenced grounds")
    );
    assert!(
        requests[0]
            .system
            .contains("deleting them does not improve this objective")
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "model_completion"
                && event["completion"]["content"].is_string())
    );
}

#[tokio::test]
async fn malformed_proposal_is_retained_before_parse_and_diagnostics_reach_repair() {
    let original = report();
    let model = MockModel::new(vec![
        Ok("{".into()),
        Ok(envelope(&shorter(&original))),
        Ok(clear_critic()),
    ]);
    let events = Mutex::new(Vec::<Value>::new());
    let result = develop(
        &model,
        &original,
        config(2, 3),
        &json!({}),
        &Investigator::supported(),
        &|event| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(result.best, Some(1));
    assert_eq!(result.calls_attempted, 3);
    assert_eq!(result.failures[0].phase, "proposal_parse");
    let requests = model.requests.lock().unwrap();
    assert!(requests[1].user.contains("proposal_parse"));
    let events = events.lock().unwrap();
    let completion = events
        .iter()
        .position(|event| {
            event["event"] == "model_completion" && event["completion"]["content"] == "{"
        })
        .unwrap();
    let failure = events
        .iter()
        .position(|event| event["event"] == "development_failure")
        .unwrap();
    assert!(completion < failure);
}

#[tokio::test]
async fn rejected_formulation_and_its_guard_diagnosis_reach_the_repair_call() {
    struct AdmissionInvestigator(Investigator);
    impl CandidateInvestigator for AdmissionInvestigator {
        async fn assess(
            &self,
            parent: &RunReport,
            proposal: SupportedArgument,
            index: usize,
        ) -> Result<CandidateOutcome, Error> {
            if let Err(errors) = crate::domain::validate_case(&parent.request, &proposal.argument) {
                return Ok(CandidateOutcome {
                    report: None,
                    assessment: super::super::Assessment {
                        supported: false,
                        status: "verification_failed".into(),
                        artifact: format!("candidates/{index:04}"),
                        reasons: errors.clone(),
                    },
                    verification: json!({"guard":"argument_admission","status":"fail","reasons":errors}),
                });
            }
            self.0.assess(parent, proposal, index).await
        }
    }
    let original = report();
    let repaired = shorter(&original);
    let mut rejected = repaired.clone();
    rejected.argument.grounds[0].evidence_ids = vec!["invented_assurance_receipt_evidence".into()];
    let model = MockModel::new(vec![
        Ok(envelope(&rejected)),
        Ok(envelope(&repaired)),
        Ok(clear_critic()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 3),
        &json!({}),
        &AdmissionInvestigator(Investigator::supported()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 3);
    assert_eq!(result.best, Some(2));
    assert_eq!(result.candidates[2].parent_index, Some(0));
    assert!(result.candidates[1].outcome.report.is_none());
    assert!(result.candidates[1].rejected_proposal.is_some());
    assert!(result.candidates[2].rejected_proposal.is_none());
    let stored = serde_json::to_value(&result.candidates[1]).unwrap();
    assert!(stored.get("rejected_proposal").is_none());
    let requests = model.requests.lock().unwrap();
    let repair_context: Value = serde_json::from_str(&requests[1].user).unwrap();
    let diagnosed = repair_context["diagnosed_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["index"] == 1)
        .unwrap();
    assert_eq!(
        diagnosed["admission_status"],
        "rejected_unadmitted_proposal"
    );
    assert_eq!(
        diagnosed["argument"]["grounds"][0]["evidence_ids"][0],
        "invented_assurance_receipt_evidence"
    );
    assert_eq!(
        diagnosed["support_elaboration"]["schema"],
        SUPPORT_ELABORATION_SCHEMA
    );
    assert_eq!(
        diagnosed["verification_results"]["guard"],
        "argument_admission"
    );
    assert!(
        diagnosed["diagnostics"]
            .to_string()
            .contains("invented_assurance_receipt_evidence")
    );
    assert!(
        requests[1]
            .system
            .contains("outputs, not supplied evidence IDs")
    );
}

#[tokio::test]
async fn unresolved_sceptic_issue_vetoes_a_shorter_supported_result_and_guides_repair() {
    let original = report();
    let short = envelope(&shorter(&original));
    let mut repaired = shorter(&original);
    repaired.argument.warrants[0].rule = "Use the checked direct fixture correspondence required by the independently specified oracle.".into();
    let criticism = json!({"critical_issues":["Check the missing interpretation correspondence with a distinguishing example."],"observations":[]}).to_string();
    let model = MockModel::new(vec![
        Ok(short),
        Ok(criticism),
        Ok(envelope(&repaired)),
        Ok(clear_critic()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert!(!result.candidates[1].eligible);
    assert!(result.candidates[1].outcome.assessment.supported);
    assert_eq!(result.best, Some(2));
    // The original remains in the width-one beam after the veto. Global failed
    // findings still need to reach a subsequent proposal from that parent.
    assert_eq!(
        result.candidates[1]
            .criticism
            .as_ref()
            .unwrap()
            .critical_issues
            .len(),
        1
    );
    assert!(
        model.requests.lock().unwrap()[2]
            .user
            .contains("missing interpretation correspondence")
    );
}

#[tokio::test]
async fn repeated_identical_candidate_cannot_drop_a_prior_critical_issue() {
    let original = report();
    let short = envelope(&shorter(&original));
    let criticism = json!({"critical_issues":["The interpretation needs an actual distinguishing check."],"observations":[]}).to_string();
    let model = MockModel::new(vec![
        Ok(short.clone()),
        Ok(criticism),
        Ok(short),
        Ok(clear_critic()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.best, Some(0));
    assert!(!result.candidates[2].eligible);
    assert_eq!(result.candidates[2].pending_critical_issues.len(), 1);
}

#[tokio::test]
async fn material_finding_vetoes_an_identical_baseline_as_well_as_the_new_copy() {
    let original = report();
    let identical = SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().unwrap(),
    };
    let model = MockModel::new(vec![
        Ok(envelope(&identical)),
        Ok(json!({"critical_issues":["The baseline interpretation needs its independent distinguishing check."],"observations":[]}).to_string()),
    ]);
    let events = Mutex::new(Vec::<Value>::new());
    let result = develop(
        &model,
        &original,
        config(1, 2),
        &json!({}),
        &Investigator::supported(),
        &|event| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(result.best, None);
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| !candidate.eligible)
    );
    assert_eq!(
        result.candidates[0].pending_critical_issues,
        result.candidates[1].pending_critical_issues
    );
    assert!(events.lock().unwrap().iter().any(|event| {
        event["event"] == "semantic_veto" && event["candidate_indices"] == json!([0, 1])
    }));
}

#[tokio::test]
async fn later_material_finding_vetoes_an_identical_previously_eligible_winner() {
    let original = report();
    let short = envelope(&shorter(&original));
    let model = MockModel::new(vec![
        Ok(short.clone()),
        Ok(clear_critic()),
        Ok(short),
        Ok(json!({"critical_issues":["A distinguishing counterexample remains unresolved for this shortcut."],"observations":[]}).to_string()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert!(
        result.candidates[1]
            .criticism
            .as_ref()
            .unwrap()
            .critical_issues
            .is_empty()
    );
    assert!(result.candidates[1].outcome.assessment.supported);
    assert!(!result.candidates[1].eligible);
    assert!(!result.candidates[2].eligible);
    assert_eq!(
        result.candidates[1].pending_critical_issues,
        result.candidates[2].pending_critical_issues
    );
    assert_eq!(result.best, Some(0));
    assert!(result.candidates[0].eligible);
}

#[tokio::test]
async fn refreshed_receipt_timestamps_cannot_clear_an_identical_proposal_veto() {
    struct RefreshingInvestigator(Investigator);
    impl CandidateInvestigator for RefreshingInvestigator {
        async fn assess(
            &self,
            parent: &RunReport,
            proposal: SupportedArgument,
            index: usize,
        ) -> Result<CandidateOutcome, Error> {
            let mut outcome = self.0.assess(parent, proposal, index).await?;
            outcome.verification["observed_at"] = json!(format!("2026-10-30T00:00:{index:02}Z"));
            Ok(outcome)
        }
    }
    let original = report();
    let short = envelope(&shorter(&original));
    let model = MockModel::new(vec![
        Ok(short.clone()),
        Ok(json!({"critical_issues":["The intended interpretation still needs its distinguishing check."],"observations":[]}).to_string()),
        Ok(short),
        Ok(clear_critic()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &RefreshingInvestigator(Investigator::supported()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    let first = &result.candidates[1];
    let refreshed = &result.candidates[2];
    assert_ne!(first.outcome.verification, refreshed.outcome.verification);
    assert_ne!(first.investigation_sha256, refreshed.investigation_sha256);
    assert_eq!(first.proposal_sha256, refreshed.proposal_sha256);
    assert!(refreshed.outcome.assessment.supported);
    assert!(
        refreshed
            .criticism
            .as_ref()
            .unwrap()
            .critical_issues
            .is_empty()
    );
    assert!(!refreshed.eligible);
    assert_eq!(
        refreshed.pending_critical_issues,
        first.pending_critical_issues
    );
    assert_eq!(result.best, Some(0));
}

#[tokio::test]
async fn provider_failure_consumes_allowance_and_retains_the_failed_request() {
    let original = report();
    let model = MockModel::new(vec![Err("Synthetic provider failure".into())]);
    let events = Mutex::new(Vec::<Value>::new());
    let result = develop(
        &model,
        &original,
        config(2, 1),
        &json!({}),
        &Investigator::supported(),
        &|event| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 1);
    assert!(result.budget_exhausted);
    assert_eq!(result.stop_reason, "call_budget");
    assert_eq!(result.failures[0].phase, "proposal");
    assert!(!result.calls[0].completed);
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "model_request")
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "model_failure")
    );
}

#[tokio::test]
async fn exhausted_critic_allowance_never_promotes_an_unreviewed_candidate() {
    let original = report();
    let model = MockModel::new(vec![Ok(envelope(&shorter(&original)))]);
    let result = develop(
        &model,
        &original,
        config(1, 1),
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.best, Some(0));
    assert!(!result.candidates[1].eligible);
    assert!(result.candidates[1].criticism.is_none());
    assert!(result.budget_exhausted);
}

#[tokio::test]
async fn no_supported_investigation_cannot_be_repaired_by_model_agreement_alone() {
    let original = report();
    let model = MockModel::new(vec![Ok(envelope(&shorter(&original))), Ok(clear_critic())]);
    let investigator = Investigator {
        calls: AtomicUsize::new(0),
        supported: false,
    };
    let result = develop(
        &model,
        &original,
        config(1, 2),
        &json!({}),
        &investigator,
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert!(result.best.is_none());
    assert!(!result.candidates[1].eligible);
}

#[tokio::test]
async fn input_and_response_limits_stop_interpretation_and_keep_accounting() {
    let original = report();
    let model = MockModel::new(vec![]);
    let tiny = DevelopmentConfig {
        max_input_bytes: 1,
        ..config(1, 1)
    };
    let result = develop(
        &model,
        &original,
        tiny,
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 0);
    assert_eq!(result.failures[0].phase, "input_limit");
    let model = MockModel::new(vec![Ok("x".repeat(64))]);
    let tiny = DevelopmentConfig {
        max_response_bytes: 32,
        ..config(1, 1)
    };
    let events = Mutex::new(Vec::<Value>::new());
    let result = develop(
        &model,
        &original,
        tiny,
        &json!({}),
        &Investigator::supported(),
        &|event| {
            events.lock().unwrap().push(event.clone());
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 1);
    assert_eq!(result.failures[0].phase, "response_limit");
    assert!(result.calls[0].response_content_omitted);
    let events = events.lock().unwrap();
    let completion = events
        .iter()
        .find(|event| event["event"] == "model_completion")
        .unwrap();
    assert_eq!(completion["completion"]["content"], "");
    assert_eq!(completion["call"]["response_content_bytes"], 64);
}

#[tokio::test]
async fn inability_to_retain_a_completion_stops_before_parse_or_investigation() {
    let original = report();
    let model = MockModel::new(vec![Ok(envelope(&shorter(&original)))]);
    let investigator = Investigator::supported();
    let result = develop(
        &model,
        &original,
        config(1, 2),
        &json!({}),
        &investigator,
        &|event| {
            if event["event"] == "model_completion" {
                Err(Error::Io(std::io::Error::other("synthetic audit failure")))
            } else {
                Ok(())
            }
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(investigator.calls.load(Ordering::SeqCst), 1);
    assert_eq!(model.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_bounds_fail_before_investigator_or_model_calls() {
    let original = report();
    let investigator = Investigator::supported();
    let model = MockModel::new(vec![]);
    for config in [
        DevelopmentConfig {
            rounds: 0,
            ..DevelopmentConfig::default()
        },
        DevelopmentConfig {
            max_calls: 65,
            ..DevelopmentConfig::default()
        },
        DevelopmentConfig {
            beam_width: 0,
            ..DevelopmentConfig::default()
        },
    ] {
        assert!(
            develop(
                &model,
                &original,
                config,
                &json!({}),
                &investigator,
                &|_| Ok(())
            )
            .await
            .is_err()
        );
    }
    assert_eq!(investigator.calls.load(Ordering::SeqCst), 0);
}
