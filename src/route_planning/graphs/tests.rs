use super::*;
use crate::support_graph::{QualifiedBatchCorrespondence, validate_execution_graph};
use eal_collector_contracts::{capability::*, execution::*};
use serde_json::json;

fn fixture() -> (
    ArgumentCase,
    SupportElaboration,
    CapabilityCatalogue,
    PlanningConfig,
    DiscoverySnapshot,
) {
    let (case, mut elaboration, mut catalogue, mut config, mut discovery) =
        super::super::tests::fixture();
    config.ranking.dimensions = vec![CostDimension::ExecutionRequests];
    let mut list = elaboration.acquisitions[0].clone();
    list.id = "items".into();
    list.contract.acceptable_methods = vec!["item-list/1".into()];
    list.contract.quantities[0].name = "ids".into();
    list.contract.quantities[0].unit = "item".into();
    list.operation_spec = None;
    let compute = &mut elaboration.acquisitions[0];
    compute.arguments["expression"] = json!(null);
    compute.for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "items".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/expression".into(),
        maximum_items: 4,
        stop_on_witness: None,
    });
    compute.operation_spec = None;
    elaboration.acquisitions.insert(0, list);
    let item = &mut catalogue.capabilities[0];
    item.costs.execution_requests = CostEstimate::declared(1, "one item invocation");
    let mut batch = item.clone();
    batch.id = "batch_calculator".into();
    batch.provider.id = batch.id.clone();
    batch.input_schema["properties"]["expression"] =
        json!({"type":"array","items":{"type":"string"},"maxItems":4});
    batch.output_schema["properties"]["value"] = json!({"type":"array","items":{"type":"integer"}});
    batch.input_schema_sha256 = schema_identity(&batch.input_schema).unwrap();
    batch.output_schema_sha256 = schema_identity(&batch.output_schema).unwrap();
    config
        .batch_correspondences
        .push(QualifiedBatchCorrespondence {
            id: "calculator_batch".into(),
            qualification_reference: "operator://ordered-batch-fixtures/1".into(),
            item_capability_id: item.id.clone(),
            item_measurement_sha256: item.measurement_identity().unwrap(),
            batch_capability_id: batch.id.clone(),
            batch_measurement_sha256: batch.measurement_identity().unwrap(),
            logical_argument_pointer: "/expression".into(),
            item_argument_pointer: "/expression".into(),
            batch_argument_pointer: "/expression".into(),
            maximum_items: 4,
        });
    let mut list_capability = item.clone();
    list_capability.id = "item_list".into();
    list_capability.provider.id = list_capability.id.clone();
    list_capability.semantics.as_mut().unwrap().methods = vec!["item-list/1".into()];
    list_capability.semantics.as_mut().unwrap().quantities = vec![MeasurementQuantity {
        name: "ids".into(),
        unit: "item".into(),
        output_pointer: "/items".into(),
    }];
    list_capability.output_schema = json!({"type":"object","properties":{"items":{"type":"array","items":{"type":"string"},"maxItems":4}},"required":["items"],"additionalProperties":false});
    list_capability.output_schema_sha256 = schema_identity(&list_capability.output_schema).unwrap();
    discovery
        .admitted_capabilities
        .extend([batch.id.clone(), list_capability.id.clone()]);
    catalogue.capabilities.extend([batch, list_capability]);
    (case, elaboration, catalogue, config, discovery)
}

#[test]
fn compares_authored_fanout_with_qualified_batch_and_replays_projection() {
    let (case, elaboration, catalogue, config, discovery) = fixture();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let selection = plan.selection.as_ref().unwrap();
    let graphs = selection.graphs.as_ref().unwrap();
    assert!(graphs.search_complete);
    assert_eq!(graphs.alternatives.len(), 2);
    assert_ne!(graphs.selected, "authored");
    // Independent objective: one list request plus one complete batch request.
    assert_eq!(selection.total_costs, vec![(0, 2)]);
    assert_eq!(
        graphs
            .alternatives
            .iter()
            .find(|graph| graph.id == "authored")
            .unwrap()
            .total_costs,
        vec![(1, 1)]
    );
    let compute = plan
        .execution
        .acquisitions
        .iter()
        .find(|step| step.id == "compute")
        .unwrap();
    assert_eq!(compute.source.capability_id, "batch_calculator");
    assert!(compute.for_each.is_none());
    assert_eq!(compute.bindings.len(), 1);
    assert!(matches!(
        plan.execution
            .derivations
            .iter()
            .find(|step| step.inputs[0].step_id == "compute")
            .unwrap()
            .operation,
        DerivationOperation::ProjectValue { .. }
    ));
    validate_execution_graph(&elaboration, &plan.projections, &plan.execution).unwrap();
    assert_eq!(
        canonical_identity(&plan.execution.evaluations[0].criterion).unwrap(),
        canonical_identity(&elaboration.evaluations[0].criterion).unwrap()
    );
    let mut tampered = plan.execution.clone();
    tampered
        .acquisitions
        .iter_mut()
        .find(|step| step.id == "compute")
        .unwrap()
        .bindings
        .clear();
    assert!(validate_execution_graph(&elaboration, &plan.projections, &tampered).is_err());
}

#[test]
fn changed_qualification_and_scope_are_retained_as_rejected_graphs() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[1]
        .semantics
        .as_mut()
        .unwrap()
        .scope_constraints
        .insert("revision".into(), json!("other"));
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let graphs = plan.selection.unwrap().graphs.unwrap();
    assert_eq!(graphs.selected, "authored");
    assert!(
        graphs
            .alternatives
            .iter()
            .any(|entry| entry.rejection.is_some())
    );
    assert!(
        plan.execution
            .acquisitions
            .iter()
            .find(|step| step.id == "compute")
            .unwrap()
            .for_each
            .is_some()
    );
}

#[test]
fn explicit_readiness_preference_applies_across_topologies_with_fallback() {
    let (case, elaboration, catalogue, mut config, mut discovery) = fixture();
    discovery
        .admitted_capabilities
        .retain(|id| id != "batch_calculator");
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_ne!(plan.selection.unwrap().graphs.unwrap().selected, "authored");
    config.prefer_configured = true;
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(plan.selection.unwrap().graphs.unwrap().selected, "authored");
    discovery.admitted_capabilities.clear();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_ne!(plan.selection.unwrap().graphs.unwrap().selected, "authored");
    assert!(!plan.gaps.is_empty());
}

#[test]
fn unknown_batch_cost_uses_declared_unknown_first_objective() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[1].costs.execution_requests = CostEstimate::unknown("unmeasured");
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    // Both alternatives have one unknown estimate and the same known list cost.
    // The stable lexical graph tie retains the authored topology.
    assert_eq!(plan.selection.unwrap().graphs.unwrap().selected, "authored");
}

#[test]
fn batch_schema_and_budget_must_match_qualified_item_contract() {
    let (case, elaboration, mut catalogue, mut config, discovery) = fixture();
    catalogue.capabilities[1].input_schema["properties"]["expression"]["maxItems"] = json!(5);
    catalogue.capabilities[1].input_schema_sha256 =
        schema_identity(&catalogue.capabilities[1].input_schema).unwrap();
    config.batch_correspondences[0].batch_measurement_sha256 =
        catalogue.capabilities[1].measurement_identity().unwrap();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(plan.selection.unwrap().graphs.unwrap().selected, "authored");
}

#[test]
fn unbounded_population_retains_per_item_execution_and_records_rejection() {
    let (case, elaboration, mut catalogue, config, discovery) = fixture();
    catalogue.capabilities[2].output_schema["properties"]["items"]
        .as_object_mut()
        .unwrap()
        .remove("maxItems");
    catalogue.capabilities[2].output_schema_sha256 =
        schema_identity(&catalogue.capabilities[2].output_schema).unwrap();
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let graphs = plan.selection.unwrap().graphs.unwrap();
    assert_eq!(graphs.selected, "authored");
    assert!(graphs.alternatives.iter().any(|entry| {
        entry
            .rejection
            .as_ref()
            .is_some_and(|reason| reason.contains("cardinality"))
    }));
    assert!(
        plan.execution
            .acquisitions
            .iter()
            .find(|step| step.id == "compute")
            .unwrap()
            .for_each
            .is_some()
    );
}

#[test]
fn rejected_correspondence_does_not_remove_valid_fanout_over_arrays() {
    let (case, elaboration, mut catalogue, mut config, discovery) = fixture();
    catalogue.capabilities[2].output_schema["properties"]["items"]["items"] =
        json!({"type":"array","items":{"type":"string"},"maxItems":4});
    catalogue.capabilities[2].output_schema_sha256 =
        schema_identity(&catalogue.capabilities[2].output_schema).unwrap();
    let with = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    config.batch_correspondences.clear();
    let without = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert_eq!(
        with.routes
            .iter()
            .map(|route| &route.selected)
            .collect::<Vec<_>>(),
        without
            .routes
            .iter()
            .map(|route| &route.selected)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        with.routes
            .iter()
            .find(|route| route.acquisition_id == "compute")
            .unwrap()
            .selected
            .as_deref(),
        Some("batch_calculator")
    );
    assert_eq!(with.selection.unwrap().graphs.unwrap().selected, "authored");
}

#[test]
fn dynamic_comparison_keeps_known_scalar_guard_and_cardinality_gates() {
    let (case, mut elaboration, catalogue, config, discovery) = fixture();
    let mut comparison = elaboration.acquisitions[1].clone();
    comparison.id = "comparison".into();
    comparison.arguments["expression"] = json!("3+4");
    comparison.for_each = None;
    elaboration.acquisitions[1]
        .for_each
        .as_mut()
        .unwrap()
        .stop_on_witness = Some(WitnessPredicate {
        pointer: "/result".into(),
        equals: None,
        equals_output: Some(OutputReference {
            step_id: comparison.id.clone(),
            pointer: "/result".into(),
        }),
    });
    elaboration.acquisitions.push(comparison);
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let guard = plan
        .execution
        .derivations
        .iter()
        .find(|step| step.id.ends_with("_known_identity"))
        .expect("known scalar comparison guard");
    assert!(matches!(
        guard.operation,
        DerivationOperation::AnyIdentityEqual
    ));
    assert_eq!(guard.inputs.len(), 2);
    let singleton = plan
        .execution
        .derivations
        .iter()
        .find(|step| step.id == guard.inputs[1].step_id)
        .unwrap();
    assert!(matches!(singleton.operation, DerivationOperation::Collect));
    assert_eq!(
        canonical_identity(&singleton.inputs[0]).unwrap(),
        canonical_identity(&guard.inputs[0]).unwrap()
    );
    assert!(
        plan.execution.evaluations[0]
            .applicability
            .iter()
            .any(|reference| reference.step_id.ends_with("_cardinality"))
    );
    validate_execution_graph(&elaboration, &plan.projections, &plan.execution).unwrap();
    let mut tampered = plan.execution.clone();
    tampered.evaluations[0].applicability.clear();
    assert!(validate_execution_graph(&elaboration, &plan.projections, &tampered).is_err());
}

#[test]
fn shared_batches_charge_one_request_and_topology_limits_are_truthful() {
    let (case, mut elaboration, catalogue, config, discovery) = fixture();
    let compute = elaboration.acquisitions[1].clone();
    for index in 0..3 {
        let mut copy = compute.clone();
        copy.id = format!("compute_{index}");
        let mut requirement = elaboration.requirements[0].clone();
        requirement.id = format!("requirement_{index}");
        let mut evaluation = elaboration.evaluations[0].clone();
        evaluation.id = format!("evaluation_{index}");
        evaluation.requirement_id = requirement.id.clone();
        evaluation.inputs[0].step_id = copy.id.clone();
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
        elaboration.acquisitions.push(copy);
    }
    let plan = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let selection = plan.selection.unwrap();
    assert_eq!(selection.total_costs, vec![(0, 2)]);
    assert!(selection.graphs.unwrap().search_complete);
    for index in 3..7 {
        let mut copy = compute.clone();
        copy.id = format!("compute_{index}");
        let mut requirement = elaboration.requirements[0].clone();
        requirement.id = format!("requirement_{index}");
        let mut evaluation = elaboration.evaluations[0].clone();
        evaluation.id = format!("evaluation_{index}");
        evaluation.requirement_id = requirement.id.clone();
        evaluation.inputs[0].step_id = copy.id.clone();
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
        elaboration.acquisitions.push(copy);
    }
    let first = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    elaboration.acquisitions.reverse();
    let second = compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    let first = first.selection.unwrap();
    let second = second.selection.unwrap();
    assert!(!first.search_complete);
    assert_eq!(
        first.graphs.as_ref().unwrap().alternatives.len(),
        MAX_GRAPHS
    );
    assert_eq!(
        first.graphs.unwrap().selected,
        second.graphs.unwrap().selected
    );
}
