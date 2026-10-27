//! Independently authored synthetic review obligations for compiler boundary tests.
use eal_core::{ExecutionLimits, composition, expressions};
use engarg::{
    Error,
    domain::{ArgumentCase, Request},
    eal_export::{EalExport, ExportProjection},
    support_requirements::{self, SupportElaboration},
};
use serde_json::json;

/// The fixture explicitly chooses scoped reviews; production never invents them.
pub fn elaboration(case: &ArgumentCase) -> SupportElaboration {
    let mut targets = support_requirements::material_targets(case);
    targets.extend(case.assumptions.iter().map(|a| a.id.clone()));
    targets.extend(case.defeaters.iter().map(|d| d.id.clone()));
    targets.extend(case.compositions.iter().map(|c| c.conclusion_id.clone()));
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in targets {
        let challenge = case.defeaters.iter().any(|d| d.id == target);
        let roles: &[&str] = if challenge {
            &["challenge_activation", "challenge_resolution"]
        } else {
            &["support"]
        };
        let mut needs = case
            .evidence_plan
            .iter()
            .filter(|n| n.target_id == target)
            .collect::<Vec<_>>();
        needs.sort_by_key(|n| &n.id);
        for role in roles {
            let id = format!("review_{role}_{target}");
            let scope = json!({"kind":"synthetic_review","subject":"Authored fixture node",
                "selectors":{"target":target},"identity_dimensions":["target"]});
            let criterion =
                format!("Independently assess {role} for the exact authored target {target}");
            let acceptance = json!({"kind":"judgement","criterion":criterion,"scope":scope});
            requirements.push(json!({"id":id,"role":role,"target_id":target,
                "evidence_need_ids":needs.iter().map(|n|&n.id).collect::<Vec<_>>(),
                "original_acceptance_criteria":needs.iter().map(|n|&n.acceptance_criterion).collect::<Vec<_>>(),
                "contract":{"schema":"eal.support-contract/1","support_type":"judgement","scope":scope,
                    "acceptable_methods":["structured/1"],"quantities":[{"name":"review","unit":"1"}],
                    "logical_strength":"judgement","population":null,
                    "time":{"historical_interval":null,"max_age_seconds":300,"max_uncertainty_seconds":null,"require_original_observation_time":true},
                    "provenance":{"required_authorities":["independent-fixture-reviewer"],"transformation_history":true},
                    "expected_output_schema_sha256":null},
                "discriminating_evidence":[],"acceptance":acceptance,
                "acceptance_rationale":"A scoped independent review remains required; authored metadata supplies no verdict.",
                "gaps":[]}));
            evaluations.push(json!({"id":format!("evaluation_{id}"),"requirement_id":id,
                "inputs":[],"criterion":acceptance,"max_age_seconds":300}));
        }
    }
    let mut alternatives = Vec::new();
    for alternative in support_requirements::alternative_obligations(case) {
        let id = "review_support_current_claim";
        requirements.iter_mut().find(|r| r["id"] == id).unwrap()["discriminating_evidence"]
            .as_array_mut()
            .unwrap()
            .push(json!(alternative.distinguishing_test));
        alternatives.push(json!({"alternative_id":alternative.alternative_id,
            "distinguishing_test":alternative.distinguishing_test,
            "claim_ids":["current_claim","stronger_target"],"requirement_ids":[id]}));
    }
    serde_json::from_value(
        json!({"schema":support_requirements::SUPPORT_ELABORATION_SCHEMA,
        "requirements":requirements,"acquisitions":[],"derivations":[],"evaluations":evaluations,
        "revisions":[],"acquisition_revisions":[],"alternative_support":alternatives}),
    )
    .unwrap()
}

pub fn compile(request: &Request, case: &ArgumentCase) -> Result<ExportProjection, Error> {
    engarg::support_export::compile_direct(
        request,
        case,
        &elaboration(case),
        &Default::default(),
        &Default::default(),
    )
}

pub fn gate(bundle: &EalExport, target: &str, role: &str) -> String {
    let program =
        composition::compile(&bundle.source, None, "", ExecutionLimits::default()).unwrap();
    program
        .evidence
        .values()
        .find(|e| e.kind == "review" && e.input["target"] == target && e.input["role"] == role)
        .unwrap()
        .name
        .strip_suffix("_review")
        .unwrap()
        .to_owned()
}

pub fn assert_scoped_review_gates(projection: &ExportProjection) {
    let bundle = &projection.bundle;
    let program =
        composition::compile(&bundle.source, None, "", ExecutionLimits::default()).unwrap();
    assert_eq!(
        program
            .evidence
            .values()
            .filter(|e| e.kind == "scoped_review")
            .count(),
        projection.review_requests.len()
    );
    for review in &projection.review_requests {
        let acquired = &program.evidence[&review.evidence_id];
        assert_eq!(acquired.kind, "review");
        assert_eq!(acquired.input, review.input);
        assert_eq!(review.context, bundle.context);
        let declaration = review.evidence_id.strip_suffix("_review").unwrap();
        let gate = &program.evidence[declaration];
        let checked = &program.evidence[&format!("{declaration}_scoped_review")];
        assert_eq!(checked.kind, "scoped_review");
        assert_eq!(checked.input["scope"], review.input["scope"]);
        for verdict in ["supported", "rejected", "inconclusive"] {
            let value = json!({"criterion":review.input["criterion"],"target":review.input["target"],"scope":review.input["scope"],
                "reviewer":"synthetic-gate-reviewer","verdict":verdict,"reasons":["Synthetic predicate regression only."],"reviewed_at":"2026-10-23T12:00:00Z"});
            let result = expressions::evaluate_expression(
                gate.derivation.as_ref().unwrap(),
                &json!({"review":value}),
            )
            .value
            .unwrap();
            assert_eq!(
                result["accepted"],
                verdict == "supported",
                "{}: {verdict}",
                review.evidence_id
            );
        }
    }
}
