use eal_collector_contracts::{
    capability::{
        CapabilityDescriptor, SUPPORT_CONTRACT_SCHEMA, SupportContract, SupportType,
        schema_identity,
    },
    execution::{
        AcquisitionStep, EvaluationCriterion, EvidenceExecutionPlan, PLAN_SCHEMA, SourceIdentity,
    },
};
use engarg::{
    domain::{ArgumentCase, Request},
    support_export::{self, AcquisitionRouteProof, SupportExportLocations},
    support_requirements::{
        AcquisitionBlueprint, RequirementGap, RequirementGapKind, SupportElaboration,
        SupportRequirement, material_targets,
    },
};
use serde_json::json;

pub fn fixture() -> (
    Request,
    ArgumentCase,
    SupportElaboration,
    EvidenceExecutionPlan,
    Vec<AcquisitionRouteProof>,
    SupportExportLocations,
) {
    let request: Request =
        serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
    let case: ArgumentCase =
        serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in material_targets(&case) {
        let mut needs: Vec<_> = case
            .evidence_plan
            .iter()
            .filter(|need| need.target_id == target)
            .collect();
        needs.sort_by_key(|need| &need.id);
        let id = format!("requirement_{target}");
        let criterion = EvaluationCriterion::Unavailable {
            reason: "The independent source is not yet supplied".into(),
        };
        requirements.push(SupportRequirement {
            id: id.clone(),
            target_id: target,
            evidence_need_ids: needs.iter().map(|need| need.id.clone()).collect(),
            original_acceptance_criteria: needs
                .iter()
                .map(|need| need.acceptance_criterion.clone())
                .collect(),
            contract: None,
            discriminating_evidence: vec![
                "Check the opposing result against the same scope".into(),
            ],
            acceptance: criterion.clone(),
            acceptance_rationale: "Unavailable support must remain an explicit gap".into(),
            gaps: vec![RequirementGap {
                kind: RequirementGapKind::SourceUnavailable,
                description: "The independent source is not yet supplied".into(),
            }],
        });
        evaluations.push(serde_json::from_value(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[],"criterion":criterion,"max_age_seconds":null})).unwrap());
    }
    let contract:SupportContract = serde_json::from_value(json!({
        "schema":SUPPORT_CONTRACT_SCHEMA,"support_type":"observation",
        "scope":{"kind":"fixture","subject":"one bounded source","selectors":{"case":"dev"},"identity_dimensions":["case"]},
        "acceptable_methods":["fixture/1"],"quantities":[{"name":"value","unit":"count"}],"logical_strength":"existential_witness","population":null,
        "time":{"historical_interval":null,"max_age_seconds":300,"max_uncertainty_seconds":null,"require_original_observation_time":true},
        "provenance":{"required_authorities":["fixture"],"transformation_history":true},"expected_output_schema_sha256":null,
    })).unwrap();
    let source:SourceIdentity = serde_json::from_value(json!({
        "provider_id":"fixture","capability_id":"bounded_source","capability_version":"1",
        "input_schema_sha256":"a".repeat(64),"output_schema_sha256":"b".repeat(64),
        "qualified_contract_sha256":"e".repeat(64),
        "configuration_sha256":"c".repeat(64),"authentication_scope_sha256":"d".repeat(64),"scope":{"case":"dev"},
    })).unwrap();
    let acquisition: AcquisitionStep = serde_json::from_value(
        json!({"id":"shared_source","source":source,"arguments":{"case":"dev"}}),
    )
    .unwrap();
    for index in 0..3 {
        let id = format!("shared_requirement_{index}");
        let criterion = EvaluationCriterion::Equals {
            expected: json!(index),
        };
        requirements.push(SupportRequirement {
            id: id.clone(),
            target_id: "G1".into(),
            evidence_need_ids: Vec::new(),
            original_acceptance_criteria: Vec::new(),
            contract: Some(contract.clone()),
            discriminating_evidence: vec![
                "Evaluate the same source under a distinct bounded criterion".into(),
            ],
            acceptance: criterion.clone(),
            acceptance_rationale: "A separate machine predicate evaluates this source".into(),
            gaps: Vec::new(),
        });
        evaluations.push(serde_json::from_value(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[{"step_id":"shared_source","pointer":"/value"}],"criterion":criterion,"max_age_seconds":300,"required_coverage":"witness"})).unwrap());
    }
    for (id, support_type) in [
        ("typed_calculation", SupportType::Calculation),
        ("typed_proof", SupportType::ProofCheck),
    ] {
        let mut typed = contract.clone();
        typed.support_type = support_type;
        let criterion = EvaluationCriterion::Unavailable {
            reason: "The separately qualified typed source has not been supplied".into(),
        };
        requirements.push(SupportRequirement{id:id.into(),target_id:"G1".into(),evidence_need_ids:vec![],original_acceptance_criteria:vec![],contract:Some(typed),discriminating_evidence:vec![],acceptance:criterion.clone(),acceptance_rationale:"A shared empirical observation cannot stand in for a separately qualified calculation or proof".into(),gaps:vec![]});
        evaluations.push(serde_json::from_value(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[],"criterion":criterion,"max_age_seconds":300,"required_coverage":"witness"})).unwrap());
    }
    let judgement_id = "retained_judgement";
    let criterion = EvaluationCriterion::Judgement {
        criterion: "Independent adequacy review with retained reasons".into(),
        scope: None,
    };
    requirements.push(SupportRequirement {
        id: judgement_id.into(),
        target_id: "W1".into(),
        evidence_need_ids: Vec::new(),
        original_acceptance_criteria: Vec::new(),
        contract: None,
        discriminating_evidence: Vec::new(),
        acceptance: criterion.clone(),
        acceptance_rationale: "Arbitrary interpretation is not a machine predicate".into(),
        gaps: vec![RequirementGap {
            kind: RequirementGapKind::Judgement,
            description: "A retained independent assessment is required".into(),
        }],
    });
    evaluations.push(serde_json::from_value(json!({"id":"evaluation_retained_judgement","requirement_id":judgement_id,"inputs":[],"criterion":criterion})).unwrap());
    let mut elaboration = SupportElaboration {
        schema: engarg::support_requirements::LEGACY_SUPPORT_ELABORATION_SCHEMA.into(),
        requirements,
        acquisitions: vec![AcquisitionBlueprint {
            id: "shared_source".into(),
            contract,
            arguments: json!({"case":"dev"}),
            bindings: Vec::new(),
            for_each: None,

            operation_spec: None,
        }],
        derivations: Vec::new(),
        evaluations: evaluations.clone(),
        revisions: Vec::new(),
        acquisition_revisions: Vec::new(),

        alternative_support: vec![],
    };
    // Case-level alternatives lack a separately established narrower scope.
    // Preserve their discriminating obligations on both original root claims.
    for alternative in engarg::support_requirements::alternative_obligations(&case) {
        elaboration
            .requirements
            .iter_mut()
            .find(|requirement| requirement.target_id == "G2")
            .unwrap()
            .discriminating_evidence
            .push(alternative.distinguishing_test.clone());
        elaboration
            .alternative_support
            .push(engarg::support_requirements::AlternativeSupport {
                alternative_id: alternative.alternative_id,
                distinguishing_test: alternative.distinguishing_test,
                claim_ids: vec!["current_claim".into(), "stronger_target".into()],
                requirement_ids: vec!["requirement_G2".into()],
            });
    }
    let mut plan = EvidenceExecutionPlan {
        schema: PLAN_SCHEMA.into(),
        acquisitions: vec![acquisition],
        derivations: Vec::new(),
        evaluations,
        scheduling: Default::default(),
    };
    let locations = SupportExportLocations {
        collector: "/operator/bin/eal-collector".into(),
        execution_plan: "/operator/run/execution.json".into(),
        connections: "/operator/run/evidence-connections.json".into(),
        store: "/operator/evidence-store".into(),
        environment: Vec::new(),
        review_binding: None,
    };
    let capability = descriptor();
    plan.acquisitions[0].source.authentication_scope_sha256 =
        eal_collector_contracts::capability::canonical_identity(
            &capability.provider.authentication_scope,
        )
        .unwrap();
    plan.acquisitions[0].source.input_schema_sha256 = capability.input_schema_sha256.clone();
    plan.acquisitions[0].source.output_schema_sha256 = capability.output_schema_sha256.clone();
    plan.acquisitions[0].source.qualified_contract_sha256 =
        capability.measurement_identity().unwrap();
    let routes = vec![AcquisitionRouteProof {
        batch: None,
        blueprint_id: "shared_source".into(),
        capability,
        projection_id: Some("qualified_quantities".into()),
        input_bindings: Default::default(),
        operator_arguments: None,
    }];
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    (request, case, elaboration, plan, routes, locations)
}
pub fn descriptor() -> CapabilityDescriptor {
    let schema = json!({"type":"object"});
    let estimate = json!({"value":null,"basis":"unknown","reference":"fixture qualification"});
    serde_json::from_value(json!({
        "id":"bounded_source","version":"1","description":"Qualified bounded source",
        "provider":{"id":"fixture","version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"fixture","configuration_sha256":"c".repeat(64)},
        "input_schema":schema,"input_schema_sha256":schema_identity(&schema).unwrap(),"output_schema":schema,"output_schema_sha256":schema_identity(&schema).unwrap(),
        "semantics":{"qualification_reference":"fixture-contract/1","support_types":["observation"],"subject_kinds":["fixture"],"methods":["fixture/1"],"identity_dimensions":["case"],"scope_constraints":{},"scope_bindings":{"case":"/case"},"quantities":[{"name":"value","unit":"count","output_pointer":"/raw/measured"}],"coverage":[{"strength":"existential_witness","population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},"provenance":{"authorities":["fixture"],"transformation_history":true},"limitations":[]},
        "configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},
        "costs":{"configuration_effort_seconds":estimate,"implementation_effort_seconds":estimate,"execution_requests":estimate,"execution_latency_ms":estimate,"monetary_microunits":estimate,"transfer_bytes":estimate},
    })).unwrap()
}
