//! Explicitly unavailable obligations for authoring-provider integration fixtures.
use engarg::{
    domain::ArgumentCase,
    support_requirements::{
        SUPPORT_ELABORATION_SCHEMA, SupportElaboration, alternative_obligations, material_targets,
    },
};
use serde_json::json;

pub fn pending(case: &ArgumentCase) -> SupportElaboration {
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in material_targets(case) {
        let mut needs = case
            .evidence_plan
            .iter()
            .filter(|need| need.target_id == target)
            .collect::<Vec<_>>();
        needs.sort_by_key(|need| &need.id);
        let id = format!("requirement_{target}");
        let role = if case.defeaters.iter().any(|defeater| defeater.id == target) {
            "challenge_resolution"
        } else {
            "support"
        };
        let criterion = json!({"kind":"unavailable","reason":"The source and method require operator clarification."});
        requirements.push(json!({
            "id":id,"role":role,"target_id":target,"evidence_need_ids":needs.iter().map(|need|&need.id).collect::<Vec<_>>(),
            "original_acceptance_criteria":needs.iter().map(|need|&need.acceptance_criterion).collect::<Vec<_>>(),
            "contract":null,"discriminating_evidence":["Retain adverse findings against the same scope."],
            "acceptance":criterion,"acceptance_rationale":"Source availability has not been established.",
            "gaps":[{"kind":"clarification","description":"Declare the source and qualified method before acquisition."}],
        }));
        evaluations.push(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[],"criterion":criterion,"max_age_seconds":null,"required_coverage":"unspecified","applicability":[],"challenges":[]}));
    }
    let alternative_support=alternative_obligations(case).into_iter().map(|alternative| {
        let requirement=requirements.last_mut().unwrap();
        requirement["discriminating_evidence"].as_array_mut().unwrap().push(json!(alternative.distinguishing_test));
        json!({"alternative_id":alternative.alternative_id,"distinguishing_test":alternative.distinguishing_test,"claim_ids":alternative.claim_ids,"requirement_ids":[requirement["id"]]})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({"schema":SUPPORT_ELABORATION_SCHEMA,"requirements":requirements,"acquisitions":[],"derivations":[],"evaluations":evaluations,"revisions":[],"alternative_support":alternative_support})).unwrap()
}
