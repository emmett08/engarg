use super::*;
use crate::support_requirements::{
    AcquisitionBlueprint,
    tests::{case, pending},
};
use eal_collector_contracts::{capability::*, execution::*};
use serde_json::json;
use std::collections::BTreeMap;

fn fixture() -> (
    ArgumentCase,
    SupportElaboration,
    CapabilityCatalogue,
    PlanningConfig,
    DiscoverySnapshot,
) {
    let case = case();
    let mut elaboration = pending(&case);
    let contract:SupportContract=serde_json::from_value(json!({
        "schema":SUPPORT_CONTRACT_SCHEMA,"support_type":"calculation",
        "scope":{"kind":"expression","subject":"configured expression","selectors":{"revision":"r1"},"identity_dimensions":["revision"]},
        "acceptable_methods":["exact-calculation/1"],"quantities":[{"name":"result","unit":"1"}],"logical_strength":"exact_calculation","population":null,
        "time":{"historical_interval":null,"max_age_seconds":60,"max_uncertainty_seconds":null,"require_original_observation_time":true},
        "provenance":{"required_authorities":["calculator"],"transformation_history":true},"expected_output_schema_sha256":null
    })).unwrap();
    elaboration.requirements[0].contract = Some(contract.clone());
    elaboration.requirements[0].gaps.clear();
    elaboration.requirements[0].acceptance = EvaluationCriterion::Equals { expected: json!(7) };
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "compute".into(),
        contract,
        arguments: json!({"revision":"r1","expression":"3+4"}),
        bindings: vec![],
        for_each: None,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "compute".into(),
        pointer: "/result".into(),
    }];
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    elaboration.evaluations[0].max_age_seconds = Some(60);
    let input = json!({"type":"object","properties":{"revision":{"type":"string"},"expression":{"type":"string"}},"required":["revision","expression"],"additionalProperties":false});
    let output = json!({"type":"object","properties":{"value":{"type":"integer"}},"required":["value"],"additionalProperties":false});
    let descriptor:CapabilityDescriptor=serde_json::from_value(json!({
        "id":"calculator","version":"1","description":"a qualified calculator",
        "provider":{"id":"calculator","version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"public","configuration_sha256":"a".repeat(64)},
        "input_schema":input,"input_schema_sha256":schema_identity(&input).unwrap(),"output_schema":output,"output_schema_sha256":schema_identity(&output).unwrap(),
        "semantics":{"qualification_reference":"tested-exact-arithmetic/1","support_types":["calculation"],"subject_kinds":["expression"],"methods":["exact-calculation/1"],"identity_dimensions":["revision"],"scope_constraints":{},"scope_bindings":{"revision":"/revision"},"quantities":[{"name":"result","unit":"1","output_pointer":"/value"}],"coverage":[{"strength":"exact_calculation","population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},"provenance":{"authorities":["calculator"],"transformation_history":true},"limitations":[]},
        "configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},"costs":CapabilityCosts::unknown("operator estimates not supplied")
    })).unwrap();
    let catalogue = CapabilityCatalogue {
        schema: CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities: vec![descriptor],
    };
    let config = PlanningConfig {
        schema: PLANNING_CONFIG_SCHEMA.into(),
        collector: "/usr/bin/eal-collector".into(),
        connections: "/operator/connections.json".into(),
        store: "/operator/store".into(),
        environment: vec![],
        ranking: RankingPolicy {
            dimensions: vec![
                CostDimension::ExecutionRequests,
                CostDimension::TransferBytes,
            ],
        },
        scheduling: SchedulingPolicy::default(),
        bindings: vec![],
    };
    let discovery = DiscoverySnapshot {
        budget_usage: json!({"requests_upper_bound":1,"bytes_upper_bound":0,"pages_by_connection":{}}),
        limits: json!({"requests":64,"bytes":4194304,"candidates":128,"pages_per_connection":16,"timeout_seconds":60}),
        admitted_capabilities: vec!["calculator".into()],
        incomplete: false,
        gaps: vec![],
        metrics: json!({"requests":1}),
    };
    (case, elaboration, catalogue, config, discovery)
}
#[test]
fn qualified_outputs_project_without_changing_acceptance_or_original_requirements() {
    let (case, elaboration, catalogue, config, discovery) = fixture();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(plan.routes[0].selected.as_deref(), Some("calculator"));
    assert!(plan.routes[0].configuration_gaps.is_empty());
    assert_eq!(
        canonical_identity(&plan.execution.evaluations[0].criterion).unwrap(),
        canonical_identity(&elaboration.evaluations[0].criterion).unwrap()
    );
    assert_ne!(plan.execution.evaluations[0].inputs[0].step_id, "compute");
    assert!(
        matches!(&plan.execution.derivations[0].operation,DerivationOperation::Project{fields} if fields.get("result")==Some(&"/value".to_owned()))
    );
}
#[test]
fn missing_credentials_and_conflicting_scope_do_not_generate_tools() {
    let (case, elaboration, mut catalogue, mut config, mut discovery) = fixture();
    catalogue.capabilities[0]
        .configuration_requirements
        .push(ConfigurationPrerequisite {
            id: "token".into(),
            kind: ConfigurationKind::CredentialReference,
            description: "Set the operator token reference".into(),
            required: true,
        });
    discovery.admitted_capabilities.clear();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.gaps
            .iter()
            .any(|g| g.kind == GapKind::Configuration && g.description.contains("token"))
    );
    assert!(plan.gaps.iter().all(|g| g.implementation.is_none()));
    config.bindings.push(CapabilityInputBinding {
        capability_id: "calculator".into(),
        arguments: json!({"revision":"prod","expression":"3+4"}),
        input_bindings: BTreeMap::new(),
        configured_prerequisites: vec![],
    });
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.routes[0]
            .configuration_gaps
            .iter()
            .any(|g| g.contains("conflicts"))
    );
}
#[test]
fn inadequate_semantics_are_mismatches_and_discovery_boundary_blocks_generation() {
    let (case, elaboration, mut catalogue, config, mut discovery) = fixture();
    catalogue.capabilities[0]
        .semantics
        .as_mut()
        .unwrap()
        .quantities[0]
        .unit = "ms".into();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.gaps
            .iter()
            .any(|g| g.kind == GapKind::CapabilityMismatch)
    );
    assert!(plan.routes[0].selected.is_none());
    assert!(plan.gaps.iter().all(|g| g.implementation.is_none()));
    catalogue.capabilities.clear();
    discovery.incomplete = true;
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.gaps
            .iter()
            .any(|g| g.kind == GapKind::DiscoveryIncomplete)
    );
    assert!(plan.gaps.iter().all(|g| g.implementation.is_none()));
}
#[test]
fn missing_operation_request_is_parameterised_and_reusable_across_consumer_scope() {
    let (case, mut elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities.clear();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let request = plan
        .gaps
        .iter()
        .find_map(|gap| gap.implementation.as_ref())
        .unwrap();
    assert!(request.support_contract.scope.selectors.is_empty());
    request.validate().unwrap();
    assert!(request.support_contract.time.max_age_seconds.is_none());
    elaboration.acquisitions[0].arguments["revision"] = json!("r2");
    elaboration.acquisitions[0]
        .contract
        .scope
        .selectors
        .insert("revision".into(), json!("r2"));
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .scope
        .selectors
        .insert("revision".into(), json!("r2"));
    elaboration.requirements[0].acceptance = EvaluationCriterion::Equals { expected: json!(8) };
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    let changed = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let other = changed
        .gaps
        .iter()
        .find_map(|gap| gap.implementation.as_ref())
        .unwrap();
    assert_eq!(
        canonical_identity(request).unwrap(),
        canonical_identity(other).unwrap()
    );
}
#[test]
fn cost_ranking_never_uses_transport_preference_and_records_unknown_estimates() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    let mut cheaper = catalogue.capabilities[0].clone();
    cheaper.id = "http-calculator".into();
    cheaper.provider.transport = TransportKind::Http;
    cheaper.costs.execution_requests = CostEstimate::declared(1, "operator benchmark");
    catalogue.capabilities.push(cheaper);
    let mut discovery = discovery;
    discovery
        .admitted_capabilities
        .push("http-calculator".into());
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(plan.routes[0].selected.as_deref(), Some("http-calculator"));
    assert!(!plan.routes[0].ranked[0].unknown_dimensions.is_empty());
}

#[test]
fn unrelated_catalogue_does_not_hide_a_missing_transformation() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[0]
        .semantics
        .as_mut()
        .unwrap()
        .subject_kinds = vec!["unrelated_subject".into()];
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let gap = plan
        .gaps
        .iter()
        .find(|gap| gap.acquisition_id.as_deref() == Some("compute"))
        .unwrap();
    assert_eq!(gap.kind, GapKind::TransformationImplementation);
    let implementation = gap.implementation.as_ref().unwrap();
    assert_eq!(
        implementation.output_schema["properties"]["collected"]["type"],
        "boolean"
    );
    assert!(
        implementation.output_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("collected"))
    );
}

#[test]
fn configured_adequate_route_is_selected_before_pending_configuration() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    let mut unavailable = catalogue.capabilities[0].clone();
    unavailable.id = "cheap-unconfigured".into();
    unavailable.costs.execution_requests = CostEstimate::declared(0, "declared request estimate");
    catalogue.capabilities.push(unavailable);
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(plan.routes[0].ranked[0].capability_id, "cheap-unconfigured");
    assert_eq!(plan.routes[0].selected.as_deref(), Some("calculator"));
    assert!(
        plan.routes[0]
            .candidates
            .iter()
            .any(|candidate| candidate.capability_id == "cheap-unconfigured"
                && !candidate.executable
                && !candidate.configuration_gaps.is_empty())
    );
}

#[test]
fn generated_interfaces_preserve_arbitrary_integers_and_heterogeneous_inputs() {
    let (_, mut elaboration, _, _, _) = fixture();
    elaboration.acquisitions[0].arguments = eal_collector_contracts::strict_json_exact(
        br#"{"revision":"r1","items":[123456789012345678901234567890,true,"trace"]}"#,
        Default::default(),
    )
    .unwrap();
    let request = implementation::request(&elaboration.acquisitions[0]).unwrap();
    request.validate().unwrap();
    let items = &request.input_schema["properties"]["items"]["items"]["anyOf"];
    assert_eq!(items.as_array().unwrap().len(), 3);
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|schema| schema["type"] == "integer")
    );
    elaboration.acquisitions[0].contract.quantities.clear();
    let request = implementation::request(&elaboration.acquisitions[0]).unwrap();
    request.validate().unwrap();
    assert!(
        request.output_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("value"))
    );
}
