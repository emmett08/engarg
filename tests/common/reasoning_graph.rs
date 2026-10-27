//! Independently specified graph fixture for native reasoning-method exports.
use super::reasoning::{ModeCase, fixture as argument_fixture};
use eal_collector_contracts::{
    capability::{CapabilityDescriptor, SupportContract, schema_identity},
    execution::{AcquisitionStep, EvaluationCriterion, EvidenceExecutionPlan, SourceIdentity},
};
use engarg::{
    domain::{ArgumentCase, Request},
    support_export::{self, AcquisitionRouteProof, SupportExportLocations},
    support_requirements::{
        AcquisitionBlueprint, SupportElaboration, SupportRequirement, material_targets,
    },
};
use serde_json::json;

pub type GraphFixture = (
    Request,
    ArgumentCase,
    SupportElaboration,
    EvidenceExecutionPlan,
    Vec<AcquisitionRouteProof>,
    SupportExportLocations,
);

pub fn contract(support_type: &str, strength: &str, quantity: &str, unit: &str) -> SupportContract {
    serde_json::from_value(json!({
        "schema":"eal.support-contract/1","support_type":support_type,
        "scope":{"kind":"fixture","subject":"mode-acceptance-test-bench","selectors":{"fixture":"frozen-fixture-v1"},"identity_dimensions":["fixture"]},
        "acceptable_methods":["fixture.retained-record/1"],"quantities":[{"name":quantity,"unit":unit}],
        "logical_strength":strength,"population":null,
        "time":{"historical_interval":null,"max_age_seconds":3600,"max_uncertainty_seconds":null,"require_original_observation_time":true},
        "provenance":{"required_authorities":["fixture"],"transformation_history":true},"expected_output_schema_sha256":null,
    })).unwrap()
}

pub fn capability(id: &str, contract: &SupportContract) -> CapabilityDescriptor {
    let properties: serde_json::Map<_, _> = contract
        .scope
        .selectors
        .iter()
        .map(|(name, value)| {
            let kind = if value.is_boolean() {
                "boolean"
            } else if value.is_number() {
                "number"
            } else {
                "string"
            };
            (name.clone(), json!({"type":kind}))
        })
        .collect();
    let required = contract.scope.selectors.keys().collect::<Vec<_>>();
    let scope_bindings: serde_json::Map<_, _> = contract
        .scope
        .selectors
        .keys()
        .map(|name| {
            (
                name.clone(),
                json!(format!("/{}", name.replace('~', "~0").replace('/', "~1"))),
            )
        })
        .collect();
    let input_schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let output_schema = json!({"type":"object","properties":{"value":{}},"required":["value"]});
    let estimate = json!({"value":null,"basis":"unknown","reference":"deterministic test bench"});
    serde_json::from_value(json!({
        "id":id,"version":"1","description":"Identified frozen test-bench record",
        "provider":{"id":"fixture","version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"fixture","configuration_sha256":"c".repeat(64)},
        "input_schema":input_schema,"input_schema_sha256":schema_identity(&input_schema).unwrap(),
        "output_schema":output_schema,"output_schema_sha256":schema_identity(&output_schema).unwrap(),
        "semantics":{"qualification_reference":"fixture-retained-record/1","support_types":[contract.support_type],"subject_kinds":[contract.scope.kind],"methods":contract.acceptable_methods,"identity_dimensions":contract.scope.identity_dimensions,"scope_constraints":{},"scope_bindings":scope_bindings,"quantities":contract.quantities.iter().map(|q|json!({"name":q.name,"unit":q.unit,"output_pointer":"/value"})).collect::<Vec<_>>(),"coverage":[{"strength":contract.logical_strength,"population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},"provenance":{"authorities":["fixture"],"transformation_history":true},"limitations":["A frozen test-bench record establishes only the identified fixture, not a physical deployment."]},
        "configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},
        "costs":{"configuration_effort_seconds":estimate,"implementation_effort_seconds":estimate,"execution_requests":estimate,"execution_latency_ms":estimate,"monetary_microunits":estimate,"transfer_bytes":estimate},
    })).unwrap()
}

pub fn source(capability: &CapabilityDescriptor) -> SourceIdentity {
    serde_json::from_value(json!({
        "provider_id":capability.provider.id,"capability_id":capability.id,"capability_version":capability.version,
        "input_schema_sha256":capability.input_schema_sha256,"output_schema_sha256":capability.output_schema_sha256,
        "qualified_contract_sha256":capability.measurement_identity().unwrap(),
        "configuration_sha256":capability.provider.configuration_sha256,
        "authentication_scope_sha256":eal_collector_contracts::capability::canonical_identity(&capability.provider.authentication_scope).unwrap(),
        "scope":{"fixture":"frozen-fixture-v1"},
    })).unwrap()
}

pub fn graph_fixture(mode: &ModeCase) -> GraphFixture {
    let (request, case) = argument_fixture(mode);
    let input_contract = contract("source_retrieval", "clarification", "typed_input", "json");
    let premise_contract = contract(
        "observation",
        "existential_witness",
        "measurement",
        mode.unit,
    );
    let applicability_contract =
        contract("observation", "existential_witness", "applies", "boolean");
    let sources = [
        ("formal_source", input_contract.clone()),
        ("premise_source", premise_contract.clone()),
        ("applicability_source", applicability_contract.clone()),
    ];
    let acquisitions = sources
        .iter()
        .map(|(id, contract)| AcquisitionBlueprint {
            id: (*id).into(),
            contract: contract.clone(),
            arguments: json!({"fixture":"frozen-fixture-v1"}),
            bindings: vec![],
            for_each: None,
            operation_spec: None,
        })
        .collect::<Vec<_>>();
    let routes = sources
        .iter()
        .map(|(id, contract)| AcquisitionRouteProof {
            composition: None,
            batch: None,
            blueprint_id: (*id).into(),
            capability: capability(id, contract),
            projection_id: Some(format!("{id}_quantities")),
            input_bindings: Default::default(),
            operator_arguments: None,
        })
        .collect::<Vec<_>>();
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in material_targets(&case) {
        let mut needs = case
            .evidence_plan
            .iter()
            .filter(|need| need.target_id == target)
            .collect::<Vec<_>>();
        needs.sort_by_key(|need| &need.id);
        let (contract, criterion, reference, coverage) = match target.as_str() {
            "W1" => (
                Some(input_contract.clone()),
                EvaluationCriterion::MethodInput {
                    method: format!("{}/1", mode.kind),
                },
                Some(("formal_source", "/typed_input")),
                "unspecified",
            ),
            "G1" => (
                Some(premise_contract.clone()),
                EvaluationCriterion::Equals {
                    expected: mode.payload.clone(),
                },
                Some(("premise_source", "/measurement")),
                "witness",
            ),
            "A1" | "current_claim" => (
                Some(applicability_contract.clone()),
                EvaluationCriterion::AllTrue,
                Some(("applicability_source", "/applies")),
                "witness",
            ),
            _ => (
                None,
                EvaluationCriterion::Unavailable {
                    reason: "The wider physical deployment has not been observed.".into(),
                },
                None,
                "unspecified",
            ),
        };
        let id = format!("requirement_{target}");
        requirements.push(SupportRequirement {
            id:id.clone(),target_id:target,evidence_need_ids:needs.iter().map(|need|need.id.clone()).collect(),original_acceptance_criteria:needs.iter().map(|need|need.acceptance_criterion.clone()).collect(),contract,
            discriminating_evidence:vec![],acceptance:criterion.clone(),acceptance_rationale:"Evaluate only this frozen fixture's retained source and declared method-input format; physical adequacy remains outside the conclusion.".into(),
            gaps:if reference.is_none(){vec![serde_json::from_value(json!({"kind":"source_unavailable","description":"The wider deployment is unobserved."})).unwrap()]}else{vec![]},
        });
        let inputs = reference
            .map(|(step, pointer)| json!([{"step_id":step,"pointer":pointer}]))
            .unwrap_or(json!([]));
        evaluations.push(serde_json::from_value(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":inputs,"criterion":criterion,"max_age_seconds":reference.map(|_|3600),"required_coverage":coverage})).unwrap());
    }
    let elaboration = SupportElaboration {
        schema: engarg::support_requirements::SUPPORT_ELABORATION_SCHEMA.into(),
        requirements,
        acquisitions,
        derivations: vec![],
        evaluations: evaluations.clone(),
        revisions: vec![],
        acquisition_revisions: vec![],
        alternative_support: vec![],
    };
    let mut plan = EvidenceExecutionPlan {
        schema: eal_collector_contracts::execution::PLAN_SCHEMA.into(),
        acquisitions: routes
            .iter()
            .map(|route| AcquisitionStep {
                coverage_proof: None,
                id: route.blueprint_id.clone(),
                source: source(&route.capability),
                arguments: json!({"fixture":"frozen-fixture-v1"}),
                bindings: vec![],
                for_each: None,
                availability: Default::default(),
                reuse: Default::default(),
                max_age_seconds: Some(3600),
            })
            .collect(),
        derivations: vec![],
        evaluations,
        scheduling: Default::default(),
    };
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let errors = engarg::support_requirements::validate_elaboration(&case, &elaboration);
    assert!(errors.is_empty(), "{}", errors.join("; "));
    let locations = SupportExportLocations {
        collector: "/operator/bin/eal-collector".into(),
        execution_plan: "/operator/run/plan.json".into(),
        connections: "/operator/run/connections.json".into(),
        store: "/operator/store".into(),
        environment: vec![],
        review_binding: None,
    };
    (request, case, elaboration, plan, routes, locations)
}
