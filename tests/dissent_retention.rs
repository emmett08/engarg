//! Revisions preserve dissent first identified during synthesis.

#[path = "common/live_output.rs"]
mod live_output;

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
}

struct Model {
    specialist_objection: bool,
    revision: Revision,
    calls: Mutex<Vec<CompletionRequest>>,
}

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        live_output::case(include_str!("../examples/case.json")),
    )
}

impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let role = request.role;
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
                let mut need = case.evidence_plan[0].clone();
                need.id = "scope-review".into();
                need.target_id = defeater.id.clone();
                need.enables_claim = "current_claim".into();
                case.evidence_plan.push(need);
                case.defeaters.push(defeater);
            }
            // Each revision is structurally valid. The workflow must additionally
            // enforce dissent preservation across these otherwise valid cases.
            assert_eq!(validate_case(&request, &case), Ok(()));
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
