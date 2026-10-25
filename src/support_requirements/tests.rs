use super::*;
use crate::domain::ArgumentCase;
use eal_collector_contracts::{
    capability::{SupportContract, canonical_identity},
    execution::{
        DerivationOperation, DerivationStep, EvaluationCriterion, OutputReference,
        RequiredCoverage, RequirementEvaluation,
    },
};
use serde_json::json;

pub(crate) fn case() -> ArgumentCase {
    serde_json::from_str(include_str!("../../examples/claim-only-case.json")).unwrap()
}

/// Unknown acquisition details remain explicit gaps rather than invented facts.
pub(crate) fn pending(case: &ArgumentCase) -> SupportElaboration {
    let mut requirements = material_targets(case)
        .into_iter()
        .enumerate()
        .map(|(index, target_id)| {
            let mut needs = case
                .evidence_plan
                .iter()
                .filter(|need| need.target_id == target_id)
                .collect::<Vec<_>>();
            needs.sort_by_key(|need| &need.id);
            SupportRequirement {
                id: format!("requirement-{index}"),
                target_id,
                evidence_need_ids: needs.iter().map(|need| need.id.clone()).collect(),
                original_acceptance_criteria: needs
                    .iter()
                    .map(|need| need.acceptance_criterion.clone())
                    .collect(),
                contract: None,
                discriminating_evidence: vec![
                    "Retain adverse findings as well as supporting findings.".into(),
                ],
                acceptance: EvaluationCriterion::Unavailable {
                    reason: "The source and measurement method require clarification.".into(),
                },
                acceptance_rationale:
                    "No input establishes a qualified measurement source or completed judgement."
                        .into(),
                gaps: vec![RequirementGap {
                    kind: RequirementGapKind::Clarification,
                    description: "Declare the measurement method and source identity.".into(),
                }],
            }
        })
        .collect::<Vec<_>>();
    let alternative_support = alternative_obligations(case)
        .into_iter()
        .map(|alternative| {
            let requirement = requirements.last_mut().expect("case has material support");
            requirement
                .discriminating_evidence
                .push(alternative.distinguishing_test.clone());
            AlternativeSupport {
                alternative_id: alternative.alternative_id,
                distinguishing_test: alternative.distinguishing_test,
                claim_ids: alternative.claim_ids,
                requirement_ids: vec![requirement.id.clone()],
            }
        })
        .collect();
    let evaluations = requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| RequirementEvaluation {
            id: format!("evaluation-{index}"),
            requirement_id: requirement.id.clone(),
            inputs: vec![],
            criterion: requirement.acceptance.clone(),
            max_age_seconds: None,
            required_coverage: RequiredCoverage::Unspecified,
            applicability: vec![],
            challenges: vec![],
        })
        .collect();
    SupportElaboration {
        schema: SUPPORT_ELABORATION_SCHEMA.into(),
        requirements,
        acquisitions: vec![],
        derivations: vec![],
        evaluations,
        revisions: vec![],

        alternative_support,
    }
}

pub(crate) fn contract(
    support_type: &str,
    strength: &str,
    method: &str,
    unit: &str,
) -> SupportContract {
    serde_json::from_value(json!({
        "schema":"eal.support-contract/1","support_type":support_type,
        "scope":{"kind":"frozen_case","subject":"The explicitly identified candidate","selectors":{"revision":"frozen-revision"},"identity_dimensions":["revision"]},
        "acceptable_methods":[method],"quantities":[{"name":"result","unit":unit}],
        "logical_strength":strength,"population":if strength=="complete_enumeration" {json!("All declared API endpoints")} else {json!(null)},
        "time":{"historical_interval":null,"max_age_seconds":60,"max_uncertainty_seconds":1,"require_original_observation_time":true},
        "provenance":{"required_authorities":["identified-source"],"transformation_history":true},
        "expected_output_schema_sha256":null,
    })).unwrap()
}

pub(crate) fn with_measurement(
    case: &ArgumentCase,
    contract: SupportContract,
    criterion: EvaluationCriterion,
) -> SupportElaboration {
    let mut elaboration = pending(case);
    elaboration.requirements[0].contract = Some(contract.clone());
    elaboration.requirements[0].acceptance = criterion.clone();
    elaboration.requirements[0].gaps.clear();
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "source".into(),
        contract,
        arguments: json!({"revision":"frozen-revision"}),
        bindings: vec![],
        for_each: None,

        operation_spec: None,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "source".into(),
        pointer: "/result".into(),
    }];
    elaboration.evaluations[0].criterion = criterion;
    elaboration.evaluations[0].max_age_seconds = Some(60);
    elaboration.evaluations[0].required_coverage = match elaboration.requirements[0]
        .contract
        .as_ref()
        .unwrap()
        .logical_strength
    {
        eal_collector_contracts::capability::LogicalStrength::ExistentialWitness => {
            RequiredCoverage::Witness
        }
        eal_collector_contracts::capability::LogicalStrength::CompleteEnumeration => {
            RequiredCoverage::CompleteEnumeration
        }
        eal_collector_contracts::capability::LogicalStrength::StatisticalEstimate => {
            RequiredCoverage::Statistical
        }
        _ => RequiredCoverage::Unspecified,
    };
    elaboration
}

#[test]
fn explicit_obligations_cover_all_material_targets_without_inventing_support() {
    let case = case();
    let elaboration = pending(&case);
    assert!(validate_elaboration(&case, &elaboration).is_empty());
    assert!(elaboration.requirements.len() >= case.evidence_plan.len());
    assert!(
        elaboration
            .requirements
            .iter()
            .all(|requirement| requirement.contract.is_none())
    );
}

#[test]
fn contracts_preserve_performance_reliability_api_numerical_and_design_meaning() {
    let case = case();
    for (support, strength, method, unit, criterion) in [
        (
            "experiment",
            "statistical_estimate",
            "benchmark.controlled-load/1",
            "s",
            json!({"kind":"numeric","comparison":"at_most","threshold":2.5,"value_unit":"s","threshold_unit":"s"}),
        ),
        (
            "experiment",
            "statistical_estimate",
            "reliability.binomial-protocol/1",
            "1",
            json!({"kind":"numeric","comparison":"at_least","threshold":0.999,"value_unit":"1","threshold_unit":"1"}),
        ),
        (
            "observation",
            "complete_enumeration",
            "api.exhaustive-conformance/1",
            "boolean",
            json!({"kind":"all_true"}),
        ),
        (
            "calculation",
            "exact_calculation",
            "arithmetic.rational-check/1",
            "1",
            json!({"kind":"equals","expected":"1/3"}),
        ),
        (
            "judgement",
            "judgement",
            "design.independent-review/1",
            "judgement",
            json!({"kind":"judgement","criterion":"An identified reviewer must assess the stated design alternatives against the declared tradeoffs."}),
        ),
    ] {
        let elaboration = with_measurement(
            &case,
            contract(support, strength, method, unit),
            serde_json::from_value(criterion).unwrap(),
        );
        assert!(
            validate_elaboration(&case, &elaboration).is_empty(),
            "{method}: {:?}",
            validate_elaboration(&case, &elaboration)
        );
    }
}

#[test]
fn coverage_original_clauses_units_freshness_and_evaluation_thresholds_are_enforced() {
    let case = case();
    let mut missing = pending(&case);
    let removed = missing.requirements.remove(0);
    missing
        .evaluations
        .retain(|evaluation| evaluation.requirement_id != removed.id);
    assert!(
        validate_elaboration(&case, &missing)
            .iter()
            .any(|error| error.contains("requires a support requirement"))
    );
    let mut changed = pending(&case);
    changed.requirements[0].original_acceptance_criteria[0] = "An easier threshold".into();
    assert!(
        validate_elaboration(&case, &changed)
            .iter()
            .any(|error| error.contains("exact original"))
    );
    let criterion = EvaluationCriterion::Numeric {
        comparison: eal_collector_contracts::execution::NumericComparison::AtMost,
        threshold: json!(1),
        value_unit: "s".into(),
        threshold_unit: "ms".into(),
    };
    let mut mismatch = with_measurement(
        &case,
        contract(
            "experiment",
            "statistical_estimate",
            "benchmark.controlled-load/1",
            "s",
        ),
        criterion,
    );
    mismatch.evaluations[0].max_age_seconds = Some(61);
    mismatch.evaluations[0].criterion = EvaluationCriterion::Equals {
        expected: json!(true),
    };
    let errors = validate_elaboration(&case, &mismatch).join(";");
    for expected in [
        "matching explicit units",
        "changed its requirement acceptance",
        "changed its requirement freshness",
    ] {
        assert!(errors.contains(expected), "{errors}");
    }
}

#[test]
fn unknown_contracts_cannot_smuggle_executable_facts_and_graph_cycles_are_rejected() {
    let case = case();
    let mut elaboration = pending(&case);
    elaboration.requirements[0].acceptance = EvaluationCriterion::AllTrue;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("unknown contract"))
    );
    elaboration = pending(&case);
    elaboration.derivations = vec![
        DerivationStep {
            id: "left".into(),
            inputs: vec![OutputReference {
                step_id: "right".into(),
                pointer: "".into(),
            }],
            operation: DerivationOperation::Collect,
        },
        DerivationStep {
            id: "right".into(),
            inputs: vec![OutputReference {
                step_id: "left".into(),
                pointer: "".into(),
            }],
            operation: DerivationOperation::Collect,
        },
    ];
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("cycle"))
    );
}

#[test]
fn semantic_revisions_require_prior_identity_and_explicit_reasons() {
    let case = case();
    let previous = pending(&case);
    let mut current = previous.clone();
    current.requirements[0].acceptance_rationale =
        "The original study leaves this assessment pending until review.".into();
    assert!(
        validate_revision(&case, &previous, &case, &current)
            .iter()
            .any(|error| error.contains("exact prior-digest"))
    );
    current.revisions.push(RequirementRevision {requirement_id:previous.requirements[0].id.clone(),previous_requirement_sha256:canonical_identity(&previous.requirements[0]).unwrap(),reason:"The original study explicitly reserves the acceptance judgement for independent review.".into()});
    assert!(validate_revision(&case, &previous, &case, &current).is_empty());
}

#[test]
fn encoded_envelopes_reject_duplicate_keys_unknown_fields_and_oversized_content() {
    let case = case();
    let elaboration = pending(&case);
    let envelope=json!({"argument":case,"support_elaboration_json":serde_json::to_string(&elaboration).unwrap()}).to_string();
    assert!(parse_supported_argument(&envelope, 1_048_576).is_ok());
    assert!(parse_supported_argument(&envelope, 32).is_err());
    let mut outer: serde_json::Value = serde_json::from_str(&envelope).unwrap();
    outer["support_elaboration_json"] = json!("{\"schema\":\"one\",\"schema\":\"two\"}");
    assert!(parse_supported_argument(&outer.to_string(), 1_048_576).is_err());
    outer["unexpected"] = json!(true);
    assert!(parse_supported_argument(&outer.to_string(), 1_048_576).is_err());
    assert!(envelope_schema()["properties"]["support_elaboration_json"]["type"] == "string");
}

#[test]
fn provider_envelope_preserves_exact_numeric_threshold_scope_and_logical_arguments() {
    let case = case();
    let exact = "0.123456789012345678901234567890123456789";
    let value: serde_json::Value = serde_json::from_str(exact).unwrap();
    let mut measurement = contract(
        "calculation",
        "exact_calculation",
        "arithmetic.rational-check/1",
        "1",
    );
    measurement
        .scope
        .selectors
        .insert("exact_parameter".into(), value.clone());
    let criterion = EvaluationCriterion::Numeric {
        comparison: eal_collector_contracts::execution::NumericComparison::Equal,
        threshold: value.clone(),
        value_unit: "1".into(),
        threshold_unit: "1".into(),
    };
    let mut elaboration = with_measurement(&case, measurement, criterion);
    elaboration.acquisitions[0].arguments["exact_parameter"] = value;
    let encoded=json!({"argument":case,"support_elaboration_json":serde_json::to_string(&elaboration).unwrap()}).to_string();
    let parsed = parse_supported_argument(&encoded, 1_048_576).unwrap();
    assert_eq!(
        parsed.support_elaboration.acquisitions[0].arguments["exact_parameter"].to_string(),
        exact
    );
    assert_eq!(
        parsed.support_elaboration.requirements[0]
            .contract
            .as_ref()
            .unwrap()
            .scope
            .selectors["exact_parameter"]
            .to_string(),
        exact
    );
    let EvaluationCriterion::Numeric { threshold, .. } =
        &parsed.support_elaboration.requirements[0].acceptance
    else {
        panic!("numeric criterion")
    };
    assert!(threshold.is_number());
    assert_eq!(threshold.to_string(), exact);
    assert!(validate_elaboration(&parsed.argument, &parsed.support_elaboration).is_empty());
}

#[test]
fn exact_rational_thresholds_are_admitted_without_rounding_or_zero_denominators() {
    let case = case();
    let criterion = EvaluationCriterion::Numeric {
        comparison: eal_collector_contracts::execution::NumericComparison::Equal,
        threshold: json!({"schema":"eal.exact-number/1","numerator":"1","denominator":"3","unit":"1"}),
        value_unit: "1".into(),
        threshold_unit: "1".into(),
    };
    let mut elaboration = with_measurement(
        &case,
        contract(
            "calculation",
            "exact_calculation",
            "arithmetic.rational-check/1",
            "1",
        ),
        criterion,
    );
    assert!(validate_elaboration(&case, &elaboration).is_empty());
    if let EvaluationCriterion::Numeric { threshold, .. } =
        &mut elaboration.requirements[0].acceptance
    {
        threshold["denominator"] = json!("0");
    }
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("numeric threshold"))
    );
}

#[test]
fn semantic_flow_rejects_scope_method_coverage_provenance_and_unit_laundering() {
    let case = case();
    let criterion:EvaluationCriterion=serde_json::from_value(json!({"kind":"numeric","comparison":"at_least","threshold":1,"value_unit":"s","threshold_unit":"s"})).unwrap();
    let base = with_measurement(
        &case,
        contract(
            "observation",
            "complete_enumeration",
            "api.exhaustive-conformance/1",
            "s",
        ),
        criterion,
    );
    for (field, expected) in [
        ("scope", "subject scope"),
        ("method", "measurement method"),
        ("coverage", "logical strength"),
        ("provenance", "provenance"),
        ("unit", "unit s"),
    ] {
        let mut changed = base.clone();
        let offered = &mut changed.acquisitions[0].contract;
        match field {
            "scope" => {
                offered
                    .scope
                    .selectors
                    .insert("revision".into(), json!("different-revision"));
            }
            "method" => offered.acceptable_methods = vec!["passive-log.sample/1".into()],
            "coverage" => {
                offered.logical_strength =
                    eal_collector_contracts::capability::LogicalStrength::ExistentialWitness
            }
            "provenance" => offered.provenance.required_authorities.clear(),
            "unit" => offered.quantities[0].unit = "ms".into(),
            _ => unreachable!(),
        }
        let errors = validate_elaboration(&case, &changed).join(";");
        assert!(errors.contains(expected), "{field}: {errors}");
    }
}

#[test]
fn explicit_scaling_preserves_quantity_scope_methods_coverage_and_exact_unit_factors() {
    let case = case();
    let criterion:EvaluationCriterion=serde_json::from_value(json!({"kind":"numeric","comparison":"at_most","threshold":1,"value_unit":"s","threshold_unit":"s"})).unwrap();
    let mut elaboration = with_measurement(
        &case,
        contract(
            "experiment",
            "statistical_estimate",
            "benchmark.controlled-load/1",
            "s",
        ),
        criterion,
    );
    elaboration.acquisitions[0].contract.quantities[0].unit = "ms".into();
    elaboration.derivations.push(DerivationStep {
        id: "seconds".into(),
        inputs: vec![OutputReference {
            step_id: "source".into(),
            pointer: "/result".into(),
        }],
        operation: DerivationOperation::Scale {
            numerator: "1".into(),
            denominator: "1000".into(),
            from_unit: "ms".into(),
            to_unit: "s".into(),
        },
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "seconds".into(),
        pointer: "".into(),
    }];
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    if let DerivationOperation::Scale { numerator, .. } = &mut elaboration.derivations[0].operation
    {
        *numerator = "1000".into();
    }
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("exact supported conversion factor"))
    );
}

#[test]
fn closed_aggregate_identities_cannot_mislabel_a_mean_as_a_percentile() {
    let case = case();
    let criterion:EvaluationCriterion=serde_json::from_value(json!({"kind":"numeric","comparison":"at_most","threshold":1,"value_unit":"s","threshold_unit":"s"})).unwrap();
    let mut elaboration = with_measurement(
        &case,
        contract(
            "experiment",
            "statistical_estimate",
            "benchmark.controlled-load/1",
            "s",
        ),
        criterion,
    );
    elaboration.derivations.push(DerivationStep {
        id: "average".into(),
        inputs: vec![OutputReference {
            step_id: "source".into(),
            pointer: "/result".into(),
        }],
        operation: DerivationOperation::Mean,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "average".into(),
        pointer: "".into(),
    }];
    let result_contract = elaboration.requirements[0].contract.as_mut().unwrap();
    result_contract.support_type = eal_collector_contracts::capability::SupportType::Calculation;
    result_contract.logical_strength =
        eal_collector_contracts::capability::LogicalStrength::ExactCalculation;
    result_contract.acceptable_methods =
        vec!["engarg.mean/1".into(), "benchmark.controlled-load/1".into()];
    result_contract
        .scope
        .selectors
        .insert("sample_sources".into(), json!(["source"]));
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .quantities[0]
        .name = "latency_p99".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("explicit closed derivation quantity name"))
    );
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .quantities[0]
        .name = "mean_result".into();
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    elaboration.derivations.push(DerivationStep {
        id: "rename".into(),
        inputs: vec![OutputReference {
            step_id: "source".into(),
            pointer: "".into(),
        }],
        operation: DerivationOperation::Project {
            fields: std::collections::BTreeMap::from([("latency_p99".into(), "/result".into())]),
        },
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "rename".into(),
        pointer: "/latency_p99".into(),
    }];
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .quantities[0]
        .name = "latency_p99".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("does not provide logical quantity latency_p99"))
    );
}

#[test]
fn source_acceptance_clauses_cannot_be_weakened_even_with_a_revision_reason() {
    let previous_case = case();
    let previous = pending(&previous_case);
    let mut current_case = previous_case.clone();
    current_case.evidence_plan[0].acceptance_criterion =
        "Any conveniently available record suffices.".into();
    let current = pending(&current_case);
    assert!(
        validate_revision(&previous_case, &previous, &current_case, &current)
            .iter()
            .any(|error| error.contains("original acceptance clause"))
    );
}

#[test]
fn removed_target_requirements_need_prior_digest_disposition_and_retained_targets_cannot_drop() {
    let previous_case = case();
    let previous = pending(&previous_case);
    let removed = previous
        .requirements
        .iter()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap();
    let mut current = previous.clone();
    current
        .requirements
        .retain(|requirement| requirement.id != removed.id);
    current
        .evaluations
        .retain(|evaluation| evaluation.requirement_id != removed.id);
    let mut current_case = previous_case.clone();
    current_case.grounds.retain(|ground| ground.id != "G1");
    current_case
        .evidence_plan
        .retain(|need| need.target_id != "G1");
    assert!(
        validate_revision(&previous_case, &previous, &current_case, &current)
            .iter()
            .any(|error| error.contains("without its prior-digest disposition"))
    );
    current.revisions.push(RequirementRevision{requirement_id:removed.id.clone(),previous_requirement_sha256:canonical_identity(removed).unwrap(),reason:"The revised case removes the original target; its source obligation remains explicitly retired rather than silently dropped.".into()});
    assert!(validate_revision(&previous_case, &previous, &current_case, &current).is_empty());
    assert!(
        validate_revision(&previous_case, &previous, &previous_case, &current)
            .iter()
            .any(|error| error.contains("retained target"))
    );
    current.revisions[0].previous_requirement_sha256 = "a".repeat(64);
    assert!(
        validate_revision(&previous_case, &previous, &current_case, &current)
            .iter()
            .any(|error| error.contains("exact previous requirement identity"))
    );
}

#[test]
fn measured_observations_transfer_to_explicit_sum_and_count_without_becoming_measurement_methods() {
    use eal_collector_contracts::capability::{LogicalStrength, SupportType};
    use eal_collector_contracts::execution::ForEachBinding;
    let case = case();
    let mut measured = contract(
        "observation",
        "complete_enumeration",
        "fixture.complete-read/1",
        "ms",
    );
    measured.population = Some("All readings in the frozen case".into());
    measured.quantities[0].name = "latency".into();
    let criterion = EvaluationCriterion::Numeric {
        comparison: eal_collector_contracts::execution::NumericComparison::AtMost,
        threshold: json!(100),
        value_unit: "ms".into(),
        threshold_unit: "ms".into(),
    };
    let mut elaboration = with_measurement(&case, measured.clone(), criterion);
    elaboration.acquisitions[0].id = "details".into();
    elaboration.acquisitions[0].arguments = json!({"reading_id":null});
    elaboration.acquisitions[0].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "search".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/reading_id".into(),
        maximum_items: 10,
        stop_on_witness: None,
    });
    let mut search = measured.clone();
    search.scope.kind = "reading_search".into();
    search.scope.subject = "The reading identities selected before detailed reads".into();
    search.acceptable_methods = vec!["fixture.complete-search/1".into()];
    search.quantities[0].name = "ids".into();
    search.quantities[0].unit = "reading_id".into();
    elaboration.acquisitions.insert(
        0,
        AcquisitionBlueprint {
            id: "search".into(),
            contract: search,
            arguments: json!({"revision":"frozen-revision"}),
            bindings: vec![],
            for_each: None,

            operation_spec: None,
        },
    );
    elaboration.derivations.push(DerivationStep {
        id: "total".into(),
        inputs: vec![OutputReference {
            step_id: "details".into(),
            pointer: "/latency".into(),
        }],
        operation: DerivationOperation::Sum,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "total".into(),
        pointer: "".into(),
    }];
    elaboration.evaluations[0].required_coverage = RequiredCoverage::CompleteEnumeration;
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.support_type = SupportType::Calculation;
    required.logical_strength = LogicalStrength::ExactCalculation;
    required.scope.kind = "numerical_expression".into();
    required.scope.subject = "Sum of the enumerated latency values".into();
    required.acceptable_methods = vec!["engarg.sum/1".into(), "fixture.complete-read/1".into()];
    required.quantities[0].name = "sum_latency".into();
    let mut unchecked = elaboration.clone();
    unchecked.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    assert!(
        validate_elaboration(&case, &unchecked)
            .iter()
            .any(|error| error.contains("logical coverage")),
        "population totals must demand actual complete runtime coverage"
    );
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    assert_eq!(
        elaboration.acquisitions[1].contract.support_type,
        SupportType::Observation
    );
    elaboration.derivations[0].operation = DerivationOperation::Count;
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.acceptable_methods = vec!["engarg.count/1".into()];
    required.quantities[0].name = "count_latency".into();
    required.quantities[0].unit = "1".into();
    elaboration.requirements[0].acceptance = EvaluationCriterion::Numeric {
        comparison: eal_collector_contracts::execution::NumericComparison::AtLeast,
        threshold: json!(2),
        value_unit: "1".into(),
        threshold_unit: "1".into(),
    };
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    elaboration.acquisitions[0].contract.logical_strength = LogicalStrength::ExistentialWitness;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("selection population or logical strength"))
    );
    elaboration.acquisitions[1].contract.logical_strength = LogicalStrength::ExistentialWitness;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("population calculation requires complete enumeration"))
    );
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.population = None;
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    required
        .scope
        .selectors
        .insert("sample_sources".into(), json!(["details"]));
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
}

fn identity_fixture() -> (ArgumentCase, SupportElaboration) {
    use eal_collector_contracts::capability::{LogicalStrength, SupportType};
    let case = case();
    let mut resource = contract(
        "observation",
        "existential_witness",
        "kubernetes.resource-identity/1",
        "opaque_identity",
    );
    resource.scope.kind = "k8s_resource".into();
    resource.scope.subject = "The Kubernetes resource".into();
    resource.scope.selectors = std::collections::BTreeMap::from([
        ("cluster".into(), json!("dev")),
        ("resource".into(), json!("catalog-sync")),
    ]);
    resource.scope.identity_dimensions = vec!["cluster".into(), "resource".into()];
    resource.quantities[0].name = "resource_uid".into();
    resource.provenance.required_authorities = vec!["kubernetes".into()];
    let mut trace = resource.clone();
    trace.scope.kind = "trace".into();
    trace.scope.subject = "The retained trace".into();
    trace
        .scope
        .selectors
        .insert("trace_id".into(), json!("trace-1"));
    trace.acceptable_methods = vec!["tempo.trace-resource-identity/1".into()];
    trace.provenance.required_authorities = vec!["tempo".into()];
    let mut elaboration = with_measurement(
        &case,
        resource.clone(),
        EvaluationCriterion::Equals {
            expected: json!(true),
        },
    );
    elaboration.acquisitions[0].id = "resource".into();
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "trace".into(),
        contract: trace.clone(),
        arguments: json!({"trace_id":"trace-1"}),
        bindings: vec![],
        for_each: None,

        operation_spec: None,
    });
    elaboration.derivations.push(DerivationStep {
        id: "same_uid".into(),
        inputs: vec![
            OutputReference {
                step_id: "resource".into(),
                pointer: "/resource_uid".into(),
            },
            OutputReference {
                step_id: "trace".into(),
                pointer: "/resource_uid".into(),
            },
        ],
        operation: DerivationOperation::IdentityEqual,
    });
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "same_uid".into(),
        pointer: "".into(),
    }];
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.support_type = SupportType::Calculation;
    required.logical_strength = LogicalStrength::ExactCalculation;
    required.scope.kind = "identity_relation".into();
    required.scope.subject = "Equality of the resource and trace UID values".into();
    required.scope.selectors = std::collections::BTreeMap::from([
        ("cluster".into(), json!("dev")),
        (
            "operands".into(),
            json!({"resource":resource.scope,"trace":trace.scope}),
        ),
        (
            "operand_times".into(),
            json!({"resource":resource.time,"trace":trace.time}),
        ),
    ]);
    required.scope.identity_dimensions = vec!["cluster".into(), "operands".into()];
    required.acceptable_methods = vec![
        "engarg.identity-equal/1".into(),
        "kubernetes.resource-identity/1".into(),
        "tempo.trace-resource-identity/1".into(),
    ];
    required.quantities[0].name = "identity_equal".into();
    required.quantities[0].unit = "boolean".into();
    required.provenance.required_authorities = vec!["kubernetes".into(), "tempo".into()];
    (case, elaboration)
}

#[test]
fn cross_kind_identity_relation_explicitly_names_each_original_subject_scope() {
    let (case, mut elaboration) = identity_fixture();
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .scope
        .selectors
        .get_mut("operands")
        .unwrap()["trace"]["selectors"]["cluster"] = json!("prod");
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("exact original SubjectScope"))
    );
}

#[test]
fn exhaustive_production_selector_cannot_be_relabelled_as_development_detail_coverage() {
    use eal_collector_contracts::execution::ForEachBinding;
    let case = case();
    let mut details = contract(
        "observation",
        "complete_enumeration",
        "fixture.details/1",
        "boolean",
    );
    details.population = Some("All development records".into());
    details
        .scope
        .selectors
        .insert("cluster".into(), json!("dev"));
    details.quantities[0].name = "match".into();
    let mut elaboration = with_measurement(&case, details.clone(), EvaluationCriterion::AllTrue);
    elaboration.acquisitions[0].id = "details".into();
    elaboration.acquisitions[0].arguments = json!({"id":null});
    elaboration.acquisitions[0].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "production_search".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/id".into(),
        maximum_items: 100,
        stop_on_witness: None,
    });
    let mut search = details;
    search
        .scope
        .selectors
        .insert("cluster".into(), json!("prod"));
    search.quantities[0].name = "ids".into();
    search.quantities[0].unit = "record_id".into();
    elaboration.acquisitions.insert(
        0,
        AcquisitionBlueprint {
            id: "production_search".into(),
            contract: search,
            arguments: json!({"cluster":"prod"}),
            bindings: vec![],
            for_each: None,

            operation_spec: None,
        },
    );
    elaboration.evaluations[0].inputs = vec![OutputReference {
        step_id: "details".into(),
        pointer: "/match".into(),
    }];
    let errors = validate_elaboration(&case, &elaboration).join(";");
    assert!(
        errors.contains("selector ancestor production_search") && errors.contains("scope"),
        "{errors}"
    );
}

#[test]
fn retained_judgement_requires_the_exact_typed_judgement_scope() {
    let case = case();
    let measured = contract(
        "judgement",
        "judgement",
        "design.independent-review/1",
        "judgement_record",
    );
    let criterion = EvaluationCriterion::Judgement {
        criterion: "Assess the declared alternatives against the frozen design criteria.".into(),
        scope: Some(measured.scope.clone()),
    };
    let mut elaboration = with_measurement(&case, measured, criterion);
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    if let EvaluationCriterion::Judgement {
        scope: Some(scope), ..
    } = &mut elaboration.requirements[0].acceptance
    {
        scope
            .selectors
            .insert("revision".into(), json!("another-design"));
    }
    elaboration.evaluations[0].criterion = elaboration.requirements[0].acceptance.clone();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("exact subject scope"))
    );
}

#[test]
fn resource_identity_matches_foreach_trace_collection_with_explicit_time_and_scope_operands() {
    use eal_collector_contracts::execution::ForEachBinding;
    let (case, mut elaboration) = identity_fixture();
    let mut search = elaboration.acquisitions[1].contract.clone();
    search.scope.selectors.remove("trace_id");
    search.quantities[0].name = "trace_ids".into();
    search.quantities[0].unit = "trace_id".into();
    search.acceptable_methods = vec!["tempo.search/1".into()];
    search.time.historical_interval =
        Some(eal_collector_contracts::capability::HistoricalInterval {
            start_unix: 1_700_000_000,
            end_unix: 1_700_000_100,
        });
    search.time.max_age_seconds = None;
    elaboration.acquisitions[1]
        .contract
        .scope
        .selectors
        .remove("trace_id");
    elaboration.acquisitions[1].contract.time = search.time.clone();
    // An immutable-ID lookup retains the selector's historical clock without
    // pretending that the lookup itself performs an interval query.
    elaboration.acquisitions[1]
        .contract
        .time
        .historical_interval = None;
    elaboration.acquisitions[1].arguments = json!({"trace_id":null});
    elaboration.acquisitions[1].for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "trace_search".into(),
            pointer: "/trace_ids".into(),
        },
        argument_pointer: "/trace_id".into(),
        maximum_items: 100,
        stop_on_witness: None,
    });
    let search_time = search.time.clone();
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "trace_search".into(),
        contract: search,
        arguments: json!({"cluster":"dev"}),
        bindings: vec![],
        for_each: None,

        operation_spec: None,
    });
    let trace_scope = elaboration.acquisitions[1].contract.scope.clone();
    let trace_time = elaboration.acquisitions[1].contract.time.clone();
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.scope.selectors.get_mut("operands").unwrap()["trace"] = json!(trace_scope);
    required.scope.selectors.get_mut("operand_times").unwrap()["trace"] = json!(trace_time);
    required.scope.selectors.get_mut("operand_times").unwrap()["trace_search"] = json!(search_time);
    required.time.max_age_seconds = Some(86_400_000);
    elaboration.evaluations[0].max_age_seconds = Some(86_400_000);
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("same scalar/collection shape"))
    );
    elaboration.derivations[0].operation = DerivationOperation::AnyIdentityEqual;
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.acceptable_methods[0] = "engarg.any-identity-equal/1".into();
    required.quantities[0].name = "any_identity_equal".into();
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Witness;
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    let mut changed_window = elaboration.clone();
    changed_window.acquisitions[1]
        .contract
        .time
        .historical_interval = Some(eal_collector_contracts::capability::HistoricalInterval {
        start_unix: 1_700_001_000,
        end_unix: 1_700_001_100,
    });
    assert!(
        validate_elaboration(&case, &changed_window)
            .iter()
            .any(|error| error.contains("selection historical interval"))
    );
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Unspecified;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("logical coverage"))
    );
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Witness;
    elaboration.requirements[0]
        .contract
        .as_mut()
        .unwrap()
        .scope
        .selectors
        .get_mut("operand_times")
        .unwrap()["trace_search"]["historical_interval"] = serde_json::Value::Null;
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("exact original TimeRequirement"))
    );
}

#[test]
fn resource_identity_matches_existing_batch_identity_quantity_without_fanout() {
    let (case, mut elaboration) = identity_fixture();
    let batch = &mut elaboration.acquisitions[1];
    batch.contract.scope.subject = "The retained batch of trace resource identities".into();
    batch.contract.scope.selectors.remove("trace_id");
    batch.contract.acceptable_methods = vec!["tempo.batch-trace-resource-identities/1".into()];
    batch.arguments = json!({"resource":"catalog-sync"});
    let batch_scope = batch.contract.scope.clone();
    let batch_time = batch.contract.time.clone();
    elaboration.derivations[0].operation = DerivationOperation::AnyIdentityEqual;
    let required = elaboration.requirements[0].contract.as_mut().unwrap();
    required.acceptable_methods = vec![
        "engarg.any-identity-equal/1".into(),
        "kubernetes.resource-identity/1".into(),
        "tempo.batch-trace-resource-identities/1".into(),
    ];
    required.quantities[0].name = "any_identity_equal".into();
    required.scope.selectors.get_mut("operands").unwrap()["trace"] = json!(batch_scope);
    required.scope.selectors.get_mut("operand_times").unwrap()["trace"] = json!(batch_time);
    elaboration.evaluations[0].required_coverage = RequiredCoverage::Witness;
    assert!(
        elaboration
            .acquisitions
            .iter()
            .all(|acquisition| acquisition.for_each.is_none())
    );
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
}
