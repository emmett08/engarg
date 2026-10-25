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
