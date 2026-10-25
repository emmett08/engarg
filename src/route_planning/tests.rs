use super::*;
use crate::support_requirements::{
    AcquisitionBlueprint, OperationSpecification,
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
        operation_spec: Some(OperationSpecification {
            schema: "engarg.operation-specification/1".into(),
            definition: "Evaluate the supplied arithmetic expression exactly for the identified revision.".into(),
            input_conditions: vec!["expression is a supported exact arithmetic expression".into()],
            output_guarantees: vec!["result is the exact expression value in unit 1".into()],
            input_schema: json!({"type":"object","properties":{"revision":{"type":"string"},"expression":{"type":"string"}},"required":["revision","expression"],"additionalProperties":false}),
            output_schema: json!({"type":"object","properties":{"result":{"type":"integer"}},"required":["result"],"additionalProperties":false}),
        }),
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
        prefer_configured: false,
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
        matches!(&plan.execution.derivations[0].operation,DerivationOperation::ProjectValue{fields} if fields.get("result")==Some(&"/value".to_owned()))
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
    let (case, elaboration, mut catalogue, mut config, discovery) = fixture();
    let mut unavailable = catalogue.capabilities[0].clone();
    config.prefer_configured = true;
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
fn cost_policy_can_select_an_adequate_route_that_still_needs_configuration() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    let mut cheaper = catalogue.capabilities[0].clone();
    cheaper.id = "cheap-unconfigured".into();
    cheaper.costs.execution_requests = CostEstimate::declared(0, "operator estimate");
    catalogue.capabilities.push(cheaper);
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(
        plan.routes[0].selected.as_deref(),
        Some("cheap-unconfigured")
    );
    assert!(!plan.routes[0].configuration_gaps.is_empty());
}

#[test]
fn critic_sees_deduplicated_qualified_batch_shapes_without_operator_arguments() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    let mut batch = catalogue.capabilities[0].clone();
    batch.id = "batch".into();
    batch.input_schema["properties"]["expressions"] =
        json!({"type":"array","items":{"type":"string"},"default":["PRIVATE"]});
    batch.input_schema_sha256 = schema_identity(&batch.input_schema).unwrap();
    catalogue.capabilities.push(batch);
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let planner = Planner {
        catalogue,
        config,
        discovery,
    };
    let preview = planner.preview(&plan).unwrap();
    let interfaces = preview["qualified_alternatives"]["interfaces"]
        .as_array()
        .unwrap();
    assert_eq!(interfaces.len(), 2);
    let batch = interfaces
        .iter()
        .find(|item| item["capability_id"] == "batch")
        .unwrap();
    assert_eq!(
        batch["input_shape"]["properties"]["expressions"]["type"],
        "array"
    );
    assert!(!preview.to_string().contains("PRIVATE"));
    assert!(preview.get("configuration").is_none());
    assert!(preview["qualified_sources"][0].get("semantics").is_none());
}

#[test]
fn joint_cost_selection_shares_a_combined_measurement_across_distinct_quantities() {
    let (case, mut elaboration, mut catalogue, config, mut discovery) = fixture();
    let mut second = elaboration.acquisitions[0].clone();
    second.id = "memory_source".into();
    second.contract.quantities[0].name = "memory".into();
    elaboration.acquisitions[0].contract.quantities[0].name = "cpu".into();
    elaboration.requirements[0].contract = Some(elaboration.acquisitions[0].contract.clone());
    elaboration.evaluations[0].inputs[0].pointer = "/cpu".into();
    elaboration.acquisitions.push(second.clone());
    elaboration.requirements[1].contract = Some(second.contract);
    elaboration.requirements[1].gaps.clear();
    elaboration.requirements[1].acceptance = EvaluationCriterion::Equals { expected: json!(7) };
    elaboration.evaluations[1].criterion = elaboration.requirements[1].acceptance.clone();
    elaboration.evaluations[1].inputs = vec![OutputReference {
        step_id: "memory_source".into(),
        pointer: "/memory".into(),
    }];
    elaboration.evaluations[1].max_age_seconds = Some(60);
    let mut cpu = catalogue.capabilities[0].clone();
    cpu.id = "a_cpu".into();
    cpu.semantics.as_mut().unwrap().quantities[0].name = "cpu".into();
    cpu.costs.execution_requests = CostEstimate::declared(1, "one source request");
    let mut memory = cpu.clone();
    memory.id = "b_memory".into();
    memory.semantics.as_mut().unwrap().quantities[0].name = "memory".into();
    let mut both = cpu.clone();
    both.id = "z_combined".into();
    both.semantics
        .as_mut()
        .unwrap()
        .quantities
        .push(memory.semantics.as_ref().unwrap().quantities[0].clone());
    discovery.admitted_capabilities = vec![cpu.id.clone(), memory.id.clone(), both.id.clone()];
    catalogue.capabilities = vec![cpu, memory, both];
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.routes
            .iter()
            .all(|route| route.selected.as_deref() == Some("z_combined"))
    );
    let selection = plan.selection.unwrap();
    assert!(selection.search_complete);
    assert_eq!(selection.estimated_distinct_acquisitions, Some(1));
    assert_eq!(selection.total_costs[0], (0, 1));
}

#[test]
fn generated_interfaces_preserve_explicit_arbitrary_integer_and_heterogeneous_contracts() {
    let (_, mut elaboration, _, _, _) = fixture();
    elaboration.acquisitions[0].arguments = eal_collector_contracts::strict_json_exact(
        br#"{"revision":"r1","items":[123456789012345678901234567890,true,"trace"]}"#,
        Default::default(),
    )
    .unwrap();
    elaboration.acquisitions[0]
        .operation_spec
        .as_mut()
        .unwrap()
        .input_schema = json!({"type":"object","properties":{"revision":{"type":"string"},"items":{"type":"array","items":{"anyOf":[{"type":"integer"},{"type":"boolean"},{"type":"string"}]}}},"required":["revision","items"],"additionalProperties":false});
    let request = implementation::request(&elaboration.acquisitions[0], &elaboration).unwrap();
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
    let request = implementation::request(&elaboration.acquisitions[0], &elaboration).unwrap();
    request.validate().unwrap();
    assert!(
        request.output_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("result"))
    );
}

#[test]
fn an_undefined_operation_remains_a_clarification_instead_of_invented_code() {
    let (case, mut elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities.clear();
    elaboration.acquisitions[0].operation_spec = None;
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.gaps
            .iter()
            .any(|gap| gap.kind == GapKind::Clarification)
    );
    assert!(plan.gaps.iter().all(|gap| gap.implementation.is_none()));
}

#[test]
fn shared_request_estimates_preserve_scope_and_arguments_but_not_consumer_age() {
    let (_, mut elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[0].costs.execution_requests = CostEstimate::declared(1, "one read");
    let mut second = elaboration.acquisitions[0].clone();
    second.id = "second".into();
    second.contract.time.max_age_seconds = Some(30);
    elaboration.acquisitions.push(second);
    let (_, shared) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(shared.estimated_distinct_acquisitions, Some(1));
    assert_eq!(shared.estimated_distinct_acquisitions_upper_bound, Some(2));
    elaboration.acquisitions[1].arguments["expression"] = json!("5+6");
    let (_, distinct) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(distinct.estimated_distinct_acquisitions, Some(2));
    elaboration.acquisitions[1].arguments["expression"] = json!("3+4");
    elaboration.acquisitions[1]
        .contract
        .scope
        .selectors
        .insert("revision".into(), json!("r2"));
    let (_, scoped) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(scoped.estimated_distinct_acquisitions, Some(2));
}

#[test]
fn bounded_selection_reports_pruning_and_never_overflows_known_costs() {
    let (_, mut elaboration, mut catalogue, config, discovery) = fixture();
    let original = catalogue.capabilities[0].clone();
    catalogue.capabilities = (0..65)
        .map(|index| {
            let mut capability = original.clone();
            capability.id = format!("candidate_{index:02}");
            capability.costs.execution_requests =
                CostEstimate::declared(u64::MAX, "large estimate");
            capability
        })
        .collect();
    let mut second = elaboration.acquisitions[0].clone();
    second.id = "second".into();
    second.arguments["expression"] = json!("5+6");
    elaboration.acquisitions.push(second);
    let (_, selected) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(!selected.search_complete);
    assert_eq!(selected.total_costs[0], (0, 2 * u128::from(u64::MAX)));
    assert_eq!(selected.estimated_distinct_acquisitions, Some(2));
}

#[test]
fn fanout_preserves_unknown_cost_and_explicit_call_bound() {
    let (_, mut elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[0].costs.execution_requests =
        CostEstimate::declared(1, "one detail request");
    catalogue.capabilities[0].costs.transfer_bytes =
        CostEstimate::declared(100, "one detail response");
    elaboration.acquisitions[0].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "identifier_search".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/expression".into(),
        maximum_items: 1000,
        stop_on_witness: None,
    });
    let (_, selection) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(selection.total_costs, vec![(1, 0), (1, 0)]);
    assert_eq!(selection.estimated_distinct_acquisitions, None);
    assert_eq!(
        selection.estimated_distinct_acquisitions_upper_bound,
        Some(1000)
    );
    assert_eq!(selection.dynamic_acquisition_groups, 1);
}

#[test]
fn dependent_request_groups_report_bounds_without_claiming_distinct_resolved_sources() {
    let (_, mut elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[0].costs.execution_requests = CostEstimate::declared(1, "one request");
    elaboration.acquisitions[0]
        .bindings
        .push(DependencyBinding {
            output: OutputReference {
                step_id: "first_selector".into(),
                pointer: "/expression".into(),
            },
            argument_pointer: "/expression".into(),
        });
    let mut second = elaboration.acquisitions[0].clone();
    second.id = "second_detail".into();
    second.bindings[0].output.step_id = "second_selector".into();
    elaboration.acquisitions.push(second);
    let (_, selection) = selection::prepare(&elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(selection.estimated_distinct_acquisitions, None);
    assert_eq!(
        selection.estimated_distinct_acquisitions_upper_bound,
        Some(2)
    );
    assert_eq!(selection.dynamic_acquisition_groups, 2);
    assert_eq!(selection.total_costs[0], (0, 2));
    assert!(selection.method.contains("symbolic_request_sharing"));
}
