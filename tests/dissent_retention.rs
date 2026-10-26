//! Revisions preserve dissent first identified during synthesis.

use engarg::{
    Error,
    domain::{ArgumentCase, DefeaterStatus, Request, validate_case},
    engine::{RunConfig, run},
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};
use serde_json::{Value, json};
use std::sync::Mutex;

const CHALLENGE: &str = "The proposed claim may be read as applying to unmeasured runners.";

#[derive(Clone, Copy)]
enum Revision {
    Drop,
    Downgrade,
    NarrowScope,
    StillApplies,
}

struct Model {
    specialist_objection: bool,
    revision: Revision,
    calls: Mutex<Vec<CompletionRequest>>,
}

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let role = request.role;
        let prompt: Value = serde_json::from_str(&request.user).unwrap();
        self.calls.lock().unwrap().push(request);
        let content = if matches!(role, Role::Synthesiser | Role::Adjudicator) {
            let (request, mut case) = fixture();
            if role == Role::Synthesiser || !matches!(self.revision, Revision::Drop) {
                let mut defeater = case.defeaters[0].clone();
                defeater.id = "sceptic-1".into();
                defeater.target_id = "current_claim".into();
                defeater.challenge = CHALLENGE.into();
                defeater.material =
                    role == Role::Synthesiser || !matches!(self.revision, Revision::Downgrade);
                if role == Role::Adjudicator && matches!(self.revision, Revision::NarrowScope) {
                    defeater.status = DefeaterStatus::InapplicableAfterRevision;
                    defeater.response = "The current claim now applies only to the paired runs on the identified runner. Generalisation to other runners remains outside that revised scope.".into();
                    case.current_claim.qualifier =
                        "Restricted to paired runs on the identified runner configuration.".into();
                }
                if role == Role::Adjudicator
                    && matches!(
                        self.revision,
                        Revision::StillApplies | Revision::NarrowScope
                    )
                {
                    if matches!(self.revision, Revision::StillApplies) {
                        case.current_claim.qualifier.push_str(
                            " Applicability still depends on the identified runner configuration.",
                        );
                    }
                    let origin = prompt["dissent_revision_candidates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|choice| choice["objection_id"] == "sceptic-1")
                        .unwrap();
                    case.dissent_revisions = Some(serde_json::from_value(json!({
                        "schema":"engarg.dissent-revisions/1", "applications":[{
                            "objection_id":"sceptic-1", "origin_context_sha256":origin["original_context_sha256"],
                            "effect":if matches!(self.revision, Revision::StillApplies) {"still_applies"} else {"removed_by_revision"}, "rationale":defeater.response
                        }]
                    })).unwrap());
                }
                let mut need = case.evidence_plan[0].clone();
                need.id = "scope-review".into();
                need.target_id = defeater.id.clone();
                need.enables_claim = "current_claim".into();
                case.evidence_plan.push(need);
                case.defeaters.push(defeater);
            }
            // Each revision is structurally valid. The workflow must additionally
            // enforce dissent preservation across these otherwise valid cases.
            if case.dissent_revisions.is_none() {
                assert_eq!(validate_case(&request, &case), Ok(()));
            }
            serde_json::to_string(&case)?
        } else {
            json!({
                "findings": [],
                "objections": if role == Role::Sceptic && self.specialist_objection {
                    vec![CHALLENGE]
                } else {
                    vec![]
                },
                "evidence_gaps": []
            })
            .to_string()
        };
        Ok(Completion {
            content,
            model: "dissent-transition-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

fn model(specialist_objection: bool, revision: Revision) -> Model {
    Model {
        specialist_objection,
        revision,
        calls: Mutex::new(Vec::new()),
    }
}

#[tokio::test]
async fn adjudication_cannot_drop_an_objection_introduced_only_in_synthesis() {
    let model = model(false, Revision::Drop);
    let error = run(&model, &fixture().0, &RunConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::InvalidOutput(message)
        if message.contains("omitted or altered retained objection sceptic-1")));
    let calls = model.calls.lock().unwrap();
    for call in calls
        .iter()
        .filter(|call| matches!(call.role, Role::FinalCritic | Role::Adjudicator))
    {
        let data: Value = serde_json::from_str(&call.user).unwrap();
        assert!(
            data["retained_objections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|objection| objection["id"] == "sceptic-1"
                    && objection["challenge"] == CHALLENGE
                    && objection["source_role"] == "synthesiser"
                    && objection["must_remain_material"] == true)
        );
    }
}

#[tokio::test]
async fn adjudication_cannot_downgrade_an_existing_objection_marked_material_in_synthesis() {
    let model = model(true, Revision::Downgrade);
    let error = run(&model, &fixture().0, &RunConfig::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::InvalidOutput(message)
        if message.contains("revision downgraded materiality of retained objection sceptic-1")));
}

#[tokio::test]
async fn a_scope_revision_can_change_disposition_while_preserving_the_material_challenge() {
    let model = model(false, Revision::NarrowScope);
    let report = run(&model, &fixture().0, &RunConfig::default())
        .await
        .unwrap();
    let defeater = report
        .argument
        .defeaters
        .iter()
        .find(|defeater| defeater.id == "sceptic-1")
        .unwrap();
    assert_eq!(defeater.challenge, CHALLENGE);
    assert!(defeater.material);
    assert_eq!(defeater.status, DefeaterStatus::InapplicableAfterRevision);
    assert!(defeater.response.contains("identified runner"));
}

#[tokio::test]
async fn a_revised_target_can_retain_open_dissent_with_persisted_original_context() {
    let model = model(false, Revision::StillApplies);
    let report = run(&model, &fixture().0, &RunConfig::default())
        .await
        .unwrap();
    let history = report.argument.dissent_revisions.as_ref().unwrap();
    let origin = history
        .origins
        .iter()
        .find(|origin| origin.objection_id == "sceptic-1")
        .unwrap();
    assert_eq!(
        origin.target_context["target"]["qualifier"],
        fixture().1.current_claim.qualifier
    );
    assert!(!history.applications[0].current_context_sha256.is_empty());
    assert!(
        report
            .argument
            .defeaters
            .iter()
            .any(|d| d.id == "sceptic-1" && d.status == DefeaterStatus::Open && d.material)
    );
    assert_eq!(validate_case(&report.request, &report.argument), Ok(()));
    assert_eq!(report.transcript.len(), 6);
}
