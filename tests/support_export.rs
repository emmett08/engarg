//! The new support bundle leaves the historical authored projection unchanged.
use eal_collector_contracts::{
    capability::{CapabilityDescriptor, SupportType},
    execution::{
        AcquisitionStep, EvaluationCriterion, EvidenceExecutionPlan, SourceIdentity,
        canonical_digest,
    },
};
use engarg::{
    domain::{ArgumentCase, ArgumentIndex, Request},
    export_policy::ExportPolicy,
    support_export::{self, AcquisitionRouteProof, SupportExportArtifacts, SupportExportLocations},
    support_requirements::{AcquisitionBlueprint, SupportElaboration, SupportRequirement},
};
use serde_json::json;
use std::path::PathBuf;

#[path = "common/support.rs"]
mod support_fixture;
use support_fixture::{descriptor, fixture};

fn search_detail_fixture() -> (
    Request,
    ArgumentCase,
    SupportElaboration,
    EvidenceExecutionPlan,
    Vec<AcquisitionRouteProof>,
    SupportExportLocations,
) {
    let (request, case, mut elaboration, mut plan, mut routes, locations) = fixture();
    let mut detail_contract = elaboration.acquisitions[0].contract.clone();
    detail_contract.quantities =
        vec![serde_json::from_value(json!({"name":"match","unit":"boolean"})).unwrap()];
    elaboration.acquisitions[0].contract.quantities =
        vec![serde_json::from_value(json!({"name":"ids","unit":"trace_id"})).unwrap()];
    let fanout = eal_collector_contracts::execution::ForEachBinding {
        output: serde_json::from_value(json!({"step_id":"shared_source","pointer":"/ids"}))
            .unwrap(),
        argument_pointer: "/trace_id".into(),
        maximum_items: 3,
        stop_on_witness: Some(eal_collector_contracts::execution::WitnessPredicate {
            pointer: "/match".into(),
            equals: Some(json!(true)),
            equals_output: None,
        }),
    };
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "trace_details".into(),
        contract: detail_contract.clone(),
        arguments: json!({"case":"dev","trace_id":null}),
        bindings: vec![],
        for_each: Some(fanout.clone()),

        operation_spec: None,
    });
    for requirement in elaboration
        .requirements
        .iter_mut()
        .filter(|requirement| requirement.id.starts_with("shared_requirement"))
    {
        requirement.contract = Some(detail_contract.clone());
        requirement.acceptance = EvaluationCriterion::AnyTrue;
        let evaluation = elaboration
            .evaluations
            .iter_mut()
            .find(|evaluation| evaluation.requirement_id == requirement.id)
            .unwrap();
        evaluation.inputs = vec![
            serde_json::from_value(json!({"step_id":"trace_details","pointer":"/match"})).unwrap(),
        ];
        evaluation.criterion = EvaluationCriterion::AnyTrue;
    }
    let mut search = descriptor();
    search.id = "search_ids".into();
    search.semantics.as_mut().unwrap().quantities = vec![
        serde_json::from_value(
            json!({"name":"ids","unit":"trace_id","output_pointer":"/results/trace_ids"}),
        )
        .unwrap(),
    ];
    let mut detail = descriptor();
    detail.id = "trace_detail".into();
    detail.semantics.as_mut().unwrap().quantities = vec![
        serde_json::from_value(
            json!({"name":"match","unit":"boolean","output_pointer":"/data/matched"}),
        )
        .unwrap(),
    ];
    let source = |capability: &CapabilityDescriptor| SourceIdentity {
        provider_id: capability.provider.id.clone(),
        capability_id: capability.id.clone(),
        capability_version: capability.version.clone(),
        input_schema_sha256: capability.input_schema_sha256.clone(),
        output_schema_sha256: capability.output_schema_sha256.clone(),
        qualified_contract_sha256: capability.measurement_identity().unwrap(),
        configuration_sha256: capability.provider.configuration_sha256.clone(),
        authentication_scope_sha256: eal_collector_contracts::capability::canonical_identity(
            &capability.provider.authentication_scope,
        )
        .unwrap(),
        scope: json!({"case":"dev"}),
        historical_interval: None,
    };
    plan.acquisitions[0].source = source(&search);
    plan.acquisitions.push(AcquisitionStep {
        coverage_proof: None,
        id: "trace_details".into(),
        max_age_seconds: Some(300),
        source: source(&detail),
        arguments: json!({"case":"dev","query":{"trace_id":null}}),
        bindings: vec![],
        for_each: Some(fanout),
        availability: Default::default(),
        reuse: Default::default(),
    });
    routes[0].capability = search;
    routes[0].projection_id = Some("search_quantities".into());
    routes.push(AcquisitionRouteProof {
        composition: None,
        batch: None,
        blueprint_id: "trace_details".into(),
        capability: detail,
        projection_id: Some("detail_quantities".into()),
        input_bindings: std::collections::BTreeMap::from([(
            "/query/trace_id".into(),
            "/trace_id".into(),
        )]),
        operator_arguments: Some(json!({"case":"dev","query":{"trace_id":null}})),
    });
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    (request, case, elaboration, plan, routes, locations)
}

#[test]
fn search_detail_witness_and_project_nodes_use_qualified_nested_output_pointers() {
    let (request, case, elaboration, plan, routes, locations) = search_detail_fixture();
    let fanout = plan.acquisitions[1].for_each.as_ref().unwrap();
    assert_eq!(fanout.output.step_id, "search_quantities");
    assert_eq!(fanout.output.pointer, "/ids");
    assert_eq!(fanout.argument_pointer, "/query/trace_id");
    assert_eq!(
        fanout.stop_on_witness.as_ref().unwrap().pointer,
        "/data/matched"
    );
    assert_eq!(
        fanout.stop_on_witness.as_ref().unwrap().equals,
        Some(json!(true))
    );
    assert_eq!(
        elaboration.acquisitions[1]
            .for_each
            .as_ref()
            .unwrap()
            .stop_on_witness
            .as_ref()
            .unwrap()
            .pointer,
        "/match",
        "the authored logical predicate is unchanged"
    );
    let compiled = serde_json::to_value(&plan).unwrap();
    assert!(
        compiled["derivations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["id"] == "detail_quantities"
                && step["operation"]
                    == json!({"kind":"project","fields":{"match":"/data/matched"}}))
    );
    assert!(
        compiled["evaluations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|evaluation| evaluation["requirement_id"]
                .as_str()
                .unwrap()
                .starts_with("shared_requirement"))
            .all(|evaluation| evaluation["inputs"]
                == json!([{"step_id":"detail_quantities","pointer":"/match"}]))
    );
    let exported = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    support_export::reconstruct_bundle(&exported.bundle).unwrap();
    let mut tampered = plan.clone();
    tampered.acquisitions[1]
        .for_each
        .as_mut()
        .unwrap()
        .stop_on_witness
        .as_mut()
        .unwrap()
        .pointer = "/match".into();
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &tampered,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
    for pointer in ["/unknown", "/match/partial", ""] {
        let mut changed = elaboration.clone();
        changed.acquisitions[1]
            .for_each
            .as_mut()
            .unwrap()
            .stop_on_witness
            .as_mut()
            .unwrap()
            .pointer = pointer.into();
        assert!(
            support_export::project_execution_graph(&changed, &routes, &mut plan.clone()).is_err(),
            "{pointer} must not select an undeclared or partial quantity"
        );
    }
}
#[test]
fn dynamic_witness_references_are_projected_and_retained_in_requirement_lineage() {
    let (request, case, mut elaboration, mut execution, routes, locations) =
        search_detail_fixture();
    let mut expected_blueprint = elaboration.acquisitions[0].clone();
    expected_blueprint.id = "expected_source".into();
    expected_blueprint.contract.quantities =
        vec![serde_json::from_value(json!({"name":"expected_match","unit":"boolean"})).unwrap()];
    elaboration.acquisitions.push(expected_blueprint);
    let mut routes = routes;
    let mut expected_route = routes[0].clone();
    expected_route.blueprint_id = "expected_source".into();
    expected_route.projection_id = Some("expected_quantities".into());
    expected_route.capability.id = "expected_match_source".into();
    expected_route.capability.semantics.as_mut().unwrap().quantities = vec![serde_json::from_value(json!({"name":"expected_match","unit":"boolean","output_pointer":"/results/expected_match"})).unwrap()];
    let mut expected_step = execution.acquisitions[0].clone();
    expected_step.id = "expected_source".into();
    expected_step.source.capability_id = expected_route.capability.id.clone();
    expected_step.source.qualified_contract_sha256 =
        expected_route.capability.measurement_identity().unwrap();
    execution.acquisitions.push(expected_step);
    routes.push(expected_route);
    let witness = elaboration.acquisitions[1]
        .for_each
        .as_mut()
        .unwrap()
        .stop_on_witness
        .as_mut()
        .unwrap();
    witness.equals = None;
    witness.equals_output = Some(
        serde_json::from_value(json!({"step_id":"expected_source","pointer":"/expected_match"}))
            .unwrap(),
    );
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    let witness = execution.acquisitions[1]
        .for_each
        .as_ref()
        .unwrap()
        .stop_on_witness
        .as_ref()
        .unwrap();
    assert_eq!(witness.pointer, "/data/matched");
    assert!(witness.equals.is_none());
    assert_eq!(
        serde_json::to_value(&witness.equals_output).unwrap(),
        json!({"step_id":"expected_quantities","pointer":"/expected_match"})
    );
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    for requirement in artifacts
        .bundle
        .requirements
        .iter()
        .filter(|requirement| requirement.requirement_id.starts_with("shared_requirement"))
    {
        assert!(
            requirement
                .source_step_ids
                .contains(&"expected_source".into())
        );
        assert!(
            requirement
                .derived_step_ids
                .contains(&"expected_quantities".into())
        );
    }
    support_export::reconstruct(&artifacts.bundle).unwrap();
    let mut tampered = execution;
    tampered.acquisitions[1]
        .for_each
        .as_mut()
        .unwrap()
        .stop_on_witness
        .as_mut()
        .unwrap()
        .equals_output
        .as_mut()
        .unwrap()
        .step_id = "shared_source".into();
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &tampered,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
}

fn exported() -> SupportExportArtifacts {
    let (request, case, elaboration, plan, routes, locations) = fixture();
    support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap()
}

#[test]
fn shared_sources_have_separate_typed_gates_and_the_original_authored_case() {
    let artifacts = exported();
    let bundle = &artifacts.bundle;
    assert_eq!(bundle.target_language, "EAL/3");
    assert!(bundle.source.starts_with(&bundle.authored.source));
    let ordinary = engarg::eal_export::export(
        &bundle.authored.original_request,
        &bundle.authored.original_case,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&bundle.authored).unwrap(),
        serde_json::to_value(ordinary).unwrap()
    );
    assert!(engarg::eal_acquisition::reconstruct_bundle(&bundle.authored).is_ok());
    let shared: Vec<_> = bundle
        .requirements
        .iter()
        .filter(|requirement| requirement.requirement_id.starts_with("shared_requirement"))
        .collect();
    assert_eq!(shared.len(), 3);
    for requirement in &shared {
        assert_eq!(requirement.source_step_ids, ["shared_source"]);
    }
    assert_ne!(shared[0].evaluation_id, shared[1].evaluation_id);
    assert!(
        shared
            .iter()
            .all(|requirement| requirement.support_type == Some(SupportType::Observation))
    );
    assert_eq!(
        bundle
            .requirements
            .iter()
            .find(|requirement| requirement.requirement_id == "typed_calculation")
            .unwrap()
            .support_type,
        Some(SupportType::Calculation)
    );
    assert_eq!(
        bundle
            .requirements
            .iter()
            .find(|requirement| requirement.requirement_id == "typed_proof")
            .unwrap()
            .support_type,
        Some(SupportType::ProofCheck)
    );
    assert!(
        bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "independent_judgement"
                && pending.requirement_id.as_deref() == Some("retained_judgement"))
    );
    assert!(!bundle.authored.obligations.is_empty());
    for need in &bundle.authored.original_case.evidence_plan {
        assert!(
            bundle.source_map[&need.id]
                .declarations
                .iter()
                .any(|declaration| declaration.role == "generated_support_evaluation")
        );
    }
    assert!(
        bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "independent_review_registration")
    );
    assert_eq!(artifacts.registry.tools.len(), 1);
    assert!(!artifacts.tools_toml.contains("REPLACE"));
    assert!(eal_collector_contracts::RegistryDocument::from_toml(&artifacts.tools_toml).is_ok());
    assert_eq!(
        artifacts.registry.tools["engarg_support_graph"].argv[3],
        "/operator/run/execution.json"
    );
    assert_eq!(
        bundle.execution_plan_sha256,
        canonical_digest(&bundle.execution_plan).unwrap()
    );
    for request in artifacts
        .acquisition_requests
        .iter()
        .chain(&artifacts.review_requests)
    {
        assert_eq!(request.context, bundle.context);
        assert_eq!(request.input["case_digest"], bundle.authored.revision);
    }
    for request in &artifacts.acquisition_requests {
        assert_eq!(
            request.input["execution_plan_sha256"],
            bundle.execution_plan_sha256
        );
    }
    assert_eq!(
        serde_json::to_value(support_export::reconstruct_bundle(bundle).unwrap()).unwrap(),
        serde_json::to_value(bundle).unwrap()
    );
}

#[test]
fn gates_follow_selected_dependency_cones_and_keep_generated_nodes_distinct() {
    let artifacts = exported();
    let bundle = &artifacts.bundle;
    let index = ArgumentIndex::new(&bundle.authored.original_case);
    let alternative_gates =
        engarg::support_requirements::alternative_requirement_gates(&bundle.support_elaboration);
    for requirement in &bundle.requirements {
        for id in bundle.claim_names.keys() {
            assert_eq!(
                requirement.gated_claim_ids.contains(id),
                index
                    .selected_dependency(id)
                    .contains(requirement.target_id.as_str())
                    || alternative_gates
                        .get(&requirement.requirement_id)
                        .is_some_and(|claims| claims.contains(id))
            );
        }
    }
    assert_ne!(
        bundle.claim_names["current_claim"],
        bundle.authored.claim_names["current_claim"]
    );
    assert!(
        bundle.source_map["current_claim"]
            .declarations
            .iter()
            .any(|declaration| declaration.role == "generated_support_gate")
    );
    assert!(
        !bundle
            .requirements
            .iter()
            .find(|requirement| requirement.target_id == "W3")
            .unwrap()
            .gated_claim_ids
            .contains(&"current_claim".into())
    );
}

#[test]
fn changes_to_source_graph_schema_requests_paths_and_canonical_mapping_are_rejected() {
    let bundle = exported().bundle;
    for mutation in 0..9 {
        let mut changed = bundle.clone();
        match mutation {
            0 => changed.schema_version = "engarg.support-export/99".into(),
            1 => changed.source.push_str("\n// changed\n"),
            2 => changed.execution_plan.acquisitions[0].arguments["case"] = json!("different"),
            3 => changed.execution_plan_sha256 = "0".repeat(64),
            4 => changed.locations.execution_plan = "/operator/other/execution.json".into(),
            5 => changed.context["engarg_support_export_digest"] = json!("0".repeat(64)),
            6 => changed
                .claim_names
                .insert(
                    "current_claim".into(),
                    bundle.authored.claim_names["current_claim"].clone(),
                )
                .map(drop)
                .unwrap(),
            7 => changed.requirements[0]
                .source_step_ids
                .push("invented_independent_source".into()),
            _ => {
                changed
                    .registry
                    .tools
                    .get_mut("engarg_support_graph")
                    .unwrap()
                    .argv[3] = "/other/plan.json".into()
            }
        }
        assert!(
            support_export::reconstruct_bundle(&changed).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn execution_cannot_weaken_criteria_dependencies_or_coverage_and_paths_are_final() {
    let (request, case, elaboration, plan, routes, locations) = fixture();
    for mutation in 0..5 {
        let mut changed = plan.clone();
        let mut paths = locations.clone();
        match mutation {
            0 => {
                changed.evaluations[0].criterion = EvaluationCriterion::Equals {
                    expected: json!(true),
                }
            }
            1 => {
                changed.evaluations[0].required_coverage =
                    eal_collector_contracts::execution::RequiredCoverage::CompleteEnumeration
            }
            2 => changed.acquisitions[0].id = "different_source".into(),
            3 => paths.execution_plan = "relative/execution.json".into(),
            _ => paths.connections = paths.execution_plan.clone(),
        }
        assert!(
            support_export::export(
                &request,
                &case,
                &elaboration,
                &changed,
                &routes,
                &paths,
                &ExportPolicy::default()
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn qualified_quantity_pointers_are_compiled_without_changing_acceptance() {
    let (request, case, mut elaboration, mut plan, mut routes, locations) = fixture();
    elaboration.acquisitions[0].contract.quantities =
        vec![serde_json::from_value(json!({"name":"value","unit":"count"})).unwrap()];
    routes[0].capability.semantics.as_mut().unwrap().quantities = vec![
        serde_json::from_value(
            json!({"name":"value","unit":"count","output_pointer":"/raw/measured"}),
        )
        .unwrap(),
    ];
    routes[0].projection_id = Some("qualified_quantities".into());
    plan.acquisitions[0].source.qualified_contract_sha256 =
        routes[0].capability.measurement_identity().unwrap();
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let projected = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        projected["derivations"][0]["operation"],
        json!({"kind":"project_value","fields":{"value":"/raw/measured"}})
    );
    for evaluation in plan
        .evaluations
        .iter()
        .filter(|evaluation| evaluation.requirement_id.starts_with("shared_requirement"))
    {
        assert_eq!(evaluation.inputs[0].step_id, "qualified_quantities");
        assert_eq!(evaluation.inputs[0].pointer, "/value");
        assert_eq!(
            canonical_digest(&evaluation.criterion).unwrap(),
            canonical_digest(
                &elaboration
                    .evaluations
                    .iter()
                    .find(|original| original.id == evaluation.id)
                    .unwrap()
                    .criterion
            )
            .unwrap()
        );
    }
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    assert_eq!(
        serde_json::to_value(&plan).unwrap(),
        projected,
        "projection compilation is idempotent"
    );
    support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    let mut tampered = plan.clone();
    if let eal_collector_contracts::execution::DerivationOperation::ProjectValue { fields } =
        &mut tampered.derivations[0].operation
    {
        fields.insert("value".into(), "/unqualified".into());
    }
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &tampered,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
    routes[0].capability.semantics.as_mut().unwrap().quantities[0].unit = "seconds".into();
    assert!(support_export::project_execution_graph(&elaboration, &routes, &mut plan).is_err());
}

#[test]
fn a_missing_route_can_only_remain_an_explicit_unavailable_source() {
    let (request, case, elaboration, mut plan, _routes, locations) = fixture();
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &plan,
            &[],
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
    plan.acquisitions[0].availability =
        eal_collector_contracts::execution::SourceAvailability::ConfigurationMissing {
            requirements: vec!["Select a qualified source".into()],
        };
    support_export::project_execution_graph(&elaboration, &[], &mut plan).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &[],
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "configuration_missing")
    );
}

#[test]
fn operator_input_mappings_preserve_dependency_values_and_only_change_declared_slots() {
    let (request, case, mut elaboration, mut plan, mut routes, locations) = fixture();
    let binding: eal_collector_contracts::execution::DependencyBinding=serde_json::from_value(json!({"output":{"step_id":"shared_source","pointer":"/value"},"argument_pointer":"/lookup"})).unwrap();
    let mut blueprint = elaboration.acquisitions[0].clone();
    blueprint.id = "downstream_source".into();
    blueprint.arguments = json!({"case":"dev","lookup":null});
    blueprint.bindings = vec![binding.clone()];
    elaboration.acquisitions.push(blueprint);
    elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == "shared_requirement_2")
        .unwrap()
        .inputs[0]
        .step_id = "downstream_source".into();
    let mut acquisition = plan.acquisitions[0].clone();
    acquisition.id = "downstream_source".into();
    acquisition.arguments = json!({"case":"dev","request":{"lookup":null}});
    acquisition.bindings = vec![binding];
    plan.acquisitions.push(acquisition);
    let mut proof = routes[0].clone();
    proof.blueprint_id = "downstream_source".into();
    proof.projection_id = Some("downstream_quantities".into());
    proof.input_bindings =
        std::collections::BTreeMap::from([("/request/lookup".into(), "/lookup".into())]);
    proof.operator_arguments = Some(json!({"case":"dev","request":{"lookup":null}}));
    routes.push(proof);
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    assert_eq!(
        plan.acquisitions[1].bindings[0].argument_pointer,
        "/request/lookup"
    );
    assert_eq!(
        plan.acquisitions[1].bindings[0].output.step_id,
        "qualified_quantities"
    );
    assert_eq!(plan.acquisitions[1].bindings[0].output.pointer, "/value");
    support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    plan.acquisitions[1].arguments["request"]["lookup"] = json!("altered input");
    assert!(support_export::project_execution_graph(&elaboration, &routes, &mut plan).is_err());
}

#[test]
fn graph_freshness_matches_runtime_and_unbounded_machine_support_needs_clarification() {
    let (request, case, mut elaboration, mut plan, routes, locations) = fixture();
    let policy = ExportPolicy {
        acquisition_max_age_seconds: 1,
        ..Default::default()
    };
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &policy,
    )
    .unwrap();
    assert!(artifacts.bundle.source.matches("max_age 300\n").count() >= 3);
    assert!(
        !artifacts.bundle.source.contains("max_age 1\n"),
        "legacy flat policy must not disagree with graph cache freshness"
    );
    elaboration
        .requirements
        .iter_mut()
        .find(|requirement| requirement.id == "shared_requirement_0")
        .unwrap()
        .contract
        .as_mut()
        .unwrap()
        .time
        .max_age_seconds = None;
    elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == "shared_requirement_0")
        .unwrap()
        .max_age_seconds = None;
    let admission = engarg::support_requirements::validate_freshness(&elaboration);
    assert!(
        admission
            .iter()
            .any(|error| error.contains("SUPPORT_FRESHNESS_CLARIFICATION")),
        "the same missing age must be rejected before discovery or coding: {admission:?}"
    );
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let error = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &policy,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("SUPPORT_FRESHNESS_CLARIFICATION"),
        "{error}"
    );
}

#[test]
fn acquisition_freshness_cannot_be_weakened_by_downstream_ages_or_missing_routes() {
    use eal_collector_contracts::execution::SourceAvailability;
    let (request, case, mut elaboration, plan, routes, locations) = fixture();
    elaboration.acquisitions[0].contract.time.max_age_seconds = Some(60);
    for availability in [
        SourceAvailability::Configured,
        SourceAvailability::ConfigurationMissing {
            requirements: vec!["Select the source".into()],
        },
        SourceAvailability::Unavailable {
            reason: "Source unavailable".into(),
        },
        SourceAvailability::HistoricalUnavailable {
            reason: "Historical record unavailable".into(),
        },
    ] {
        let selected = if matches!(availability, SourceAvailability::Configured) {
            routes.as_slice()
        } else {
            &[]
        };
        let mut compiled = plan.clone();
        compiled.acquisitions[0].availability = availability;
        // The pure graph API and the retained exporter API name the same type.
        let _: &engarg::support_graph::AcquisitionRouteProof = &routes[0];
        engarg::support_graph::project_execution_graph(&elaboration, selected, &mut compiled)
            .unwrap();
        assert_eq!(compiled.acquisitions[0].max_age_seconds, Some(60));
        assert_eq!(
            compiled
                .evaluations
                .iter()
                .find(|evaluation| evaluation.requirement_id == "shared_requirement_0")
                .unwrap()
                .max_age_seconds,
            Some(300),
            "downstream freshness does not replace the stricter source contract"
        );
        engarg::support_graph::validate_execution_graph(&elaboration, selected, &compiled).unwrap();
        support_export::export(
            &request,
            &case,
            &elaboration,
            &compiled,
            selected,
            &locations,
            &ExportPolicy::default(),
        )
        .unwrap();
        for altered in [None, Some(1), Some(61), Some(300)] {
            let mut changed = compiled.clone();
            changed.acquisitions[0].max_age_seconds = altered;
            assert!(
                engarg::support_graph::validate_execution_graph(&elaboration, selected, &changed)
                    .is_err(),
                "source limit {altered:?} cannot replace the declared60second contract"
            );
            let error = support_export::export(
                &request,
                &case,
                &elaboration,
                &changed,
                selected,
                &locations,
                &ExportPolicy::default(),
            )
            .unwrap_err();
            assert!(error.to_string().contains("source freshness"), "{error}");
            engarg::support_graph::project_execution_graph(&elaboration, selected, &mut changed)
                .unwrap();
            assert_eq!(changed.acquisitions[0].max_age_seconds, Some(60));
        }
    }
}

#[test]
fn historical_contract_and_physical_window_cannot_be_dropped_during_export() {
    let (request, case, mut elaboration, mut execution, mut routes, locations) = fixture();
    let history: eal_collector_contracts::capability::HistoricalInterval =
        serde_json::from_value(json!({"start_unix":1761264000,"end_unix":1761350400})).unwrap();
    elaboration.acquisitions[0]
        .contract
        .time
        .historical_interval = Some(history.clone());
    elaboration.acquisitions[0].arguments["start"] = json!(history.start_unix);
    elaboration.acquisitions[0].arguments["end"] = json!(history.end_unix);
    for requirement in elaboration
        .requirements
        .iter_mut()
        .filter(|requirement| requirement.id.starts_with("shared_requirement"))
    {
        requirement
            .contract
            .as_mut()
            .unwrap()
            .time
            .historical_interval = Some(history.clone());
    }
    execution.acquisitions[0].arguments = elaboration.acquisitions[0].arguments.clone();
    let semantics = routes[0].capability.semantics.as_mut().unwrap();
    semantics.time.historical_intervals = true;
    semantics.time.historical_input = Some(
        serde_json::from_value(
            json!({"start_pointer":"/start","end_pointer":"/end","encoding":"unix_seconds"}),
        )
        .unwrap(),
    );
    execution.acquisitions[0].source.qualified_contract_sha256 =
        routes[0].capability.measurement_identity().unwrap();
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    let baseline = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    support_export::reconstruct_bundle(&baseline.bundle).unwrap();
    for interval in [
        None,
        Some(
            serde_json::from_value(
                json!({"start":"2000-01-01T00:00:00Z","end":"2000-01-02T00:00:00Z"}),
            )
            .unwrap(),
        ),
    ] {
        let mut changed = execution.clone();
        changed.acquisitions[0].source.historical_interval = interval;
        assert!(
            support_export::export(
                &request,
                &case,
                &elaboration,
                &changed,
                &routes,
                &locations,
                &ExportPolicy::default()
            )
            .is_err()
        );
    }
    execution.acquisitions[0].arguments["start"] = json!(0);
    execution.acquisitions[0].arguments["end"] = json!(1);
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &execution,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
    assert!(
        support_export::project_execution_graph(&elaboration, &routes, &mut execution).is_err()
    );
}

#[test]
fn unmapped_static_arguments_and_changed_operator_projections_are_rejected() {
    let (request, case, mut elaboration, mut execution, mut routes, locations) = fixture();
    elaboration.acquisitions[0].arguments["query"] = json!("the exact original query");
    execution.acquisitions[0].arguments = elaboration.acquisitions[0].arguments.clone();
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    execution.acquisitions[0].arguments["query"] = json!("a different query");
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &execution,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
    routes[0].operator_arguments = Some(execution.acquisitions[0].arguments.clone());
    assert!(
        support_export::project_execution_graph(&elaboration, &routes, &mut execution).is_err(),
        "operator constants cannot silently replace known original logical values"
    );
    routes[0].operator_arguments =
        Some(json!({"case":"dev","request":{"query":null},"read_only":true}));
    routes[0].input_bindings =
        std::collections::BTreeMap::from([("/request/query".into(), "/query".into())]);
    execution.acquisitions[0].arguments =
        json!({"case":"dev","request":{"query":"the exact original query"},"read_only":true});
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    execution.acquisitions[0].arguments["read_only"] = json!(false);
    assert!(
        support_export::export(
            &request,
            &case,
            &elaboration,
            &execution,
            &routes,
            &locations,
            &ExportPolicy::default()
        )
        .is_err()
    );
}

#[test]
fn planned_authentication_and_complete_qualified_scope_must_match_route_proofs() {
    let (request, case, mut elaboration, mut execution, mut routes, locations) = fixture();
    elaboration.acquisitions[0]
        .contract
        .scope
        .selectors
        .insert("tenant".into(), json!("development-reader"));
    for requirement in &mut elaboration.requirements {
        if let Some(contract) = &mut requirement.contract {
            contract
                .scope
                .selectors
                .insert("tenant".into(), json!("development-reader"));
        }
    }
    routes[0]
        .capability
        .semantics
        .as_mut()
        .unwrap()
        .scope_constraints
        .insert("tenant".into(), json!("development-reader"));
    execution.acquisitions[0].source.scope["tenant"] = json!("development-reader");
    execution.acquisitions[0].source.qualified_contract_sha256 =
        routes[0].capability.measurement_identity().unwrap();
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    for mutation in 0..3 {
        let mut changed = execution.clone();
        match mutation {
            0 => changed.acquisitions[0].source.authentication_scope_sha256 = "0".repeat(64),
            1 => {
                changed.acquisitions[0]
                    .source
                    .scope
                    .as_object_mut()
                    .unwrap()
                    .remove("tenant");
            }
            _ => changed.acquisitions[0].source.scope["injected_scope"] = json!("foreign"),
        }
        assert!(
            support_export::export(
                &request,
                &case,
                &elaboration,
                &changed,
                &routes,
                &locations,
                &ExportPolicy::default()
            )
            .is_err(),
            "mutation{mutation} must not survive re-export"
        );
    }
}

#[test]
fn alternative_requirements_gate_explicit_claims_and_legacy_imports_remain_pending() {
    let (request, case, mut elaboration, execution, routes, locations) = fixture();
    elaboration.schema = engarg::support_requirements::SUPPORT_ELABORATION_SCHEMA.into();
    let bundle = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap()
    .bundle;
    let requirement = bundle
        .requirements
        .iter()
        .find(|requirement| requirement.requirement_id == "requirement_G2")
        .unwrap();
    assert!(
        requirement
            .gated_claim_ids
            .contains(&"current_claim".into()),
        "explicit alternative testing survives target placement outside the selected cone"
    );
    elaboration.schema = engarg::support_requirements::LEGACY_SUPPORT_ELABORATION_SCHEMA.into();
    elaboration.alternative_support.clear();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    let projection = artifacts.bundle.alternative_projection.as_ref().unwrap();
    assert_eq!(projection.guards.len(), case.alternatives.len());
    assert_eq!(
        projection.original_execution_plan.evaluations.len(),
        execution.evaluations.len()
    );
    assert_eq!(
        artifacts.execution_plan.evaluations.len(),
        execution.evaluations.len() + case.alternatives.len()
    );
    for guard in &projection.guards {
        assert!(
            artifacts
                .execution_plan
                .evaluations
                .iter()
                .any(
                    |evaluation| evaluation.requirement_id == guard.requirement_id
                        && matches!(
                            evaluation.criterion,
                            EvaluationCriterion::Unavailable { .. }
                        )
                )
        );
        assert!(guard.claim_ids.contains(&"current_claim".into()));
    }
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "legacy_alternative_support")
    );
    support_export::reconstruct_bundle(&artifacts.bundle).unwrap();
    let mut tampered = artifacts.bundle.clone();
    tampered
        .alternative_projection
        .as_mut()
        .unwrap()
        .guards
        .clear();
    assert!(support_export::reconstruct_bundle(&tampered).is_err());
}

fn saved_graph_report(path: &std::path::Path) {
    let (request, case, elaboration, plan, routes, locations) = fixture();
    let value = json!({"request":request,"argument":case,"transcript":[],"evidence_planning":{
        "schema":"engarg.support-planning/1","elaboration":elaboration,"routes":{
            "schema":engarg::route_planning::ROUTE_PLAN_SCHEMA,"execution":plan,"routes":[],"projections":routes,"gaps":[],
            "discovery":{"admitted_capabilities":["bounded_source"],"incomplete":false,"gaps":[],"metrics":{}},
            "configuration":{"schema":engarg::route_planning::PLANNING_CONFIG_SCHEMA,"collector":locations.collector,"connections":locations.connections,"store":locations.store,"environment":[],"ranking":{"dimensions":["execution_requests"]},"scheduling":{"maximum_concurrent_sources":2,"per_provider":{}},"bindings":[]},
        }
    }});
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn cli_graph_export_retains_an_explicit_independent_review_binding() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    saved_graph_report(&input);
    let binding = json!({
        "kind":"command", "version":"1", "argv":["/operator/independent-review"],
        "timeout_seconds":30, "max_output_bytes":1048576,
        "inherit_env":[], "parallel_safe":false,
    });
    let binding_path = directory.path().join("review-binding.json");
    std::fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    for supplied in [false, true] {
        let output = directory.path().join(if supplied {
            "bound.json"
        } else {
            "pending.json"
        });
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", "export", "--input"])
            .arg(&input)
            .args(["--format", "json", "--output"])
            .arg(&output);
        if supplied {
            command.arg("--review-binding").arg(&binding_path);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let bundle: engarg::support_export::SupportExport =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(bundle.locations.review_binding.is_some(), supplied);
        assert_eq!(
            bundle.registry.tools.contains_key("engarg_review_contract"),
            supplied
        );
        assert_eq!(
            bundle
                .pending
                .iter()
                .any(|pending| pending.kind == "independent_review_registration"),
            !supplied
        );
        if supplied {
            assert_eq!(
                serde_json::to_value(&bundle.registry.tools["engarg_review_contract"]).unwrap(),
                binding
            );
            eal_collector_contracts::RegistryDocument::from_toml(
                &bundle.registry.to_toml().unwrap(),
            )
            .unwrap();
        }
        support_export::reconstruct_bundle(&bundle).unwrap();
    }
    let mut invalid = binding;
    invalid["timeout_seconds"] = json!(0);
    std::fs::write(&binding_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let rejected_output = directory.path().join("rejected.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "export", "--input"])
        .arg(input)
        .args(["--format", "json", "--output"])
        .arg(&rejected_output)
        .arg("--review-binding")
        .arg(binding_path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!rejected_output.exists());
}

#[test]
fn cli_export_and_prepare_write_matching_graph_bundles_with_evidence_plans() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_path_buf();
    #[cfg(unix)]
    let root = {
        let alias = root.join("operator-alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        alias
    };
    let input = root.join("analysis.json");
    saved_graph_report(&input);
    let output = root.join("argument.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "export", "--input"])
        .arg(&input)
        .args(["--format", "json", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle: engarg::support_export::SupportExport =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert!(support_export::reconstruct_bundle(&bundle).is_ok());
    assert_eq!(
        bundle.locations.execution_plan,
        root.join("argument.execution-plan.json")
            .canonicalize()
            .unwrap()
    );
    let plan: EvidenceExecutionPlan =
        serde_json::from_slice(&std::fs::read(&bundle.locations.execution_plan).unwrap()).unwrap();
    assert_eq!(
        canonical_digest(&plan).unwrap(),
        bundle.execution_plan_sha256
    );
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("argument.evidence-plan.json")).unwrap())
            .unwrap();
    assert_eq!(
        evidence["execution_plan_sha256"],
        bundle.execution_plan_sha256
    );
    assert!(!evidence["evidence_needs"].as_array().unwrap().is_empty());
    let tools = std::fs::read_to_string(root.join("argument.tools.toml")).unwrap();
    assert_eq!(
        tools,
        support_export::reconstruct(&bundle).unwrap().tools_toml
    );
    let prepared = root.join("prepared");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "prepare", "--input"])
        .arg(&input)
        .arg("--output-dir")
        .arg(&prepared)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in [
        "argument.bundle.json",
        "argument.eal",
        "argument.tools.toml",
        "argument.execution-plan.json",
        "evidence-plan.json",
        "acquisition-requests.json",
        "review-requests.json",
        "review-inputs.json",
        "context.json",
    ] {
        assert!(prepared.join(name).is_file(), "{name} absent");
    }
    let bundle: engarg::support_export::SupportExport =
        serde_json::from_slice(&std::fs::read(prepared.join("argument.bundle.json")).unwrap())
            .unwrap();
    assert!(support_export::reconstruct_bundle(&bundle).is_ok());
    assert!(bundle.registry.tools.contains_key("engarg_review_contract"));
    assert!(
        bundle.registry.tools["engarg_review_contract"]
            .argv
            .contains(&"review_json".into())
    );
    assert_eq!(
        bundle.locations.execution_plan,
        prepared
            .join("argument.execution-plan.json")
            .canonicalize()
            .unwrap()
    );
}

#[cfg(unix)]
#[test]
fn cli_host_operations_use_the_support_source_canonical_gate_and_full_context() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let bundle = exported().bundle;
    let input = directory.path().join("argument.bundle.json");
    std::fs::write(&input, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let host = directory.path().join("host");
    let capture = directory.path().join("capture.json");
    for (operation, host_operation) in [
        ("plan", "plan"),
        ("assess", "reason"),
        ("collect", "collect_claim"),
    ] {
        let response = json!({"operation":host_operation,"protocol_version":"2025-11-25","is_error":false,"result":{},"content":[]});
        std::fs::write(
            &host,
            format!(
                "#!/bin/sh\ncat > '{}'\nprintf '%s\\n' '{}'\n",
                capture.display(),
                response
            ),
        )
        .unwrap();
        std::fs::set_permissions(&host, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", operation, "--input"])
            .arg(&input)
            .arg("--eal-host")
            .arg(&host)
            .arg("--workspace")
            .arg(directory.path());
        if operation == "assess" {
            command.args(["--now", "2026-10-25T12:00:00Z"]);
        }
        if operation == "collect" {
            let registry = directory.path().join("tools.toml");
            std::fs::write(
                &registry,
                support_export::reconstruct(&bundle).unwrap().tools_toml,
            )
            .unwrap();
            command.arg("--tools").arg(registry);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let captured: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&capture).unwrap()).unwrap();
        assert_eq!(captured["source"], bundle.source);
        if operation != "assess" {
            assert_eq!(captured["claim"], bundle.claim_names["current_claim"]);
        }
        if operation != "plan" {
            assert_eq!(captured["context"], bundle.context);
        }
    }
}

/// This crosses the executable boundary and uses the actual EAL/3 parser/runtime.
#[tokio::test]
#[ignore = "requires a real EAL installation; set ENGARG_EAL_HOST"]
async fn actual_eal_parser_validates_support_source_and_plans_the_canonical_gate() {
    use engarg::eal_host::{HostConfig, invoke};
    use std::time::Duration;
    let workspace = tempfile::tempdir().unwrap();
    let config = HostConfig {
        executable: PathBuf::from(
            std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"),
        ),
        workspace: workspace.path().into(),
        database: Some(workspace.path().join("case.sqlite")),
        registry: None,
        collector_env: Vec::new(),
        public_collector_env: Vec::new(),
        url: None,
        token_env: None,
        timeout: Duration::from_secs(30),
    };
    let artifacts = exported();
    let validation = invoke(
        &config,
        &json!({"operation":"validate","source":artifacts.bundle.source}),
    )
    .await
    .unwrap();
    assert_eq!(validation["is_error"], false, "{validation}");
    assert_eq!(validation["result"]["valid"], true, "{validation}");
    let plan=invoke(&config,&json!({"operation":"plan","source":artifacts.bundle.source,"claim":artifacts.bundle.claim_names["current_claim"]})).await.unwrap();
    assert_eq!(plan["is_error"], false, "{plan}");
    let ids: Vec<_> = plan["result"]["evidence_ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        support_export::collection_budget(&artifacts.bundle).claim_evidence_counts["current_claim"],
        "metadata must count the actual host selected-claim plan"
    );
    for requirement in artifacts.bundle.requirements.iter().filter(|requirement| {
        requirement
            .gated_claim_ids
            .contains(&"current_claim".into())
    }) {
        assert!(
            ids.contains(&requirement.eal_evidence_name.as_str()),
            "{} omitted",
            requirement.requirement_id
        );
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires real EAL binaries including eal-collector; set ENGARG_EAL_HOST"]
async fn actual_legacy_alternative_guard_is_planned_unavailable_support() {
    use engarg::eal_host::{HostConfig, invoke};
    use std::io::Write;
    let directory = tempfile::tempdir().unwrap();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let (request, case, mut elaboration, mut execution, routes, mut locations) = fixture();
    elaboration.alternative_support.clear();
    execution.acquisitions[0].availability =
        eal_collector_contracts::execution::SourceAvailability::Unavailable {
            reason: "This fixture retains no instrument source".into(),
        };
    locations.collector = host.with_file_name("eal-collector");
    locations.execution_plan = directory.path().join("execution.json");
    locations.connections = directory.path().join("connections.json");
    locations.store = directory.path().join("store");
    std::fs::write(&locations.connections,serde_json::to_vec(&json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":[],"local_catalogues":[],"approved_catalogues":[]})).unwrap()).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    std::fs::write(
        &locations.execution_plan,
        serde_json::to_vec(&artifacts.execution_plan).unwrap(),
    )
    .unwrap();
    let tools = directory.path().join("tools.toml");
    std::fs::write(&tools, &artifacts.tools_toml).unwrap();
    let config = HostConfig {
        executable: host,
        workspace: directory.path().into(),
        database: Some(directory.path().join("case.sqlite")),
        registry: Some(tools),
        collector_env: vec![],
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: std::time::Duration::from_secs(30),
    };
    let validation = invoke(
        &config,
        &json!({"operation":"validate","source":artifacts.bundle.source}),
    )
    .await
    .unwrap();
    assert_eq!(validation["result"]["valid"], true, "{validation}");
    let collection=invoke(&config,&json!({"operation":"collect_claim","source":artifacts.bundle.source,"claim":artifacts.bundle.claim_names["current_claim"],"context":artifacts.bundle.context})).await.unwrap();
    assert_eq!(collection["is_error"], false, "{collection}");
    let now = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let assessment=invoke(&config,&json!({"operation":"reason","source":artifacts.bundle.source,"context":artifacts.bundle.context,"collection_id":collection["result"]["collection_id"],"now":now.trim()})).await.unwrap();
    assert_eq!(
        assessment["result"]["claims"][&artifacts.bundle.claim_names["current_claim"]]["status"],
        "unsupported"
    );
    for guard in &artifacts
        .bundle
        .alternative_projection
        .as_ref()
        .unwrap()
        .guards
    {
        let record = &collection["result"]["records"][&guard.evidence_name];
        assert_eq!(record["status"], "error");
        assert!(record.get("value").is_none());
        assert_eq!(
            assessment["result"]["evidence"][&guard.evidence_name]["status"],
            "unavailable"
        );
    }
    let guard = &artifacts
        .bundle
        .alternative_projection
        .as_ref()
        .unwrap()
        .guards[0];
    let request = artifacts
        .acquisition_requests
        .iter()
        .find(|request| request.evidence_id == guard.evidence_name)
        .unwrap();
    let binding = &artifacts.registry.tools["engarg_support_graph"];
    let mut child = std::process::Command::new(&binding.argv[0])
        .args(&binding.argv[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("Legacy support extraction"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("absent or ambiguous"));
}

/// A retained instrument fixture plus separately supplied review fixtures exercise
/// the real export -> prepare -> EAL command collector -> assessment boundary.
#[cfg(unix)]
#[test]
#[ignore = "requires real EAL binaries including eal-collector; set ENGARG_EAL_HOST"]
fn actual_eal_collection_reuses_one_source_and_can_support_the_reviewed_canonical_claim() {
    use sha2::{Digest, Sha256};
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let collector = host.with_file_name("eal-collector");
    let observed_at = String::from_utf8(
        Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let (request, case, mut elaboration, mut execution, mut routes, mut locations) = fixture();
    // Each represented observation still has its own criterion. A separately
    // retained scoped judgement and the authored EAL review obligations remain.
    elaboration.requirements.retain(|requirement| {
        !requirement.id.starts_with("typed_") && requirement.id != "retained_judgement"
    });
    let ids: std::collections::BTreeSet<_> = elaboration
        .requirements
        .iter()
        .map(|requirement| requirement.id.clone())
        .collect();
    elaboration
        .evaluations
        .retain(|evaluation| ids.contains(&evaluation.requirement_id));
    let cone = ArgumentIndex::new(&case).selected_dependency("current_claim");
    let contract = elaboration.acquisitions[0].contract.clone();
    let alternative_gates =
        engarg::support_requirements::alternative_requirement_gates(&elaboration);
    for requirement in &mut elaboration.requirements {
        if !cone.contains(requirement.target_id.as_str())
            && !alternative_gates
                .get(&requirement.id)
                .is_some_and(|claims| claims.contains("current_claim"))
        {
            continue;
        }
        requirement.contract = Some(contract.clone());
        requirement.acceptance = match requirement.id.as_str() {
            "shared_requirement_1" => EvaluationCriterion::Numeric {
                comparison: eal_collector_contracts::execution::NumericComparison::AtMost,
                threshold: json!(1),
                value_unit: "count".into(),
                threshold_unit: "count".into(),
            },
            "shared_requirement_2" => EvaluationCriterion::Numeric {
                comparison: eal_collector_contracts::execution::NumericComparison::AtLeast,
                threshold: json!(0),
                value_unit: "count".into(),
                threshold_unit: "count".into(),
            },
            _ => EvaluationCriterion::Equals { expected: json!(0) },
        };
        requirement.gaps.clear();
        let evaluation = elaboration
            .evaluations
            .iter_mut()
            .find(|evaluation| evaluation.requirement_id == requirement.id)
            .unwrap();
        evaluation.inputs = vec![
            serde_json::from_value(json!({"step_id":"shared_source","pointer":"/value"})).unwrap(),
        ];
        evaluation.criterion = requirement.acceptance.clone();
        evaluation.max_age_seconds = Some(300);
        evaluation.required_coverage =
            eal_collector_contracts::execution::RequiredCoverage::Witness;
    }
    let artifact = directory.path().join("instrument.json");
    let observation = serde_json::to_vec(
        &json!({"measurement":[{"case":"dev","raw":{"measured":0}}],"observed_at":observed_at}),
    )
    .unwrap();
    std::fs::write(&artifact, &observation).unwrap();
    let artifact_sha256 = Sha256::digest(&observation)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let transport = json!({"kind":"retained_artifact","path":artifact,"sha256":artifact_sha256});
    let mapping = json!({"tool":"bounded_source","raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/measurement","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"case":"/0/case"}});
    routes[0].capability.semantics.as_mut().unwrap().quantities[0].output_pointer =
        "/0/raw/measured".into();
    routes[0].capability.output_schema = json!({"type":"array","items":{"type":"object"}});
    routes[0].capability.output_schema_sha256 =
        eal_collector_contracts::capability::schema_identity(&routes[0].capability.output_schema)
            .unwrap();
    execution.acquisitions[0].source.output_schema_sha256 =
        routes[0].capability.output_schema_sha256.clone();
    let configuration_sha256 = canonical_digest(&json!([transport, mapping, [], []])).unwrap();
    routes[0].capability.provider.transport =
        eal_collector_contracts::capability::TransportKind::RetainedArtifact;
    routes[0].capability.provider.configuration_sha256 = configuration_sha256.clone();
    execution.acquisitions[0].source.configuration_sha256 = configuration_sha256;
    execution.acquisitions[0].source.qualified_contract_sha256 =
        routes[0].capability.measurement_identity().unwrap();
    let mut judgement_contract = contract.clone();
    judgement_contract.support_type = SupportType::Judgement;
    judgement_contract.logical_strength =
        eal_collector_contracts::capability::LogicalStrength::Judgement;
    judgement_contract.acceptable_methods = vec!["retained.review/1".into()];
    judgement_contract.quantities = vec![
        serde_json::from_value(json!({"name":"judgement","unit":"retained_assessment"})).unwrap(),
    ];
    let judgement_criterion = EvaluationCriterion::Judgement {
        criterion:
            "The independent fixture review finds the bounded source interpretation adequate".into(),
        scope: Some(judgement_contract.scope.clone()),
    };
    elaboration.requirements.push(SupportRequirement{id:"scoped_judgement".into(),target_id:"W1".into(),evidence_need_ids:vec![],original_acceptance_criteria:vec![],contract:Some(judgement_contract.clone()),discriminating_evidence:vec![],acceptance:judgement_criterion.clone(),acceptance_rationale:"Import the separately retained review; the runtime checks correspondence rather than certifying its truth".into(),gaps:vec![]});
    elaboration.evaluations.push(serde_json::from_value(json!({"id":"evaluation_scoped_judgement","requirement_id":"scoped_judgement","inputs":[{"step_id":"judgement_source","pointer":"/judgement"}],"criterion":judgement_criterion,"max_age_seconds":300})).unwrap());
    elaboration.acquisitions.push(AcquisitionBlueprint {
        id: "judgement_source".into(),
        contract: judgement_contract.clone(),
        arguments: json!({"case":"dev"}),
        bindings: vec![],
        for_each: None,

        operation_spec: None,
    });
    let judgement_record = json!({"schema":"eal.retained-judgement/1","criterion":"The independent fixture review finds the bounded source interpretation adequate","scope":judgement_contract.scope,"reviewer_identity":"independent fixture reviewer","reviewed_at":observed_at,"verdict":"supported","reasons":["The fixture reviewer retained this scoped decision separately from instrument acquisition"],"provenance":{"record_id":"fixture-review-1","source":"independent review fixture"}});
    let judgement_path = directory.path().join("retained-judgement.json");
    let judgement_bytes=serde_json::to_vec(&json!({"measurement":{"case":"dev","judgement":judgement_record},"observed_at":observed_at})).unwrap();
    std::fs::write(&judgement_path, &judgement_bytes).unwrap();
    let judgement_sha = Sha256::digest(&judgement_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let judgement_transport =
        json!({"kind":"retained_artifact","path":judgement_path,"sha256":judgement_sha});
    let judgement_mapping = json!({"tool":"retained_judgement","raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/measurement","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"case":"/case"}});
    let mut judgement_descriptor = descriptor();
    judgement_descriptor.id = "retained_judgement".into();
    judgement_descriptor.provider.transport =
        eal_collector_contracts::capability::TransportKind::RetainedArtifact;
    judgement_descriptor.provider.configuration_sha256 =
        canonical_digest(&json!([judgement_transport, judgement_mapping, [], []])).unwrap();
    let semantics = judgement_descriptor.semantics.as_mut().unwrap();
    semantics.support_types = vec![SupportType::Judgement];
    semantics.methods = vec!["retained.review/1".into()];
    semantics.quantities = vec![
        serde_json::from_value(
            json!({"name":"judgement","unit":"retained_assessment","output_pointer":"/judgement"}),
        )
        .unwrap(),
    ];
    semantics.coverage=vec![serde_json::from_value(json!({"strength":"judgement","population":null,"completeness_pointer":null,"sample_size_pointer":null})).unwrap()];
    let mut judgement_source = execution.acquisitions[0].clone();
    judgement_source.id = "judgement_source".into();
    judgement_source.source.capability_id = judgement_descriptor.id.clone();
    judgement_source.source.output_schema_sha256 =
        judgement_descriptor.output_schema_sha256.clone();
    judgement_source.source.configuration_sha256 =
        judgement_descriptor.provider.configuration_sha256.clone();
    judgement_source.source.qualified_contract_sha256 =
        judgement_descriptor.measurement_identity().unwrap();
    execution.acquisitions.push(judgement_source);
    routes.push(AcquisitionRouteProof {
        composition: None,
        batch: None,
        blueprint_id: "judgement_source".into(),
        capability: judgement_descriptor,
        projection_id: Some("judgement_quantities".into()),
        input_bindings: Default::default(),
        operator_arguments: None,
    });
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    let connections = directory.path().join("connections.json");
    std::fs::write(&connections,serde_json::to_vec(&json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":[{"capability":routes[0].capability,"transport":transport,"mapping":mapping,"pinned_files":[]},{"capability":routes[1].capability,"transport":judgement_transport,"mapping":judgement_mapping,"pinned_files":[]}],"local_catalogues":[],"approved_catalogues":[]})).unwrap()).unwrap();
    locations.collector = collector;
    locations.connections = connections;
    locations.store = directory.path().join("source-store");
    let input = directory.path().join("analysis.json");
    saved_graph_report(&input);
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
    saved["request"] = serde_json::to_value(request).unwrap();
    saved["argument"] = serde_json::to_value(case).unwrap();
    saved["evidence_planning"]["elaboration"] = serde_json::to_value(&elaboration).unwrap();
    saved["evidence_planning"]["routes"]["execution"] = serde_json::to_value(&execution).unwrap();
    saved["evidence_planning"]["routes"]["projections"] = serde_json::to_value(&routes).unwrap();
    for (key, path) in [
        ("collector", &locations.collector),
        ("connections", &locations.connections),
        ("store", &locations.store),
    ] {
        saved["evidence_planning"]["routes"]["configuration"][key] = json!(path);
    }
    std::fs::write(&input, serde_json::to_vec(&saved).unwrap()).unwrap();
    let prepared = directory.path().join("prepared");
    let result = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "prepare", "--input"])
        .arg(&input)
        .arg("--output-dir")
        .arg(&prepared)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle_path = prepared.join("argument.bundle.json");
    let bundle: engarg::support_export::SupportExport =
        serde_json::from_slice(&std::fs::read(&bundle_path).unwrap()).unwrap();
    assert!(
        !bundle
            .pending
            .iter()
            .any(|pending| pending.requirement_id.as_deref() == Some("scoped_judgement")),
        "a configured scoped retained judgement is not intrinsically pending"
    );
    assert!(
        !bundle.authored.acquisition_plan.as_ref().unwrap().enabled,
        "base retains no legacy collector placeholders"
    );
    let requests: Vec<eal_collector_contracts::AcquisitionRequest> =
        serde_json::from_slice(&std::fs::read(prepared.join("review-requests.json")).unwrap())
            .unwrap();
    let reviews:Vec<_>=requests.iter().map(|request|json!({"request":request.acquisition(),"observed_at":observed_at,"value":{"accepted":request.input["obligation"]["kind"]!="challenge_activation","reviewer":"independent contract fixture","reason":"Explicit fixture decision, separate from source acquisition"}})).collect();
    std::fs::write(
        prepared.join("review-inputs.json"),
        serde_json::to_vec(&json!({"schema":"engarg.review-inputs/1","reviews":reviews})).unwrap(),
    )
    .unwrap();
    let invoke = |operation: &str, extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", operation, "--input"])
            .arg(&bundle_path)
            .arg("--eal-host")
            .arg(&host)
            .arg("--workspace")
            .arg(directory.path())
            .arg("--database")
            .arg(directory.path().join("case.sqlite"))
            .arg("--tools")
            .arg(prepared.join("argument.tools.toml"))
            .args(["--timeout-secs", "60"])
            .args(extra);
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()
    };
    assert_eq!(invoke("validate", &[])["result"]["valid"], true);
    let plan = invoke("plan", &[]);
    assert!(
        plan["result"]["evidence_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id
                .as_str()
                .is_some_and(|id| id.starts_with("support_evaluation_")))
    );
    let collection = invoke("collect", &[]);
    let records = collection["result"]["records"].as_object().unwrap();
    let mut common_source = None;
    let mut source_requests = 0;
    let mut judgement_requests = 0;
    let mut original_sources = std::collections::BTreeMap::new();
    for requirement in bundle.requirements.iter().filter(|requirement| {
        requirement
            .gated_claim_ids
            .contains(&"current_claim".into())
    }) {
        let record = &records[&requirement.eal_evidence_name];
        if record["status"] != "ok" {
            use std::io::Write;
            let request = support_export::reconstruct(&bundle)
                .unwrap()
                .acquisition_requests
                .into_iter()
                .find(|request| request.evidence_id == requirement.eal_evidence_name)
                .unwrap();
            let binding = &bundle.registry.tools["engarg_support_graph"];
            let mut command = Command::new(&binding.argv[0])
                .args(&binding.argv[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            command
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&request).unwrap())
                .unwrap();
            let direct = command.wait_with_output().unwrap();
            panic!(
                "normal EAL collection failed: {record}; direct collector diagnostic: {}",
                String::from_utf8_lossy(&direct.stderr)
            );
        }
        assert_eq!(record["status"], "ok", "{record}");
        assert_eq!(record["collected_at"], observed_at);
        assert_eq!(record["value"]["criterion_satisfied"], true);
        let source_ids = record["value"]["source_ids"].clone();
        original_sources.insert(requirement.requirement_id.clone(), source_ids.clone());
        if requirement.requirement_id == "scoped_judgement" {
            judgement_requests += record["details"]["metrics"]["source_requests"]
                .as_u64()
                .unwrap();
            continue;
        }
        if let Some(expected) = &common_source {
            assert_eq!(&source_ids, expected);
        } else {
            common_source = Some(source_ids);
        }
        source_requests += record["details"]["metrics"]["source_requests"]
            .as_u64()
            .unwrap();
    }
    assert_eq!(
        source_requests, 1,
        "the normal EAL command path acquires the shared source exactly once"
    );
    assert_eq!(
        judgement_requests, 1,
        "the retained judgement is a separate qualified source"
    );
    assert_eq!(
        std::fs::read_dir(&locations.store)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json"))
            .count(),
        2
    );
    let collection_id = collection["result"]["collection_id"].as_str().unwrap();
    let assessment = invoke(
        "assess",
        &["--now", &observed_at, "--collection-id", collection_id],
    );
    assert_eq!(
        assessment["result"]["claims"][&bundle.claim_names["current_claim"]]["status"], "supported",
        "{assessment}"
    );
    let reused = invoke("collect", &[]);
    for requirement in bundle.requirements.iter().filter(|requirement| {
        requirement
            .gated_claim_ids
            .contains(&"current_claim".into())
    }) {
        let record = &reused["result"]["records"][&requirement.eal_evidence_name];
        assert_eq!(
            record["collected_at"], observed_at,
            "reuse must preserve original age"
        );
        assert_eq!(
            record["value"]["source_ids"],
            original_sources[&requirement.requirement_id]
        );
        assert_eq!(record["details"]["metrics"]["source_requests"], 0);
    }
    // A changed retained timestamp invalidates the pinned artifact rather than
    // refreshing or silently reusing its previous observation.
    let mut changed: serde_json::Value = serde_json::from_slice(&observation).unwrap();
    changed["observed_at"] = json!("2000-01-01T00:00:00Z");
    std::fs::write(&artifact, serde_json::to_vec(&changed).unwrap()).unwrap();
    let rejected = invoke("collect", &[]);
    for requirement in bundle.requirements.iter().filter(|requirement| {
        requirement
            .gated_claim_ids
            .contains(&"current_claim".into())
    }) {
        assert_eq!(
            rejected["result"]["records"][&requirement.eal_evidence_name]["status"],
            if requirement.requirement_id == "scoped_judgement" {
                "ok"
            } else {
                "error"
            },
            "a changed instrument cannot refresh an observation or invalidate the separate retained review"
        );
    }
}

#[cfg(unix)]
struct DetailFixtureServer {
    url: String,
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
#[cfg(unix)]
impl DetailFixtureServer {
    fn start(observed_at: String) -> Self {
        use std::{
            io::{Read, Write},
            sync::{
                Arc, Mutex,
                atomic::{AtomicBool, Ordering},
            },
            time::Duration,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/detail", listener.local_addr().unwrap());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen = calls.clone();
        let stopping = stop.clone();
        let worker = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 1024];
                while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                    assert!(bytes.len() < 16384);
                }
                let request = String::from_utf8(bytes).unwrap();
                let target = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let id = target
                    .split_once('?')
                    .unwrap()
                    .1
                    .split('&')
                    .find_map(|item| item.strip_prefix("trace_id="))
                    .unwrap()
                    .to_owned();
                seen.lock().unwrap().push(id.clone());
                let body=serde_json::to_vec(&json!({"measurement":{"case":"dev","data":{"matched":id=="hit"}},"observed_at":observed_at})).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            url,
            calls,
            stop,
            worker: Some(worker),
        }
    }
}
#[cfg(unix)]
impl Drop for DetailFixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !std::thread::panicking() {
                result.expect("detail fixture server panicked");
            }
        }
    }
}

/// The complete logical search -> physical detail -> Project -> AnyTrue path.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires real EAL binaries including eal-collector; set ENGARG_EAL_HOST"]
async fn actual_eal_search_detail_fanout_stops_at_a_nested_qualified_witness() {
    use engarg::eal_host::{HostConfig, invoke};
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let observed_at = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let server = DetailFixtureServer::start(observed_at.clone());
    let (request, case, elaboration, mut execution, mut routes, mut locations) =
        search_detail_fixture();
    let search_path = directory.path().join("trace-search.json");
    let search_bytes=serde_json::to_vec(&json!({"measurement":{"case":"dev","results":{"trace_ids":["miss","hit","unused"]}},"observed_at":observed_at})).unwrap();
    std::fs::write(&search_path, &search_bytes).unwrap();
    let search_sha = Sha256::digest(&search_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let transports = [
        json!({"kind":"retained_artifact","path":search_path,"sha256":search_sha}),
        json!({"kind":"http_get","url":server.url,"query_bindings":{"trace_id":"/query/trace_id"},"token_env":null}),
    ];
    let mut providers = Vec::new();
    for (index, transport) in transports.iter().enumerate() {
        let mapping = json!({"tool":routes[index].capability.id,"raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/measurement","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"case":"/case"}});
        let identity = canonical_digest(&json!([transport, mapping, [], []])).unwrap();
        routes[index].capability.provider.transport = if index == 0 {
            eal_collector_contracts::capability::TransportKind::RetainedArtifact
        } else {
            eal_collector_contracts::capability::TransportKind::Http
        };
        routes[index].capability.provider.configuration_sha256 = identity.clone();
        execution.acquisitions[index].source.configuration_sha256 = identity;
        execution.acquisitions[index]
            .source
            .qualified_contract_sha256 = routes[index].capability.measurement_identity().unwrap();
        providers.push(json!({"capability":routes[index].capability,"transport":transport,"mapping":mapping,"pinned_files":[],"credential_env":[]}));
    }
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    locations.collector = host.with_file_name("eal-collector");
    locations.connections = directory.path().join("connections.json");
    locations.execution_plan = directory.path().join("execution.json");
    locations.store = directory.path().join("source-store");
    std::fs::write(&locations.connections,serde_json::to_vec(&json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":providers,"local_catalogues":[],"approved_catalogues":[]})).unwrap()).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    std::fs::write(
        &locations.execution_plan,
        serde_json::to_vec_pretty(&artifacts.execution_plan).unwrap(),
    )
    .unwrap();
    let registry = directory.path().join("tools.toml");
    std::fs::write(&registry, &artifacts.tools_toml).unwrap();
    let config = HostConfig {
        executable: host,
        workspace: directory.path().into(),
        database: Some(directory.path().join("case.sqlite")),
        registry: Some(registry),
        collector_env: vec![],
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: std::time::Duration::from_secs(60),
    };
    let result=invoke(&config,&json!({"operation":"collect_claim","source":artifacts.bundle.source,"claim":artifacts.bundle.claim_names["current_claim"],"context":artifacts.bundle.context})).await.unwrap();
    assert_eq!(result["is_error"], false, "{result}");
    let mut shared_ids = None;
    let mut source_requests = 0;
    for projection in artifacts
        .bundle
        .requirements
        .iter()
        .filter(|projection| projection.requirement_id.starts_with("shared_requirement"))
    {
        let record = &result["result"]["records"][&projection.eal_evidence_name];
        assert_eq!(record["status"], "ok", "{record}");
        assert_eq!(
            record["value"]["criterion_satisfied"], true,
            "the Project normalisation must provide the logical match array to AnyTrue"
        );
        assert_eq!(record["collected_at"], observed_at);
        let source_ids = record["value"]["source_ids"].clone();
        if let Some(expected) = &shared_ids {
            assert_eq!(&source_ids, expected);
        } else {
            shared_ids = Some(source_ids);
        }
        source_requests += record["details"]["metrics"]["source_requests"]
            .as_u64()
            .unwrap();
    }
    assert_eq!(
        *server.calls.lock().unwrap(),
        ["miss", "hit"],
        "the physical nested witness must stop before the unused trace"
    );
    assert_eq!(
        source_requests, 3,
        "one retained search and two detail reads serve all three requirement evaluations"
    );
}

#[test]
fn collection_budget_reports_all_emitted_evidence_without_truncating_shared_requirements() {
    let (request, case, mut elaboration, mut execution, routes, locations) = fixture();
    let baseline = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    let baseline_count = support_export::collection_budget(&baseline.bundle).total_emitted_evidence;
    let template = elaboration
        .requirements
        .iter()
        .find(|requirement| requirement.id == "shared_requirement_0")
        .unwrap()
        .clone();
    let evaluation = elaboration
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == template.id)
        .unwrap()
        .clone();
    for index in 0..(129 - baseline_count) {
        let mut requirement = template.clone();
        requirement.id = format!("additional_requirement_{index}");
        let mut evaluation = evaluation.clone();
        evaluation.id = format!("additional_evaluation_{index}");
        evaluation.requirement_id = requirement.id.clone();
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
    }
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    let budget = support_export::collection_budget(&artifacts.bundle);
    assert_eq!(budget.total_emitted_evidence, 129);
    assert_eq!(budget.assumed_default_limit, 128);
    assert_eq!(
        artifacts
            .bundle
            .source
            .lines()
            .filter(|line| line.starts_with("evidence "))
            .count(),
        129
    );
    assert_eq!(
        artifacts.bundle.requirements.len(),
        elaboration.requirements.len()
    );
    assert_eq!(
        artifacts.execution_plan.acquisitions.len(),
        1,
        "sharing remains intact"
    );
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "collection_budget_exceeded"
                && pending.description.contains("Default collect-all"))
    );
    support_export::reconstruct(&artifacts.bundle).unwrap();
}
