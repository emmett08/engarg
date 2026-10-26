//! Routed debates preserve engine contracts, final dissent and bounded execution.

use std::sync::Mutex;

use engarg::{
    Error,
    domain::{ArgumentCase, Request, validate_case},
    engine::{
        Mode, RunConfig, RunReport, run,
        topology::{DebateConfig, Edge, Topology},
    },
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};
use serde_json::{Value, json};

const TOPOLOGIES: [&str; 9] = [
    "fully_connected",
    "grouped",
    "chain",
    "ring",
    "star",
    "tree",
    "layered",
    "sparse",
    "bipartite",
];
const CHALLENGE: &str = "The proposed claim may be read as applying to unmeasured runners.";
const OBJECTION_ID: &str = "agent-1-round-0-1";

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn config(topology: &str) -> RunConfig {
    RunConfig {
        debate: Some(DebateConfig {
            participants: 4,
            rounds: 1,
            topology: Topology::preset(topology, 4).unwrap(),
            adaptation: Default::default(),
        }),
        max_calls: 11,
        ..RunConfig::default()
    }
}

#[derive(Default)]
struct Model {
    calls: Mutex<Vec<CompletionRequest>>,
    dissent: bool,
    drop_until_repair: bool,
}

impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let data: Value = serde_json::from_str(&request.user)?;
        let role = request.role;
        self.calls.lock().unwrap().push(request);
        let content = if matches!(role, Role::Single | Role::Synthesiser | Role::Adjudicator) {
            let (input, mut case) = fixture();
            if self.dissent
                && (role == Role::Synthesiser
                    || !self.drop_until_repair
                    || data.get("repair_attempt").is_some())
            {
                let mut defeater = case.defeaters[0].clone();
                defeater.id = OBJECTION_ID.into();
                defeater.target_id = "current_claim".into();
                defeater.challenge = CHALLENGE.into();
                let mut need = case.evidence_plan[0].clone();
                need.id = "runner-scope-review".into();
                need.target_id = defeater.id.clone();
                need.enables_claim = "current_claim".into();
                case.evidence_plan.push(need);
                case.defeaters.push(defeater);
            }
            assert_eq!(validate_case(&input, &case), Ok(()));
            serde_json::to_string(&case)?
        } else {
            // The concern disappears from the participant's later assessment.
            // The engine must still require its original identity at adjudication.
            let objections = if self.dissent
                && data["debate"]["participant"] == 1
                && data["debate"]["round"] == 0
            {
                vec![CHALLENGE]
            } else {
                vec![]
            };
            json!({"findings":[],"objections":objections,"evidence_gaps":[]}).to_string()
        };
        Ok(Completion {
            content,
            model: "topology-contract-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

#[tokio::test]
async fn every_topology_completes_the_public_engine_and_round_trips_its_report() {
    let (request, expected) = fixture();
    for name in TOPOLOGIES {
        let model = Model::default();
        let config = config(name);
        let report = run(&model, &request, &config).await.unwrap();
        assert_eq!(report.transcript.len(), 11, "{name}");
        assert_eq!(model.calls.lock().unwrap().len(), 11, "{name}");
        assert_eq!(
            serde_json::to_value(&report.argument).unwrap(),
            serde_json::to_value(&expected).unwrap(),
            "{name}"
        );
        let restored: RunReport =
            serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
        assert_eq!(restored.run_config.unwrap().debate, config.debate, "{name}");
        assert_eq!(report.transcript[8].role, Role::Synthesiser);
        assert_eq!(report.transcript[9].role, Role::FinalCritic);
        assert_eq!(report.transcript[10].role, Role::Adjudicator);
    }
}

#[tokio::test]
async fn invalid_debates_and_unreserved_repairs_fail_before_any_provider_call() {
    let mut missing_base_calls = config("ring");
    missing_base_calls.max_calls = 10;
    let mut missing_repair_allowance = config("ring");
    missing_repair_allowance.max_repairs = 1;
    let mut invalid_graph = config("sparse");
    invalid_graph.debate.as_mut().unwrap().topology = Topology::Sparse {
        edges: vec![Edge { from: 0, to: 4 }],
    };
    let mut single = config("chain");
    single.mode = Mode::Single;
    for (config, budget_error) in [
        (missing_base_calls, true),
        (missing_repair_allowance, true),
        (invalid_graph, false),
        (single, false),
    ] {
        let model = Model::default();
        let error = run(&model, &fixture().0, &config).await.unwrap_err();
        if budget_error {
            assert!(matches!(error, Error::Budget(_)), "{error}");
        } else {
            assert!(matches!(error, Error::InvalidInput(_)), "{error}");
        }
        assert!(model.calls.lock().unwrap().is_empty());
    }
}

#[test]
fn legacy_run_configuration_needs_no_debate_field() {
    let legacy = json!({
        "mode":"hybrid", "max_calls":6,
        "max_input_bytes":1048576, "max_response_bytes":131072,
    });
    let restored: RunConfig = serde_json::from_value(legacy).unwrap();
    assert!(restored.debate.is_none());
    assert_eq!(restored.required_calls().unwrap(), 6);
    let serialised = serde_json::to_value(restored).unwrap();
    assert!(serialised.get("debate").is_none());
    let restored: RunConfig = serde_json::from_value(serialised).unwrap();
    assert!(restored.debate.is_none());
}

#[tokio::test]
async fn a_concern_omitted_from_later_peer_assessments_remains_in_the_final_case() {
    let model = Model {
        dissent: true,
        ..Model::default()
    };
    let report = run(&model, &fixture().0, &config("grouped")).await.unwrap();
    let objection = report
        .argument
        .defeaters
        .iter()
        .find(|defeater| defeater.id == OBJECTION_ID)
        .unwrap();
    assert_eq!(objection.challenge, CHALLENGE);
    for record in &report.transcript[8..] {
        let data: Value = serde_json::from_str(&record.prompt.user).unwrap();
        assert!(
            data["retained_objections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|objection| objection["id"] == OBJECTION_ID
                    && objection["challenge"] == CHALLENGE)
        );
    }
    let synthesis: Value = serde_json::from_str(&report.transcript[8].prompt.user).unwrap();
    assert!(
        synthesis["discussion_assessments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|assessment| assessment["assessment"]["objection_ids"]
                .as_array()
                .unwrap()
                .is_empty())
    );
}

#[tokio::test]
async fn adjudication_cannot_lose_peer_dissent_and_reserved_repairs_can_restore_it() {
    let model = Model {
        dissent: true,
        drop_until_repair: true,
        ..Model::default()
    };
    let error = run(&model, &fixture().0, &config("sparse"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::InvalidOutput(message)
        if message.contains(&format!("omitted or altered retained objection {OBJECTION_ID}"))));
    assert_eq!(model.calls.lock().unwrap().len(), 11);

    let model = Model {
        dissent: true,
        drop_until_repair: true,
        ..Model::default()
    };
    let mut config = config("sparse");
    config.max_calls = 12;
    config.max_repairs = 1;
    let report = run(&model, &fixture().0, &config).await.unwrap();
    assert_eq!(report.transcript.len(), 12);
    assert!(
        report
            .argument
            .defeaters
            .iter()
            .any(|defeater| defeater.id == OBJECTION_ID && defeater.challenge == CHALLENGE)
    );
    let repair: Value = serde_json::from_str(&report.transcript[11].prompt.user).unwrap();
    assert_eq!(repair["repair_attempt"], 1);
    assert!(
        repair["validation_errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error.as_str().unwrap().contains(OBJECTION_ID))
    );
}
