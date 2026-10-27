//! Regression tests for the retained catalogue route and its evidence lineage.
use eal_collector_contracts::{
    capability::{CapabilityDescriptor, compatible},
    execution::RequirementEvaluation,
};
use engarg::support_requirements::{AcquisitionBlueprint, SupportRequirement};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    source: AcquisitionBlueprint,
    audit: AcquisitionBlueprint,
    requirements: Vec<SupportRequirement>,
    evaluations: Vec<RequirementEvaluation>,
    other_evaluations: Vec<RequirementEvaluation>,
    source_capability: CapabilityDescriptor,
    audit_capability: CapabilityDescriptor,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/catalogue-composition.json")).unwrap()
}

#[test]
fn retained_route_has_a_complete_qualified_source_but_only_a_partial_calculator_qualification() {
    let f = fixture();
    f.source_capability.validate().unwrap();
    f.audit_capability.validate().unwrap();
    assert!(compatible(&f.source.contract, &f.source_capability).adequate);
    assert!(!compatible(&f.audit.contract, &f.audit_capability).adequate);
    let mappings = f
        .audit
        .bindings
        .iter()
        .map(|binding| {
            (
                binding.argument_pointer.as_str(),
                (
                    binding.output.step_id.as_str(),
                    binding.output.pointer.as_str(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        mappings,
        BTreeMap::from([
            ("/snapshot", ("AC-source", "/snapshot")),
            ("/items", ("AC-source", "/items")),
            ("/complete", ("AC-source", "/complete")),
        ])
    );
    assert_eq!(f.requirements.len(), 2);
    for evaluation in &f.evaluations {
        assert_eq!(evaluation.max_age_seconds, Some(3600));
        assert!(
            evaluation
                .applicability
                .iter()
                .any(|r| r.step_id == "AC-source" && r.pointer == "/complete")
        );
        assert!(
            evaluation
                .applicability
                .iter()
                .any(|r| r.step_id == "AC-audit" && r.pointer == "/complete")
        );
    }
}

fn elaboration(f: &Fixture) -> engarg::support_requirements::SupportElaboration {
    engarg::support_requirements::SupportElaboration {
        schema: engarg::support_requirements::LEGACY_SUPPORT_ELABORATION_SCHEMA.into(),
        requirements: f.requirements.clone(),
        acquisitions: vec![f.source.clone(), f.audit.clone()],
        derivations: vec![],
        evaluations: f
            .evaluations
            .iter()
            .chain(&f.other_evaluations)
            .cloned()
            .collect(),
        revisions: vec![],
        acquisition_revisions: vec![],
        alternative_support: vec![],
    }
}

fn check(
    f: &Fixture,
) -> Result<
    (
        engarg::support_graph::composition::IdentifierAuditProof,
        eal_collector_contracts::capability::SupportContract,
    ),
    engarg::Error,
> {
    let graph = elaboration(f);
    engarg::support_graph::composition::identifier_audit_proof(
        &graph,
        &graph.acquisitions[1],
        &graph.acquisitions[0],
        &f.audit_capability,
        &f.source_capability,
        &BTreeMap::new(),
    )
}

#[test]
fn retained_complete_snapshot_and_calculator_form_a_qualified_route() {
    let f = fixture();
    check(&f).expect("the verified source and calculator must compose");
    // The route proof must not make the calculator itself a fixture authority.
    let audit_authorities = &f
        .audit_capability
        .semantics
        .as_ref()
        .unwrap()
        .provenance
        .authorities;
    assert!(!audit_authorities.iter().any(|a| a == "catalogue-fixture"));
}

#[test]
fn rejects_missing_substituted_or_misdirected_source_bindings() {
    let mut f = fixture();
    f.audit.bindings.retain(|b| b.argument_pointer != "/items");
    assert!(
        check(&f).is_err(),
        "a missing population input cannot justify a population calculation"
    );

    let mut f = fixture();
    f.audit
        .bindings
        .iter_mut()
        .find(|b| b.argument_pointer == "/items")
        .unwrap()
        .output
        .step_id = "other-source".into();
    assert!(
        check(&f).is_err(),
        "items from a different source cannot inherit snapshot coverage"
    );

    let mut f = fixture();
    f.audit
        .bindings
        .iter_mut()
        .find(|b| b.argument_pointer == "/items")
        .unwrap()
        .output
        .pointer = "/complete".into();
    assert!(
        check(&f).is_err(),
        "a completeness flag is not the selected population"
    );

    let mut f = fixture();
    f.audit
        .bindings
        .iter_mut()
        .find(|b| b.argument_pointer == "/snapshot")
        .unwrap()
        .output
        .pointer = "/items".into();
    assert!(
        check(&f).is_err(),
        "snapshot identity must be forwarded from the qualified source"
    );
}

#[test]
fn rejects_wrong_population_incomplete_qualification_and_provenance_laundering() {
    let mut f = fixture();
    f.source_capability.semantics.as_mut().unwrap().coverage[0].population =
        Some("other-population".into());
    assert!(
        check(&f).is_err(),
        "a different population cannot qualify this result"
    );

    let mut f = fixture();
    f.source_capability.semantics.as_mut().unwrap().coverage[0].strength =
        eal_collector_contracts::capability::LogicalStrength::ExistentialWitness;
    assert!(
        check(&f).is_err(),
        "a witness is not a complete population enumeration"
    );

    let mut f = fixture();
    f.source_capability
        .semantics
        .as_mut()
        .unwrap()
        .provenance
        .authorities
        .clear();
    f.audit_capability
        .semantics
        .as_mut()
        .unwrap()
        .provenance
        .authorities
        .push("catalogue-fixture".into());
    assert!(
        check(&f).is_err(),
        "the calculator cannot assert the missing source authority"
    );

    let mut f = fixture();
    f.audit_capability
        .semantics
        .as_mut()
        .unwrap()
        .provenance
        .authorities
        .clear();
    assert!(
        check(&f).is_err(),
        "source authority cannot substitute for calculator qualification"
    );
}

#[test]
fn rejects_missing_completeness_gates_and_unqualified_original_clock() {
    let mut f = fixture();
    f.evaluations[0]
        .applicability
        .retain(|r| !(r.step_id == "AC-source" && r.pointer == "/complete"));
    assert!(
        check(&f).is_err(),
        "each factual result requires observed source completeness"
    );

    let mut f = fixture();
    f.evaluations[1]
        .applicability
        .retain(|r| !(r.step_id == "AC-audit" && r.pointer == "/complete"));
    assert!(
        check(&f).is_err(),
        "audit completeness must be independently gated"
    );

    let mut f = fixture();
    f.source_capability
        .semantics
        .as_mut()
        .unwrap()
        .time
        .original_observation_time = false;
    assert!(
        check(&f).is_err(),
        "a fresh calculation cannot stand in for the original source clock"
    );
}

#[test]
fn rejects_missing_or_wrong_qualified_population_carrier() {
    let mut f = fixture();
    f.source_capability.semantics.as_mut().unwrap().coverage[0].population_quantity = None;
    assert!(
        check(&f).is_err(),
        "an unqualified array can be only a subset of a complete enumeration"
    );

    let mut f = fixture();
    f.source_capability.semantics.as_mut().unwrap().coverage[0].population_quantity =
        Some("complete".into());
    assert!(
        check(&f).is_err(),
        "the Boolean completion flag is not the full population carrier"
    );
}

#[test]
fn rejects_same_schema_filtered_subset_of_a_complete_population() {
    let mut f = fixture();
    let mut subset_quantity = f
        .source
        .contract
        .quantities
        .iter()
        .find(|q| q.name == "items")
        .unwrap()
        .clone();
    subset_quantity.name = "subset".into();
    f.source.contract.quantities.push(subset_quantity);
    let mut subset_descriptor = f
        .source_capability
        .semantics
        .as_ref()
        .unwrap()
        .quantities
        .iter()
        .find(|q| q.name == "items")
        .unwrap()
        .clone();
    subset_descriptor.name = "subset".into();
    subset_descriptor.output_pointer = "/subset".into();
    f.source_capability
        .semantics
        .as_mut()
        .unwrap()
        .quantities
        .push(subset_descriptor);
    let subset_schema = f.source_capability.output_schema["properties"]["items"].clone();
    f.source_capability.output_schema["properties"]["subset"] = subset_schema;
    f.source_capability.output_schema["required"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("subset"));
    f.source_capability.output_schema_sha256 =
        eal_collector_contracts::capability::schema_identity(&f.source_capability.output_schema)
            .unwrap();
    f.audit
        .bindings
        .iter_mut()
        .find(|b| b.argument_pointer == "/items")
        .unwrap()
        .output
        .pointer = "/subset".into();
    assert!(
        compatible(&f.source.contract, &f.source_capability).adequate,
        "the source can provide both its complete population and a filtered subset"
    );
    assert!(
        check(&f).is_err(),
        "equal JSON schemas do not establish that the filtered subset is the qualified full population"
    );
}

#[test]
fn rejects_missing_or_forged_calculator_qualification() {
    let mut f = fixture();
    f.audit_capability
        .semantics
        .as_mut()
        .unwrap()
        .identifier_audit = None;
    assert!(
        check(&f).is_err(),
        "function probes alone do not qualify source/calculator correspondence"
    );

    let mut f = fixture();
    f.audit_capability
        .semantics
        .as_mut()
        .unwrap()
        .identifier_audit
        .as_mut()
        .unwrap()
        .result_count_pointer = "/distinct".into();
    assert!(
        check(&f).is_err(),
        "a Boolean distinctness value cannot serve as the numeric population count"
    );

    let mut f = fixture();
    f.audit_capability
        .semantics
        .as_mut()
        .unwrap()
        .identifier_audit
        .as_mut()
        .unwrap()
        .argument_items_pointer = "/complete".into();
    assert!(
        check(&f).is_err(),
        "the qualified population input must be the bound array"
    );
}

#[test]
fn rejects_mismatched_input_quantity_units_even_when_json_schemas_match() {
    let mut f = fixture();
    f.audit_capability.input_schema["properties"]["items"]["x-engarg-unit"] =
        serde_json::json!("entries");
    f.audit_capability.input_schema_sha256 =
        eal_collector_contracts::capability::schema_identity(&f.audit_capability.input_schema)
            .unwrap();
    assert!(
        check(&f).is_err(),
        "identical JSON structure cannot repair a declared quantity-unit mismatch"
    );
}

#[test]
fn rejects_a_physical_input_mapping_that_swaps_population_and_completeness() {
    let f = fixture();
    let graph = elaboration(&f);
    let bindings = BTreeMap::from([
        ("/items".into(), "/complete".into()),
        ("/complete".into(), "/items".into()),
    ]);
    assert!(
        engarg::support_graph::composition::identifier_audit_proof(
            &graph,
            &graph.acquisitions[1],
            &graph.acquisitions[0],
            &f.audit_capability,
            &f.source_capability,
            &bindings,
        )
        .is_err(),
        "logical references cannot be remapped to incompatible physical inputs"
    );
}

#[test]
fn rejects_a_source_qualified_for_a_different_snapshot_identity() {
    let mut f = fixture();
    f.source_capability
        .semantics
        .as_mut()
        .unwrap()
        .scope_constraints
        .insert("snapshot".into(), serde_json::json!("another-snapshot"));
    assert!(
        check(&f).is_err(),
        "population coverage for a different snapshot cannot satisfy this scope"
    );
}

#[test]
fn actual_retained_unavailable_and_judgement_consumers_remain_pending() {
    let f = fixture();
    let before = serde_json::to_value(&f.other_evaluations).unwrap();
    check(&f).expect("unscoped retained reviews must not block independent count acquisition");
    assert_eq!(serde_json::to_value(&f.other_evaluations).unwrap(), before);
    let kinds = f
        .other_evaluations
        .iter()
        .map(|evaluation| {
            serde_json::to_value(&evaluation.criterion).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds.iter().filter(|kind| *kind == "judgement").count(), 3);
    assert_eq!(
        kinds.iter().filter(|kind| *kind == "unavailable").count(),
        2
    );
}

#[test]
fn rejects_scoped_judgement_or_unrelated_measured_output_relying_on_composite_coverage() {
    let mut f = fixture();
    let scope = f.audit.contract.scope.clone();
    let evaluation = f
        .other_evaluations
        .iter_mut()
        .find(|e| e.id == "EV-stronger")
        .unwrap();
    if let eal_collector_contracts::execution::EvaluationCriterion::Judgement {
        scope: value, ..
    } = &mut evaluation.criterion
    {
        *value = Some(scope);
    } else {
        panic!("expected retained judgement");
    }
    assert!(
        check(&f).is_err(),
        "a scoped judgement cannot inherit count/distinctness coverage"
    );

    let mut f = fixture();
    let evaluation = f
        .other_evaluations
        .iter_mut()
        .find(|e| e.id == "EV-formal")
        .unwrap();
    evaluation.criterion = eal_collector_contracts::execution::EvaluationCriterion::Equals {
        expected: serde_json::json!(true),
    };
    evaluation.required_coverage =
        eal_collector_contracts::execution::RequiredCoverage::CompleteEnumeration;
    assert!(
        check(&f).is_err(),
        "coverage proof for count and distinctness cannot qualify an unrelated formal output"
    );
}

#[test]
fn graph_projection_replays_the_selected_composition_and_rejects_tampering() {
    use eal_collector_contracts::{
        capability::canonical_identity,
        execution::{
            AcquisitionStep, EvidenceExecutionPlan, PLAN_SCHEMA, SourceAvailability, SourceIdentity,
        },
    };
    use engarg::support_graph::{
        AcquisitionRouteProof, materialise_source_arguments, project_execution_graph,
        validate_execution_graph,
    };
    let f = fixture();
    let graph = elaboration(&f);
    let (proof, _) = check(&f).expect("retained graph must qualify before compilation");
    let steps = [
        (&f.source, &f.source_capability),
        (&f.audit, &f.audit_capability),
    ]
    .into_iter()
    .map(|(blueprint, capability)| {
        let (arguments, gaps) =
            materialise_source_arguments(blueprint, capability, None, &BTreeMap::new()).unwrap();
        if blueprint.id == "AC-audit" {
            assert!(
                gaps.iter()
                    .any(|gap| gap.contains("Dynamic input may replace fixed scope selector")),
                "ordinary input admission must remain fail-closed without a composition proof"
            );
        } else {
            assert!(
                gaps.is_empty(),
                "source fixture inputs must be admissible: {gaps:?}"
            );
        }
        let mut scope = capability
            .semantics
            .as_ref()
            .unwrap()
            .scope_constraints
            .clone();
        scope.extend(blueprint.contract.scope.selectors.clone());
        AcquisitionStep {
            id: blueprint.id.clone(),
            source: SourceIdentity {
                provider_id: capability.provider.id.clone(),
                capability_id: capability.id.clone(),
                capability_version: capability.version.clone(),
                input_schema_sha256: capability.input_schema_sha256.clone(),
                output_schema_sha256: capability.output_schema_sha256.clone(),
                qualified_contract_sha256: capability.measurement_identity().unwrap(),
                configuration_sha256: capability.provider.configuration_sha256.clone(),
                authentication_scope_sha256: canonical_identity(
                    &capability.provider.authentication_scope,
                )
                .unwrap(),
                scope: serde_json::to_value(scope).unwrap(),
                historical_interval: None,
            },
            arguments,
            bindings: blueprint.bindings.clone(),
            for_each: None,
            availability: SourceAvailability::Configured,
            reuse: Default::default(),
            max_age_seconds: blueprint.contract.time.max_age_seconds,
            coverage_proof: None,
        }
    })
    .collect::<Vec<_>>();
    let routes = vec![
        AcquisitionRouteProof {
            batch: None,
            blueprint_id: f.source.id.clone(),
            capability: f.source_capability.clone(),
            projection_id: Some("source_quantities".into()),
            composition: None,
            input_bindings: BTreeMap::new(),
            operator_arguments: None,
        },
        AcquisitionRouteProof {
            batch: None,
            blueprint_id: f.audit.id.clone(),
            capability: f.audit_capability.clone(),
            projection_id: Some("audit_quantities".into()),
            composition: Some(proof),
            input_bindings: BTreeMap::new(),
            operator_arguments: None,
        },
    ];
    let mut plan = EvidenceExecutionPlan {
        schema: PLAN_SCHEMA.into(),
        acquisitions: steps,
        derivations: vec![],
        evaluations: graph.evaluations.clone(),
        scheduling: Default::default(),
    };
    project_execution_graph(&graph, &routes, &mut plan)
        .expect("the selected qualified route must compile");
    validate_execution_graph(&graph, &routes, &plan).expect("the execution graph must replay");
    let audit = plan
        .acquisitions
        .iter()
        .find(|step| step.id == "AC-audit")
        .unwrap();
    let witness = serde_json::to_value(audit.coverage_proof.as_ref().unwrap()).unwrap();
    assert_eq!(witness["kind"], "identifier_audit_v1");
    assert_eq!(witness["source_step_id"], "source_quantities");
    assert_eq!(witness["population_quantity"], "items");

    let mut bad_witness = plan.clone();
    let mut serialized = serde_json::to_value(&bad_witness.acquisitions[1]).unwrap();
    serialized["coverage_proof"]["source_qualified_contract_sha256"] =
        serde_json::json!("a".repeat(64));
    bad_witness.acquisitions[1] = serde_json::from_value(serialized).unwrap();
    assert!(
        validate_execution_graph(&graph, &routes, &bad_witness).is_err(),
        "the recorded source qualification cannot be changed"
    );

    let mut bad_binding = plan.clone();
    bad_binding.acquisitions[1]
        .bindings
        .iter_mut()
        .find(|b| b.argument_pointer == "/items")
        .unwrap()
        .output
        .pointer = "/complete".into();
    assert!(
        validate_execution_graph(&graph, &routes, &bad_binding).is_err(),
        "physical evidence binding cannot be changed after compilation"
    );

    let mut bad_route = routes.clone();
    bad_route[1]
        .composition
        .as_mut()
        .unwrap()
        .source_qualification_sha256 = "b".repeat(64);
    assert!(
        validate_execution_graph(&graph, &bad_route, &plan).is_err(),
        "selected qualification must replay from the actual source descriptor"
    );
}

#[path = "common/support.rs"]
mod support_fixture;

#[test]
fn planner_selects_both_qualified_routes_and_emits_replayable_eal_witness() {
    use eal_collector_contracts::capability::{
        CAPABILITY_CATALOGUE_SCHEMA, CapabilityCatalogue, CostDimension, RankingPolicy,
    };
    use engarg::route_planning::{
        DiscoverySnapshot, PLANNING_CONFIG_SCHEMA, PlanningConfig, compile,
    };
    use engarg::support_graph::validate_execution_graph;
    let f = fixture();
    let (_, case, mut graph, _, _, _) = support_fixture::fixture();
    graph
        .requirements
        .retain(|r| !r.id.starts_with("shared_requirement_"));
    graph
        .evaluations
        .retain(|e| !e.requirement_id.starts_with("shared_requirement_"));
    for mut requirement in f.requirements.clone() {
        requirement.target_id = "G1".into();
        requirement.evidence_need_ids.clear();
        requirement.original_acceptance_criteria.clear();
        graph.requirements.push(requirement);
    }
    graph.acquisitions = vec![f.source.clone(), f.audit.clone()];
    graph.evaluations.extend(f.evaluations.clone());
    let catalogue = CapabilityCatalogue {
        schema: CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities: vec![f.source_capability.clone(), f.audit_capability.clone()],
    };
    let config = PlanningConfig {
        schema: PLANNING_CONFIG_SCHEMA.into(),
        collector: "/operator/eal-collector".into(),
        connections: "/operator/connections.json".into(),
        store: "/operator/observations".into(),
        environment: vec![],
        ranking: RankingPolicy {
            dimensions: vec![CostDimension::ExecutionRequests],
        },
        prefer_configured: false,
        scheduling: Default::default(),
        bindings: vec![],
        batch_correspondences: vec![],
        operation_specifications: BTreeMap::new(),
    };
    let discovery = DiscoverySnapshot {
        admitted_capabilities: vec![
            f.source_capability.id.clone(),
            f.audit_capability.id.clone(),
        ],
        incomplete: false,
        gaps: vec![],
        metrics: serde_json::json!({}),
        ..Default::default()
    };
    let planned = compile(&case, &graph, &catalogue, &config, &discovery)
        .expect("planner must accept the composite route");
    let source = planned
        .routes
        .iter()
        .find(|r| r.acquisition_id == "AC-source")
        .unwrap();
    let audit = planned
        .routes
        .iter()
        .find(|r| r.acquisition_id == "AC-audit")
        .unwrap();
    assert_eq!(
        source.selected.as_deref(),
        Some(f.source_capability.id.as_str())
    );
    assert_eq!(
        audit.selected.as_deref(),
        Some(f.audit_capability.id.as_str())
    );
    assert!(
        audit.configuration_gaps.is_empty(),
        "the selected route must have no binding gaps"
    );
    let witness = serde_json::to_value(
        planned
            .execution
            .acquisitions
            .iter()
            .find(|a| a.id == "AC-audit")
            .unwrap()
            .coverage_proof
            .as_ref()
            .expect("qualified audit requires a runtime witness"),
    )
    .unwrap();
    assert_eq!(witness["kind"], "identifier_audit_v1");
    assert_eq!(witness["source_acquisition_id"], "AC-source");
    assert_eq!(witness["population_quantity"], "items");
    validate_execution_graph(&graph, &planned.projections, &planned.execution)
        .expect("planner output must pass independent replay");

    // A second eligible producer must never be silently substituted for the one
    // whose exact qualification is recorded in the composite proof. A future
    // selector can resolve the ambiguity, or safely leave the route unresolved.
    let mut alternate = f.source_capability.clone();
    alternate.id = "alternate_complete_source".into();
    alternate.provider.id = "alternate_complete_provider".into();
    let mut ambiguous = catalogue.clone();
    ambiguous.capabilities.push(alternate.clone());
    let mut ambiguous_discovery = discovery.clone();
    ambiguous_discovery
        .admitted_capabilities
        .push(alternate.id.clone());
    if let Ok(result) = compile(&case, &graph, &ambiguous, &config, &ambiguous_discovery) {
        let selected_source = result
            .routes
            .iter()
            .find(|r| r.acquisition_id == "AC-source")
            .unwrap();
        let selected_audit = result
            .routes
            .iter()
            .find(|r| r.acquisition_id == "AC-audit")
            .unwrap();
        if selected_audit.selected.is_some() {
            let proof = result
                .projections
                .iter()
                .find(|p| p.blueprint_id == "AC-audit")
                .and_then(|p| p.composition.as_ref())
                .expect("composed audit must retain its proof");
            assert_eq!(
                selected_source.selected.as_deref(),
                Some(proof.source_capability_id.as_str())
            );
            validate_execution_graph(&graph, &result.projections, &result.execution).unwrap();
        }
    }
}

#[test]
fn rejects_ambiguous_identity_rebinding_and_fanout() {
    let mut f = fixture();
    f.audit
        .bindings
        .iter_mut()
        .find(|binding| binding.argument_pointer == "/snapshot")
        .unwrap()
        .output
        .step_id = "alternate-source".into();
    assert!(
        check(&f).is_err(),
        "identity cannot be rebound from an unqualified producer"
    );

    let f = fixture();
    let graph = elaboration(&f);
    let overlap = BTreeMap::from([("/snapshot/nested".into(), "/snapshot".into())]);
    assert!(
        engarg::support_graph::composition::identifier_audit_proof(
            &graph,
            &graph.acquisitions[1],
            &graph.acquisitions[0],
            &f.audit_capability,
            &f.source_capability,
            &overlap,
        )
        .is_err(),
        "identity remapping cannot replace part of an admitted selector"
    );

    let overlap = BTreeMap::from([
        ("/snapshot".into(), "/snapshot".into()),
        ("/snapshot/nested".into(), "/items".into()),
    ]);
    assert!(
        engarg::support_graph::composition::identifier_audit_proof(
            &graph,
            &graph.acquisitions[1],
            &graph.acquisitions[0],
            &f.audit_capability,
            &f.source_capability,
            &overlap,
        )
        .is_err(),
        "overlapping destination pointers must not obscure a fixed selector"
    );

    let mut f = fixture();
    f.audit.for_each = Some(
        serde_json::from_value(serde_json::json!({
            "output": {"step_id": "AC-source", "pointer": "/items"},
            "argument_pointer": "/items", "maximum_items": 3, "stop_on_witness": null
        }))
        .unwrap(),
    );
    assert!(
        check(&f).is_err(),
        "a whole-population proof cannot be reused for fanout"
    );
}

#[test]
fn rejects_unqualified_custom_schema_metadata_on_population_input() {
    let mut f = fixture();
    f.audit_capability.input_schema["properties"]["items"]["x-audit-meaning"] =
        serde_json::json!("filtered-subset");
    f.audit_capability.input_schema_sha256 =
        eal_collector_contracts::capability::schema_identity(&f.audit_capability.input_schema)
            .unwrap();
    assert!(
        check(&f).is_err(),
        "unknown semantic metadata must not be erased when comparing schemas"
    );
}
