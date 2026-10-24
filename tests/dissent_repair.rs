//! A bounded repair retains the first attachment of a final critic's objection.

use engarg::{
    Error,
    domain::{ArgumentCase, DefeaterStatus, Request, validate_case},
    engine::{Mode, RunConfig, run},
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};
use serde_json::{Value, json};
use std::sync::Mutex;

const CHALLENGES: [&str; 3] = [
    "The timing mechanism may omit fixture setup overhead.",
    "Shared mutable fixtures may invalidate outcome equivalence.",
    "The current claim may be read as applying to configurations without isolated runner resources.",
];

#[derive(Clone, Copy)]
enum Repair {
    PreserveTarget,
    ExplicitRevision,
    SilentRevision,
    RefineInvestigation,
}

struct Model {
    repair: Repair,
    calls: Mutex<Vec<CompletionRequest>>,
}

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/claim-only-case.json")).unwrap(),
    )
}

fn critic_case(repair: Option<Repair>, target_assumption: bool) -> ArgumentCase {
    let (request, mut case) = fixture();
    for (index, (challenge, target)) in CHALLENGES
        .iter()
        .zip([
            "W1",
            "G2",
            if target_assumption {
                "A1"
            } else {
                "current_claim"
            },
        ])
        .enumerate()
    {
        let id = format!("critic-{}", index + 1);
        let mut defeater = case.defeaters[0].clone();
        defeater.id.clone_from(&id);
        defeater.target_id = target.into();
        defeater.challenge = (*challenge).into();
        defeater.response =
            "The proposed investigation must assess this unresolved challenge.".into();
        if index == 2 && matches!(repair, Some(Repair::ExplicitRevision)) {
            defeater.status = DefeaterStatus::InapplicableAfterRevision;
            defeater.response = "The revised current claim explicitly excludes runner configurations without isolated resources. The original generalisation challenge is retained, but no longer applies within that restriction.".into();
        }
        case.defeaters.push(defeater);
        let mut need = case.evidence_plan[0].clone();
        need.id = format!("N-{id}");
        need.target_id = id;
        need.question = (*challenge).into();
        need.enables_claim = "current_claim".into();
        case.evidence_plan.push(need);
    }
    if repair.is_none() {
        // A different validation defect initiates the one permitted repair.
        // critic-3 acquires its first target identity from this adjudication.
        case.evidence_plan
            .retain(|need| need.target_id != "critic-3");
    }
    if matches!(
        repair,
        Some(Repair::ExplicitRevision | Repair::SilentRevision)
    ) {
        case.current_claim.qualifier.push_str(
            " The claim is restricted to runner configurations with isolated mutable resources.",
        );
        case.reading[0].paragraphs[0].text.push_str(
            " The current claim excludes runner configurations without isolated mutable resources.",
        );
    }
    if matches!(repair, Some(Repair::RefineInvestigation)) {
        case.assumptions[0].validation.push_str(
            " Retain the fixture ownership map and the run-order randomisation record with every comparison.",
        );
    }
    if repair.is_some() {
        assert_eq!(validate_case(&request, &case), Ok(()));
    }
    case
}

impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let role = request.role;
        let data: Value = serde_json::from_str(&request.user)?;
        let is_repair = data.get("repair_attempt").is_some();
        self.calls.lock().unwrap().push(request);
        let content = match role {
            Role::Adjudicator | Role::Single => serde_json::to_string(&critic_case(
                is_repair.then_some(self.repair),
                matches!(self.repair, Repair::RefineInvestigation),
            ))?,
            Role::Synthesiser => serde_json::to_string(&fixture().1)?,
            _ => json!({
                "findings": [],
                "objections": if role == Role::FinalCritic {
                    CHALLENGES.to_vec()
                } else {
                    vec![]
                },
                "evidence_gaps": [],
            })
            .to_string(),
        };
        Ok(Completion {
            content,
            model: "critic-target-repair-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

fn model(repair: Repair) -> Model {
    Model {
        repair,
        calls: Mutex::new(Vec::new()),
    }
}

fn config(mode: Mode) -> RunConfig {
    RunConfig {
        mode,
        max_calls: if mode == Mode::Hybrid { 7 } else { 2 },
        max_repairs: 1,
        ..RunConfig::default()
    }
}

fn assert_repair_context(model: &Model, expected_calls: usize) {
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), expected_calls);
    let repair = calls.last().unwrap();
    let data: Value = serde_json::from_str(&repair.user).unwrap();
    assert_eq!(data["repair_attempt"], 1);
    let objection = data["retained_objections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|objection| objection["id"] == "critic-3")
        .unwrap();
    assert_eq!(objection["challenge"], CHALLENGES[2]);
    assert_eq!(objection["must_remain_material"], true);
    let target_assumption = matches!(model.repair, Repair::RefineInvestigation);
    assert_eq!(
        objection["identity"]["target_id"],
        if target_assumption {
            "A1"
        } else {
            "current_claim"
        }
    );
    assert_eq!(objection["identity"]["kind"], "undercutting");
    assert_eq!(objection["identity"]["basis"], "hypothetical");
    if target_assumption {
        assert_eq!(
            objection["identity"]["target_context"]["target"]["statement"],
            fixture().1.assumptions[0].statement
        );
    } else {
        assert_eq!(
            objection["identity"]["target_context"]["target"]["qualifier"],
            fixture().1.current_claim.qualifier
        );
    }
    assert_eq!(
        objection["identity"]["target_context"]["scope"],
        serde_json::to_value(fixture().1.scope).unwrap()
    );
    assert!(
        data["validation_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error == "unresolved material defeater critic-3 needs an evidence plan")
    );
    for rule in [
        "identity.target_context",
        "target_id",
        "kind",
        "basis",
        "restore",
        "inapplicable_after_revision",
    ] {
        assert!(repair.system.contains(rule), "repair lacks {rule}");
    }
}

#[tokio::test]
async fn critic_3_can_keep_its_target_and_receive_the_missing_plan_in_one_repair() {
    let model = model(Repair::PreserveTarget);
    let report = run(&model, &fixture().0, &config(Mode::Hybrid))
        .await
        .unwrap();
    let objection = report
        .argument
        .defeaters
        .iter()
        .find(|objection| objection.id == "critic-3")
        .unwrap();
    assert_eq!(objection.status, DefeaterStatus::Open);
    assert_eq!(
        report.argument.current_claim.qualifier,
        fixture().1.current_claim.qualifier
    );
    assert!(
        report
            .argument
            .evidence_plan
            .iter()
            .any(|need| need.target_id == "critic-3")
    );
    assert_repair_context(&model, 7);
}

#[tokio::test]
async fn critic_3_can_record_a_target_revision_with_an_explicit_disposition() {
    let model = model(Repair::ExplicitRevision);
    let report = run(&model, &fixture().0, &config(Mode::Hybrid))
        .await
        .unwrap();
    let objection = report
        .argument
        .defeaters
        .iter()
        .find(|objection| objection.id == "critic-3")
        .unwrap();
    assert_eq!(objection.status, DefeaterStatus::InapplicableAfterRevision);
    assert_eq!(objection.challenge, CHALLENGES[2]);
    assert!(objection.response.contains("revised current claim"));
    assert_repair_context(&model, 7);
}

#[tokio::test]
async fn a_silent_critic_3_target_rewrite_still_fails_after_the_one_allowed_repair() {
    let model = model(Repair::SilentRevision);
    let error = run(&model, &fixture().0, &config(Mode::Hybrid))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::InvalidOutput(message)
        if message.contains("DISSENT_TARGET_CHANGED")
            && message.contains("critic-3")
            && message.contains("target.qualifier")
            && message.contains("after 1 repair calls (max-repairs 1)")));
    assert_repair_context(&model, 7);
}

#[tokio::test]
async fn single_mode_repairs_receive_the_complete_retained_identity_rule() {
    let model = model(Repair::PreserveTarget);
    run(&model, &fixture().0, &config(Mode::Single))
        .await
        .unwrap();
    assert_repair_context(&model, 2);
}

#[tokio::test]
async fn refining_an_assumption_investigation_preserves_the_open_critic_objection() {
    let model = model(Repair::RefineInvestigation);
    let report = run(&model, &fixture().0, &config(Mode::Hybrid))
        .await
        .unwrap();
    let objection = report
        .argument
        .defeaters
        .iter()
        .find(|objection| objection.id == "critic-3")
        .unwrap();
    assert_eq!(objection.status, DefeaterStatus::Open);
    assert!(objection.material);
    assert_eq!(objection.target_id, "A1");
    assert_eq!(
        report.argument.assumptions[0].statement,
        fixture().1.assumptions[0].statement
    );
    assert!(
        report.argument.assumptions[0]
            .validation
            .contains("fixture ownership map")
    );
    assert_repair_context(&model, 7);
}
