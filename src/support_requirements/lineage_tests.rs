use super::{
    tests::{case, contract, with_measurement},
    *,
};
use eal_collector_contracts::{
    capability::{LogicalStrength, SupportType},
    execution::{
        DerivationOperation, DerivationStep, EvaluationCriterion, NumericComparison,
        OutputReference, RequiredCoverage,
    },
};
use serde_json::json;

fn mean_fixture() -> (crate::domain::ArgumentCase, SupportElaboration) {
    let case = case();
    let criterion = EvaluationCriterion::Numeric {
        comparison: NumericComparison::AtMost,
        threshold: json!(10),
        value_unit: "ms".into(),
        threshold_unit: "ms".into(),
    };
    let mut elaboration = with_measurement(
        &case,
        contract(
            "observation",
            "statistical_estimate",
            "benchmark.sample/1",
            "ms",
        ),
        criterion,
    );
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .quantities[0]
        .name = "mean_result".into();
    elaboration.derivations.push(DerivationStep {
        id: "mean".into(),
        inputs: vec![OutputReference {
            step_id: "source".into(),
            pointer: "/result".into(),
        }],
        operation: DerivationOperation::Mean,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "mean".into(),
        pointer: "".into(),
    }];
    (case, elaboration)
}

#[test]
fn scaling_cannot_turn_a_derived_mean_into_a_qualified_statistical_measurement() {
    let (case, mut elaboration) = mean_fixture();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("Calculation/ExactCalculation"))
    );
    elaboration.derivations.push(DerivationStep {
        id: "normalised".into(),
        inputs: vec![OutputReference {
            step_id: "mean".into(),
            pointer: "".into(),
        }],
        operation: DerivationOperation::Scale {
            numerator: "1".into(),
            denominator: "1".into(),
            from_unit: "ms".into(),
            to_unit: "ms".into(),
        },
    });
    elaboration.evaluations[0].inputs[0].step_id = "normalised".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("Calculation/ExactCalculation"))
    );
}

#[test]
fn normalising_a_calculated_mean_preserves_its_method_and_exact_unit_conversion() {
    let (case, mut elaboration) = mean_fixture();
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.support_type = SupportType::Calculation;
    required.logical_strength = LogicalStrength::ExactCalculation;
    required.acceptable_methods = vec!["engarg.mean/1".into(), "benchmark.sample/1".into()];
    required
        .scope
        .selectors
        .insert("sample_sources".into(), json!(["source"]));
    required.quantities[0].unit = "s".into();
    elaboration.requirements[0].acceptance = EvaluationCriterion::Numeric {
        comparison: NumericComparison::AtMost,
        threshold: json!(1),
        value_unit: "s".into(),
        threshold_unit: "s".into(),
    };
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    elaboration.derivations.push(DerivationStep {
        id: "seconds".into(),
        inputs: vec![OutputReference {
            step_id: "mean".into(),
            pointer: "".into(),
        }],
        operation: DerivationOperation::Scale {
            numerator: "1".into(),
            denominator: "1000".into(),
            from_unit: "ms".into(),
            to_unit: "s".into(),
        },
    });
    elaboration.evaluations[0].inputs[0].step_id = "seconds".into();
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    let schema =
        logical_output_schema(&elaboration, &elaboration.evaluations[0].inputs[0]).unwrap();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["unit"]["const"], "s");
    assert_eq!(schema["x-engarg-quantity"]["unit"], "s");
}

#[test]
fn collection_preserves_calculation_lineage_from_every_input_not_only_the_first() {
    let (case, mut elaboration) = mean_fixture();
    let mut raw = elaboration.acquisitions[0].clone();
    raw.id = "reported_mean".into();
    raw.contract.quantities[0].name = "mean_result".into();
    elaboration.acquisitions.push(raw);
    elaboration.derivations.push(DerivationStep {
        id: "all_means".into(),
        inputs: vec![
            OutputReference {
                step_id: "reported_mean".into(),
                pointer: "/mean_result".into(),
            },
            OutputReference {
                step_id: "mean".into(),
                pointer: "".into(),
            },
        ],
        operation: DerivationOperation::Collect,
    });
    elaboration.evaluations[0].inputs[0].step_id = "all_means".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("Calculation/ExactCalculation"))
    );
}

#[test]
fn inferred_input_schemas_preserve_unknown_raw_shape_and_known_count_representation() {
    let (_, mut elaboration) = mean_fixture();
    let raw = logical_output_schema(
        &elaboration,
        &OutputReference {
            step_id: "source".into(),
            pointer: "/result".into(),
        },
    )
    .unwrap();
    assert!(raw.get("type").is_none());
    assert_eq!(raw["x-engarg-quantity"]["unit"], "ms");
    elaboration.derivations.push(DerivationStep {
        id: "count".into(),
        inputs: vec![OutputReference {
            step_id: "source".into(),
            pointer: "/result".into(),
        }],
        operation: DerivationOperation::Count,
    });
    let count = logical_output_schema(
        &elaboration,
        &OutputReference {
            step_id: "count".into(),
            pointer: "".into(),
        },
    )
    .unwrap();
    assert_eq!(count["type"], "integer");
    assert_eq!(count["minimum"], 0);
    assert_eq!(count["x-engarg-quantity"]["unit"], "1");
}

#[test]
fn reference_witness_is_a_typed_control_dependency_without_relabelling_selector_scope() {
    use eal_collector_contracts::execution::{ForEachBinding, WitnessPredicate};
    let case = case();
    let measured = contract(
        "observation",
        "existential_witness",
        "fixture.lookup/1",
        "opaque_identity",
    );
    let mut elaboration = with_measurement(
        &case,
        measured.clone(),
        EvaluationCriterion::Equals {
            expected: json!(["uid"]),
        },
    );
    let mut search = elaboration.acquisitions[0].clone();
    search.id = "search".into();
    search.contract.quantities[0].name = "ids".into();
    let mut target = search.clone();
    target.id = "target".into();
    target.contract.scope.kind = "resource".into();
    target.contract.scope.subject = "The independently observed comparison identity".into();
    target.contract.scope.selectors =
        std::collections::BTreeMap::from([("cluster".into(), json!("dev"))]);
    target.contract.quantities[0].name = "wanted_uid".into();
    elaboration.acquisitions[0].arguments = json!({"id":null});
    elaboration.acquisitions[0].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "search".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/id".into(),
        maximum_items: 10,
        stop_on_witness: Some(WitnessPredicate {
            pointer: "/result".into(),
            equals: None,
            equals_output: Some(OutputReference {
                step_id: "target".into(),
                pointer: "/wanted_uid".into(),
            }),
        }),
    });
    elaboration.acquisitions.extend([search, target]);
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    let mut wrong_unit = elaboration.clone();
    wrong_unit.acquisitions[2].contract.quantities[0].unit = "ms".into();
    assert!(
        validate_elaboration(&case, &wrong_unit)
            .iter()
            .any(|error| error.contains("matching units"))
    );
    let mut wrong_clock = elaboration.clone();
    wrong_clock.acquisitions[2]
        .contract
        .time
        .require_original_observation_time = false;
    assert!(
        validate_elaboration(&case, &wrong_clock)
            .iter()
            .any(|error| error.contains("witness control"))
    );
    elaboration.acquisitions[0]
        .for_each
        .as_mut()
        .unwrap()
        .stop_on_witness
        .as_mut()
        .unwrap()
        .equals_output
        .as_mut()
        .unwrap()
        .step_id = "source".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("self") || error.contains("cycle"))
    );
}

fn discovery_fixture() -> (crate::domain::ArgumentCase, SupportElaboration) {
    use eal_collector_contracts::execution::{DependencyBinding, DependencyRole};
    use std::collections::BTreeMap;
    let case = case();
    let mut measured = contract(
        "observation",
        "existential_witness",
        "fixture.object-read/1",
        "uid",
    );
    measured.scope.kind = "named_object".into();
    measured.scope.subject = "The exact named application".into();
    measured.scope.selectors = BTreeMap::from([
        ("context".into(), json!("dev")),
        ("resource".into(), json!("applications.example.invalid")),
        ("namespace".into(), json!("example")),
        ("name".into(), json!("operator")),
    ]);
    measured.scope.identity_dimensions = measured.scope.selectors.keys().cloned().collect();
    let mut elaboration = with_measurement(
        &case,
        measured,
        EvaluationCriterion::Equals {
            expected: json!("uid-1"),
        },
    );
    let mut metadata = elaboration.acquisitions[0].clone();
    metadata.id = "api_discovery".into();
    metadata.contract.support_type = SupportType::SourceRetrieval;
    metadata.contract.logical_strength = LogicalStrength::Clarification;
    metadata.contract.acceptable_methods = vec!["fixture.api-discovery/1".into()];
    metadata.contract.scope.kind = "api_discovery".into();
    metadata.contract.scope.subject =
        "Served API mapping for the requested context and resource".into();
    metadata.contract.scope.selectors = BTreeMap::from([
        ("requested_context".into(), json!("dev")),
        (
            "requested_resource".into(),
            json!("applications.example.invalid"),
        ),
    ]);
    metadata.contract.scope.identity_dimensions =
        metadata.contract.scope.selectors.keys().cloned().collect();
    metadata.contract.quantities[0].name = "api_version".into();
    metadata.contract.quantities[0].unit = "api_version".into();
    metadata.arguments =
        json!({"requested_context":"dev","requested_resource":"applications.example.invalid"});
    elaboration.acquisitions[0].arguments = json!({"api_version":null});
    elaboration.acquisitions[0].bindings = vec![DependencyBinding {
        output: OutputReference {
            step_id: "api_discovery".into(),
            pointer: "/api_version".into(),
        },
        argument_pointer: "/api_version".into(),
        role: DependencyRole::Discovery {
            scope_bindings: BTreeMap::from([
                ("/requested_context".into(), "/context".into()),
                ("/requested_resource".into(), "/resource".into()),
            ]),
        },
    }];
    elaboration.acquisitions.push(metadata);
    (case, elaboration)
}

#[test]
fn discovery_configures_a_named_read_without_claiming_to_select_its_population() {
    let (case, elaboration) = discovery_fixture();
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    let (outputs, errors) = super::lineage::infer_outputs(&elaboration);
    assert!(errors.is_empty(), "{errors:?}");
    let read = &outputs["source"];
    assert!(read.selection.is_empty());
    assert_eq!(read.discovery.len(), 1);
    assert_eq!(
        read.discovery["api_discovery"].time.max_age_seconds,
        Some(60)
    );
    assert!(
        read.discovery["api_discovery"]
            .provenance
            .transformation_history
    );
}

#[test]
fn an_unmarked_dependency_keeps_the_conservative_selection_interpretation() {
    let (case, mut elaboration) = discovery_fixture();
    elaboration.acquisitions[0].bindings[0].role = Default::default();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("selection subject scope"))
    );
    let encoded = serde_json::to_value(&elaboration.acquisitions[0].bindings[0]).unwrap();
    assert!(encoded.get("role").is_none());
}

#[test]
fn discovery_cannot_relabel_a_different_cluster_or_omit_a_scope_selector() {
    let (case, mut elaboration) = discovery_fixture();
    elaboration.acquisitions[1]
        .contract
        .scope
        .selectors
        .insert("requested_context".into(), json!("prod"));
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("discovery scope"))
    );
    elaboration.acquisitions[1]
        .contract
        .scope
        .selectors
        .insert("requested_context".into(), json!("dev"));
    elaboration.acquisitions[1]
        .contract
        .scope
        .selectors
        .insert("tenant".into(), json!("tenant-a"));
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("discovery scope"))
    );
}

#[test]
fn a_search_population_cannot_escape_scope_checks_by_being_labelled_discovery() {
    let (case, mut elaboration) = discovery_fixture();
    elaboration.acquisitions[1].contract.logical_strength = LogicalStrength::ExistentialWitness;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("Discovery")
                || error.contains("discovery requires SourceRetrieval/Clarification"))
    );
}

#[test]
fn discovery_records_keep_observation_clock_and_transformation_requirements() {
    let (case, mut elaboration) = discovery_fixture();
    elaboration.acquisitions[1]
        .contract
        .time
        .require_original_observation_time = false;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("discovery dependency")
                && error.contains("observation clocks"))
    );
    elaboration.acquisitions[1]
        .contract
        .time
        .require_original_observation_time = true;
    elaboration.acquisitions[1]
        .contract
        .provenance
        .transformation_history = false;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("discovery dependency")
                && error.contains("transformation provenance"))
    );
}

#[test]
fn missing_binding_and_fanout_slots_report_the_exact_destination_and_path() {
    use eal_collector_contracts::execution::ForEachBinding;
    let (case, mut elaboration) = discovery_fixture();
    elaboration.acquisitions[0].arguments = json!({});
    elaboration.acquisitions[0].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "api_discovery".into(),
            pointer: "/api_version".into(),
        },
        argument_pointer: "/trace_id".into(),
        maximum_items: 0,
        stop_on_witness: None,
    });
    let errors = validate_elaboration(&case, &elaboration);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("bindings[0].argument_pointer")
                && error.contains("/api_version"))
    );
    assert!(errors.iter().any(|error| error.contains("for_each.argument_pointer") && error.contains("/trace_id")));
    assert!(
        errors
            .iter()
            .any(|error| error.contains("for_each.maximum_items received 0"))
    );
}

#[test]
fn discovery_role_survives_qualified_output_projection_into_an_executable_plan() {
    use crate::route_planning::{self, DiscoverySnapshot, PLANNING_CONFIG_SCHEMA, PlanningConfig};
    use eal_collector_contracts::{capability::*, execution::DependencyRole};
    let (case, elaboration) = discovery_fixture();
    let capabilities = elaboration.acquisitions.iter().map(|blueprint| {
        let properties = blueprint.arguments.as_object().unwrap().keys().map(|name| (name.clone(), json!({"type":"string"}))).collect::<serde_json::Map<_,_>>();
        let input = json!({"type":"object","properties":properties,"required":blueprint.arguments.as_object().unwrap().keys().collect::<Vec<_>>(),"additionalProperties":false});
        let output = json!({"type":"object","properties":{"payload":{"type":"string"}},"required":["payload"],"additionalProperties":false});
        let contract = &blueprint.contract;
        serde_json::from_value::<CapabilityDescriptor>(json!({
            "id":blueprint.id,"version":"1","description":"Synthetic qualified interface for graph projection testing",
            "provider":{"id":blueprint.id,"version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"fixture","configuration_sha256":"a".repeat(64)},
            "input_schema":input,"input_schema_sha256":schema_identity(&input).unwrap(),"output_schema":output,"output_schema_sha256":schema_identity(&output).unwrap(),
            "semantics":{"qualification_reference":"fixture.synthetic/1","support_types":[contract.support_type],"subject_kinds":[contract.scope.kind],"methods":contract.acceptable_methods,"identity_dimensions":contract.scope.identity_dimensions,
                "scope_constraints":contract.scope.selectors,"scope_bindings":{},"quantities":[{"name":contract.quantities[0].name,"unit":contract.quantities[0].unit,"output_pointer":"/payload"}],
                "coverage":[{"strength":contract.logical_strength,"population":null,"completeness_pointer":null}],
                "time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},
                "provenance":{"authorities":contract.provenance.required_authorities,"transformation_history":true},"limitations":[]},
            "configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},"costs":CapabilityCosts::unknown("Synthetic test")
        })).unwrap()
    }).collect();
    let catalogue = CapabilityCatalogue {
        schema: CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities,
    };
    let config: PlanningConfig = serde_json::from_value(json!({
        "schema":PLANNING_CONFIG_SCHEMA,"collector":"/fixture/eal-collector","connections":"/fixture/connections.json","store":"/fixture/observations",
        "ranking":{"dimensions":["execution_requests"]},"scheduling":{"maximum_concurrent_sources":4,"per_provider":{}},"bindings":[]
    })).unwrap();
    let discovery = DiscoverySnapshot {
        admitted_capabilities: vec!["source".into(), "api_discovery".into()],
        ..Default::default()
    };
    let plan =
        route_planning::compile(&case, &elaboration, &catalogue, &config, &discovery).unwrap();
    assert!(
        plan.routes
            .iter()
            .all(|route| route.selected.is_some() && route.configuration_gaps.is_empty()),
        "{:?}",
        plan.routes
    );
    plan.execution.validate().unwrap();
    crate::support_graph::validate_execution_graph(
        &elaboration,
        &plan.projections,
        &plan.execution,
    )
    .unwrap();
    let dependency = &plan
        .execution
        .acquisitions
        .iter()
        .find(|step| step.id == "source")
        .unwrap()
        .bindings[0];
    assert!(matches!(dependency.role, DependencyRole::Discovery { .. }));
    assert_ne!(dependency.output.step_id, "api_discovery");
    assert!(
        plan.execution
            .derivations
            .iter()
            .any(|step| step.id == dependency.output.step_id)
    );
}

#[test]
fn projecting_an_aggregate_cannot_reclassify_it_as_scalar_discovery_metadata() {
    let (case, mut elaboration) = discovery_fixture();
    elaboration.derivations.extend([
        DerivationStep {
            id: "collected_metadata".into(),
            inputs: vec![OutputReference {
                step_id: "api_discovery".into(),
                pointer: "/api_version".into(),
            }],
            operation: DerivationOperation::Collect,
        },
        DerivationStep {
            id: "wrapped_metadata".into(),
            inputs: vec![OutputReference {
                step_id: "collected_metadata".into(),
                pointer: "".into(),
            }],
            operation: DerivationOperation::ProjectValue {
                fields: std::collections::BTreeMap::from([("api_version".into(), "".into())]),
            },
        },
    ]);
    elaboration.acquisitions[0].bindings[0].output.step_id = "wrapped_metadata".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("discovery requires SourceRetrieval/Clarification"))
    );
}
