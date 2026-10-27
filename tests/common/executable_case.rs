//! Shared finite executable authoring case; tool output carries no formula.
use super::{reasoning, reasoning_graph};
use eal_collector_contracts::execution::{EvaluationCriterion, RequiredCoverage};
use engarg::{
    domain::{ArgumentCase, Request},
    support_requirements::SupportElaboration,
};
use serde_json::json;

pub fn fixture() -> (Request, ArgumentCase, SupportElaboration) {
    let mode = reasoning::cases().remove(0);
    let (request, case, mut elaboration, _, _) = reasoning_graph::graph_fixture(&mode);
    elaboration.acquisitions.retain(|a| a.id != "formal_source");
    let observation = elaboration
        .requirements
        .iter_mut()
        .find(|r| r.target_id == "G1")
        .unwrap();
    observation.acceptance = EvaluationCriterion::Equals {
        expected: json!({"count":3,"distinct":true}),
    };
    elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == observation.id)
        .unwrap()
        .criterion = observation.acceptance.clone();
    for operation in &mut elaboration.acquisitions {
        operation.arguments = json!({"fixture":"frozen-fixture-v1", "operation":operation.id});
    }
    // This synthetic fixture defines its applicability condition exactly. The
    // authored formula is in EAL and is never acquired from a tool.
    let warrant = elaboration
        .requirements
        .iter_mut()
        .find(|r| r.target_id == "W1")
        .unwrap();
    warrant.acceptance = EvaluationCriterion::AllTrue;
    warrant.contract = Some(reasoning_graph::contract(
        "observation",
        "existential_witness",
        "applies",
        "boolean",
    ));
    let evaluation = elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == warrant.id)
        .unwrap();
    evaluation.criterion = EvaluationCriterion::AllTrue;
    evaluation.inputs = vec![
        serde_json::from_value(json!({"step_id":"applicability_source","pointer":"/applies"}))
            .unwrap(),
    ];
    evaluation.required_coverage = RequiredCoverage::Witness;
    let mut composition = warrant.clone();
    composition.id = "composition_condition".into();
    composition.target_id = "current_claim".into();
    composition.evidence_need_ids.clear();
    composition.original_acceptance_criteria.clear();
    let mut condition = evaluation.clone();
    condition.id = "composition_condition_check".into();
    condition.requirement_id = composition.id.clone();
    elaboration.requirements.push(composition);
    elaboration.evaluations.push(condition);
    (request, case, elaboration)
}
