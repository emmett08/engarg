//! The support graph supplies native method inputs and retains unresolved conditions.
use eal_collector_contracts::execution::EvaluationCriterion;
use eal_core::{ExecutionLimits, composition, methods::default_registry, validation};
use engarg::{
    export_policy::ExportPolicy,
    support_export::{self, SupportExportArtifacts},
    support_requirements::{RequirementGap, RequirementGapKind},
};
use serde_json::json;

#[path = "common/reasoning.rs"]
#[allow(dead_code)]
mod reasoning;
#[path = "common/reasoning_graph.rs"]
#[allow(dead_code)]
mod reasoning_graph;

fn exported(mode: &reasoning::ModeCase) -> SupportExportArtifacts {
    let (request, case, elaboration, plan, routes, locations) =
        reasoning_graph::graph_fixture(mode);
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

fn compile(artifacts: &SupportExportArtifacts) {
    let program = composition::compile(
        &artifacts.bundle.source,
        None,
        "",
        ExecutionLimits::default(),
    )
    .unwrap();
    let diagnostics = validation::validate(&program, default_registry());
    assert!(
        diagnostics.is_empty(),
        "{diagnostics:?}\n{}",
        artifacts.bundle.source
    );
}

#[test]
fn every_native_mode_binds_the_exact_retained_graph_input_and_independent_evidence() {
    for mode in reasoning::cases() {
        let artifacts = exported(&mode);
        compile(&artifacts);
        assert_eq!(artifacts.bundle.reasoning_inputs.len(), 1);
        let projection = &artifacts.bundle.reasoning_inputs[0];
        assert_eq!(projection.method, format!("{}/1", mode.kind));
        assert_eq!(projection.requirement_id.as_deref(), Some("requirement_W1"));
        assert_eq!(projection.output.as_ref().unwrap().pointer, "/typed_input");
        let request = artifacts
            .acquisition_requests
            .iter()
            .find(|request| request.evidence_id == projection.eal_evidence_name)
            .unwrap();
        assert_eq!(request.input["schema"], "engarg.reasoning-input/1");
        assert_eq!(
            request.input["execution_plan_sha256"],
            artifacts.bundle.execution_plan_sha256
        );
        assert_eq!(
            request.input["output"],
            serde_json::to_value(projection.output.as_ref().unwrap()).unwrap()
        );
        assert!(
            request.input["premise_bindings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|binding| binding["requirement_id"] == "requirement_G1")
        );
        assert!(
            artifacts
                .registry
                .tools
                .contains_key("engarg_reasoning_input")
        );
        assert!(
            artifacts
                .review_requests
                .iter()
                .all(|request| request.tool == "engarg_review_contract")
        );
        assert!(
            artifacts
                .bundle
                .source
                .contains("native_method_argument_5731")
        );
        assert!(
            artifacts
                .bundle
                .source
                .contains("native_ground_argument_4731")
        );
        assert!(
            artifacts
                .bundle
                .source
                .contains("native_composition_argument_63757272656e745f636c61696d")
        );
        let program = composition::compile(
            &artifacts.bundle.source,
            None,
            "",
            ExecutionLimits::default(),
        )
        .unwrap();
        let native = &program.arguments["native_method_argument_5731"];
        assert!(
            native.premises.contains(&"ground_4731".to_owned()),
            "Ground-targeted objections retain their original premise identity"
        );
        assert!(
            native
                .evidence
                .contains(&"support_evaluation_726571756972656d656e745f4731".to_owned())
        );
        assert!(
            native
                .evidence
                .contains(&"support_evaluation_726571756972656d656e745f4131".to_owned())
        );
        support_export::reconstruct(&artifacts.bundle).unwrap();
        let budget = support_export::collection_budget(&artifacts.bundle);
        assert_eq!(
            budget.total_emitted_evidence,
            artifacts.bundle.authored.obligations.len() + artifacts.bundle.requirements.len() + 1
        );
    }
}

#[test]
fn unavailable_formal_model_retains_a_pending_selector_and_no_native_bypass() {
    let mode = reasoning::cases().remove(4);
    let (request, case, mut elaboration, mut plan, mut routes, locations) =
        reasoning_graph::graph_fixture(&mode);
    let requirement = elaboration
        .requirements
        .iter_mut()
        .find(|requirement| requirement.id == "requirement_W1")
        .unwrap();
    requirement.contract = None;
    requirement.acceptance = EvaluationCriterion::Unavailable {
        reason: "The identified structural model is unavailable.".into(),
    };
    requirement.gaps = vec![RequirementGap {
        kind: RequirementGapKind::SourceUnavailable,
        description: "The structural model has not been supplied.".into(),
    }];
    let evaluation = elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .unwrap();
    evaluation.inputs.clear();
    evaluation.criterion = requirement.acceptance.clone();
    evaluation.max_age_seconds = None;
    elaboration
        .acquisitions
        .retain(|source| source.id != "formal_source");
    plan.acquisitions
        .retain(|source| source.id != "formal_source");
    routes.retain(|route| route.blueprint_id != "formal_source");
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    compile(&artifacts);
    assert!(artifacts.bundle.reasoning_inputs[0].output.is_none());
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "reasoning_method_input")
    );
    assert!(
        artifacts
            .bundle
            .source
            .contains("engarg.reasoning-input-pending/1")
    );
    assert!(
        !artifacts
            .bundle
            .source
            .contains("argument native_method_argument_5731")
    );
    support_export::reconstruct(&artifacts.bundle).unwrap();
}

#[test]
fn formal_input_admission_cannot_stand_in_for_independent_premise_truth() {
    let mode = reasoning::cases().remove(0);
    let (request, case, mut elaboration, mut plan, routes, locations) =
        reasoning_graph::graph_fixture(&mode);
    let requirement = elaboration
        .requirements
        .iter_mut()
        .find(|requirement| requirement.id == "requirement_G1")
        .unwrap();
    requirement.acceptance = EvaluationCriterion::MethodInput {
        method: "deductive/1".into(),
    };
    let evaluation = elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .unwrap();
    evaluation.criterion = requirement.acceptance.clone();
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let error = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("method_input") || error.contains("formal input"),
        "{error}"
    );
}

#[test]
fn input_selector_and_projection_metadata_are_checked_during_reconstruction() {
    let artifacts = exported(&reasoning::cases().remove(0));
    let mut changed = artifacts.bundle.clone();
    changed.reasoning_inputs[0].output.as_mut().unwrap().pointer = "/unrelated".into();
    assert!(support_export::reconstruct(&changed).is_err());
    let mut changed = artifacts.bundle.clone();
    changed.source = changed
        .source
        .replace("engarg.reasoning-input/1", "engarg.reasoning-input/2");
    assert!(support_export::reconstruct(&changed).is_err());
}

#[test]
fn equal_numbers_with_a_different_declared_input_unit_remain_unresolved() {
    let mode = reasoning::cases().remove(3);
    let (request, case, mut elaboration, mut plan, mut routes, locations) =
        reasoning_graph::graph_fixture(&mode);
    let source = elaboration
        .acquisitions
        .iter_mut()
        .find(|source| source.id == "premise_source")
        .unwrap();
    source.contract.quantities[0].unit = "m".into();
    let changed = source.contract.clone();
    elaboration
        .requirements
        .iter_mut()
        .find(|required| required.id == "requirement_G1")
        .unwrap()
        .contract = Some(changed.clone());
    let route = routes
        .iter_mut()
        .find(|route| route.blueprint_id == "premise_source")
        .unwrap();
    route.capability = reasoning_graph::capability("premise_source", &changed);
    plan.acquisitions
        .iter_mut()
        .find(|source| source.id == "premise_source")
        .unwrap()
        .source = reasoning_graph::source(&route.capability);
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    compile(&artifacts);
    assert!(artifacts.bundle.reasoning_inputs[0].output.is_none());
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "reasoning_method_input"
                && pending.description.contains("unit"))
    );
    assert!(
        !artifacts
            .bundle
            .source
            .contains("argument native_method_argument_5731")
    );
}

fn replace_source_scope(
    fixture: &mut reasoning_graph::GraphFixture,
    source_id: &str,
    requirement_id: &str,
    selectors: serde_json::Value,
) {
    let (_, _, elaboration, plan, routes, _) = fixture;
    let source = elaboration
        .acquisitions
        .iter_mut()
        .find(|source| source.id == source_id)
        .unwrap();
    source.contract.scope.selectors = serde_json::from_value(selectors.clone()).unwrap();
    source.contract.scope.identity_dimensions =
        source.contract.scope.selectors.keys().cloned().collect();
    source.arguments = selectors.clone();
    let contract = source.contract.clone();
    elaboration
        .requirements
        .iter_mut()
        .find(|required| required.id == requirement_id)
        .unwrap()
        .contract = Some(contract.clone());
    let route = routes
        .iter_mut()
        .find(|route| route.blueprint_id == source_id)
        .unwrap();
    route.capability = reasoning_graph::capability(source_id, &contract);
    let source = plan
        .acquisitions
        .iter_mut()
        .find(|source| source.id == source_id)
        .unwrap();
    source.source = reasoning_graph::source(&route.capability);
    source.source.scope = selectors.clone();
    source.arguments = selectors;
}

#[test]
fn development_evidence_cannot_be_relabelled_as_a_production_method_scope() {
    let mode = reasoning::cases().remove(1);
    for (source_id, requirement_id) in [
        ("formal_source", "requirement_W1"),
        ("premise_source", "requirement_G1"),
    ] {
        let mut fixture = reasoning_graph::graph_fixture(&mode);
        replace_source_scope(
            &mut fixture,
            "formal_source",
            "requirement_W1",
            json!({"fixture":"production"}),
        );
        replace_source_scope(
            &mut fixture,
            "premise_source",
            "requirement_G1",
            json!({"fixture":"production"}),
        );
        replace_source_scope(
            &mut fixture,
            source_id,
            requirement_id,
            json!({"fixture":"development"}),
        );
        let (request, mut case, elaboration, mut plan, routes, locations) = fixture;
        let engarg::domain::ReasoningMethodBinding::Bound(bound) =
            case.warrants[0].reasoning_method.as_mut().unwrap()
        else {
            panic!("expected bound test method");
        };
        bound.conclusion.scope.selectors[0].value =
            engarg::domain::MethodScopeScalar::String("production".into());
        support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
        let artifacts = support_export::export(
            &request,
            &case,
            &elaboration,
            &plan,
            &routes,
            &locations,
            &ExportPolicy::default(),
        )
        .unwrap();
        compile(&artifacts);
        assert!(artifacts.bundle.reasoning_inputs[0].output.is_none());
        assert!(artifacts.bundle.pending.iter().any(|pending| {
            pending.kind == "scope_transfer_required"
                && pending.description.contains(requirement_id)
        }));
        assert!(
            !artifacts
                .bundle
                .source
                .contains("argument native_method_argument_5731")
        );
        support_export::reconstruct(&artifacts.bundle).unwrap();
    }
}

#[test]
fn sample_identifiers_can_narrow_evidence_within_the_same_target_population() {
    let mode = reasoning::cases().remove(1);
    let mut fixture = reasoning_graph::graph_fixture(&mode);
    for (source_id, requirement_id) in [
        ("formal_source", "requirement_W1"),
        ("premise_source", "requirement_G1"),
    ] {
        replace_source_scope(
            &mut fixture,
            source_id,
            requirement_id,
            json!({"fixture":"frozen-fixture-v1","sample_id":"retained-sample-17"}),
        );
    }
    let (request, case, elaboration, mut plan, routes, locations) = fixture;
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    compile(&artifacts);
    assert!(artifacts.bundle.reasoning_inputs[0].output.is_some());
    assert!(
        !artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "scope_transfer_required")
    );
    assert!(
        artifacts
            .bundle
            .source
            .contains("argument native_method_argument_5731")
    );
    let engarg::domain::ReasoningMethodBinding::Bound(bound) =
        case.warrants[0].reasoning_method.as_ref().unwrap()
    else {
        panic!("expected bound test method");
    };
    let scope_identity = bound.conclusion.scope.identity().unwrap();
    assert!(
        artifacts
            .bundle
            .source
            .contains(&format!("scope \"{scope_identity}\""))
    );
    support_export::reconstruct(&artifacts.bundle).unwrap();
}

fn with_challenge(target: &str) -> SupportExportArtifacts {
    let mode = reasoning::cases().remove(0);
    let (request, mut case, mut elaboration, mut plan, routes, locations) =
        reasoning_graph::graph_fixture(&mode);
    let mut need = case
        .evidence_plan
        .iter()
        .find(|need| need.id == "N6")
        .unwrap()
        .clone();
    need.id = "N8".into();
    need.target_id = "D1".into();
    case.evidence_plan.push(need.clone());
    case.defeaters.push(serde_json::from_value(json!({"id":"D1","kind":"undermining","target_id":target,"challenge":"The retained fixture may not correspond to the declared condition.","status":"open","material":true,"basis":"hypothetical","response":"Retain this dependency until the independent challenge is resolved.","evidence_ids":[]})).unwrap());
    let mut requirement = elaboration
        .requirements
        .iter()
        .find(|requirement| requirement.id == "requirement_A1")
        .unwrap()
        .clone();
    requirement.id = "requirement_D1".into();
    requirement.target_id = "D1".into();
    requirement.evidence_need_ids = vec![need.id];
    requirement.original_acceptance_criteria = vec![need.acceptance_criterion];
    let mut evaluation = elaboration
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == "requirement_A1")
        .unwrap()
        .clone();
    evaluation.id = "evaluation_requirement_D1".into();
    evaluation.requirement_id = requirement.id.clone();
    elaboration.requirements.push(requirement);
    elaboration.evaluations.push(evaluation);
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
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
fn ground_and_warrant_material_challenges_keep_their_independent_resolution_gate() {
    for target in ["G1", "W1"] {
        let artifacts = with_challenge(target);
        compile(&artifacts);
        let program = composition::compile(
            &artifacts.bundle.source,
            None,
            "",
            ExecutionLimits::default(),
        )
        .unwrap();
        assert!(
            program.arguments["native_method_argument_5731"]
                .evidence
                .contains(&"resolution_review_4431".to_owned())
        );
        if target == "G1" {
            assert!(
                program.arguments["native_ground_argument_4731"]
                    .evidence
                    .contains(&"resolution_review_4431".to_owned())
            );
        }
    }
}

#[test]
fn challenged_applicability_assumption_stays_explicitly_unresolved() {
    let artifacts = with_challenge("A1");
    compile(&artifacts);
    assert!(
        !artifacts
            .bundle
            .source
            .contains("argument native_method_argument_5731")
    );
    assert!(
        artifacts
            .bundle
            .pending
            .iter()
            .any(|pending| pending.kind == "reasoning_evidence_requirement"
                && pending.description.contains("assumption"))
    );
}

/// Exercise the generated EngArg source and selectors through the actual EAL
/// collector, source store, process supervisor and MCP host. These are retained
/// deterministic test-bench observations, not measurements of a deployment.
#[cfg(unix)]
async fn collect_native_graph(
    mode: &reasoning::ModeCase,
    changed_premise: Option<serde_json::Value>,
    unavailable_input: bool,
) -> serde_json::Value {
    use eal_collector_contracts::{
        capability::TransportKind,
        execution::{SourceAvailability, canonical_digest},
    };
    use engarg::eal_host::{HostConfig, invoke};
    use sha2::{Digest, Sha256};
    use std::{path::PathBuf, process::Command};
    let host = PathBuf::from(
        std::env::var_os("ENGARG_EAL_HOST")
            .expect("set ENGARG_EAL_HOST to the actual pinned runtime"),
    );
    let directory = tempfile::tempdir().unwrap();
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
    let (request, case, mut elaboration, mut plan, mut routes, mut locations) =
        reasoning_graph::graph_fixture(mode);
    let premise = changed_premise.unwrap_or_else(|| mode.payload.clone());
    // The independent source's accepted value may legitimately differ from the
    // computation input. Its own criterion is satisfied in the mismatch test,
    // so only the checked value correspondence can prevent input substitution.
    let expected = EvaluationCriterion::Equals {
        expected: premise.clone(),
    };
    elaboration
        .requirements
        .iter_mut()
        .find(|required| required.id == "requirement_G1")
        .unwrap()
        .acceptance = expected.clone();
    elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == "requirement_G1")
        .unwrap()
        .criterion = expected;
    let mut providers = Vec::new();
    for route in &mut routes {
        let value = match route.blueprint_id.as_str() {
            "formal_source" => reasoning::envelope(mode),
            "premise_source" => premise.clone(),
            "applicability_source" => json!(true),
            _ => unreachable!(),
        };
        let path = directory
            .path()
            .join(format!("{}.json", route.blueprint_id));
        let bytes=serde_json::to_vec(&json!({"measurement":{"fixture":"frozen-fixture-v1","value":value},"observed_at":observed_at})).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let transport = json!({"kind":"retained_artifact","path":path,"sha256":digest});
        let mapping = json!({"tool":route.capability.id,"raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/measurement","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"fixture":"/fixture"}});
        route.capability.provider.transport = TransportKind::RetainedArtifact;
        route.capability.provider.configuration_sha256 =
            canonical_digest(&json!([transport, mapping, [], []])).unwrap();
        plan.acquisitions
            .iter_mut()
            .find(|source| source.id == route.blueprint_id)
            .unwrap()
            .source = reasoning_graph::source(&route.capability);
        providers.push(json!({"capability":route.capability,"transport":transport,"mapping":mapping,"pinned_files":[],"credential_env":[]}));
    }
    if unavailable_input {
        plan.acquisitions
            .iter_mut()
            .find(|source| source.id == "formal_source")
            .unwrap()
            .availability = SourceAvailability::Unavailable {
            reason: "The identified formal model record is unavailable.".into(),
        };
    }
    support_export::project_execution_graph(&elaboration, &routes, &mut plan).unwrap();
    locations.collector = host.with_file_name("eal-collector");
    locations.connections = directory.path().join("connections.json");
    locations.execution_plan = directory.path().join("execution.json");
    locations.store = directory.path().join("sources");
    std::fs::write(&locations.connections,serde_json::to_vec(&json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":providers,"local_catalogues":[],"approved_catalogues":[]})).unwrap()).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &plan,
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
        timeout: std::time::Duration::from_secs(60),
    };
    let collection=invoke(&config,&json!({"operation":"collect_claim","source":artifacts.bundle.source,"claim":artifacts.bundle.claim_names["current_claim"],"context":artifacts.bundle.context})).await.unwrap();
    assert_eq!(collection["is_error"], false, "{collection}");
    let assessment=invoke(&config,&json!({"operation":"reason","source":artifacts.bundle.source,"context":artifacts.bundle.context,"now":observed_at,"collection_id":collection["result"]["collection_id"]})).await.unwrap();
    assert_eq!(assessment["is_error"], false, "{assessment}");
    json!({"collection":collection["result"],"assessment":assessment["result"],"claim":artifacts.bundle.claim_names["current_claim"],"input_evidence":artifacts.bundle.reasoning_inputs[0].eal_evidence_name,"observed_at":observed_at})
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the actual pinned EAL host/collector installation; set ENGARG_EAL_HOST"]
async fn actual_exported_graph_acquires_and_computes_all_five_native_modes() {
    for mode in reasoning::cases() {
        let result = collect_native_graph(&mode, None, false).await;
        let claim = result["claim"].as_str().unwrap();
        let input = result["input_evidence"].as_str().unwrap();
        assert_eq!(
            result["collection"]["records"][input]["status"], "ok",
            "{}: {result}",
            mode.kind
        );
        assert_eq!(
            result["collection"]["records"][input]["collected_at"], result["observed_at"],
            "Original source observation time must survive method-input projection"
        );
        assert_eq!(
            result["assessment"]["valid"], true,
            "{}: {result}",
            mode.kind
        );
        assert_eq!(
            result["assessment"]["complete"], true,
            "{}: {result}",
            mode.kind
        );
        assert_eq!(
            result["assessment"]["claims"][claim]["status"], "supported",
            "{}: {result}",
            mode.kind
        );
        let computed =
            &result["assessment"]["arguments"]["native_method_argument_5731"]["reasoning_result"];
        assert_eq!(computed["method"], format!("{}/1", mode.kind));
        assert_eq!(computed["status"], "supported", "{}: {computed}", mode.kind);
        assert_eq!(
            computed["binding"]["status"], "supported",
            "{}: {computed}",
            mode.kind
        );
        match mode.kind {
            "deductive" => assert_eq!(computed["details"]["entailed"], true),
            "inductive" => assert!(
                (computed["details"]["lower"].as_f64().unwrap() - 0.8256343385).abs() < 1e-8
            ),
            "abductive" => assert!(
                (computed["details"]["posterior"]["A"].as_f64().unwrap() - 0.9).abs() < 1e-12
            ),
            "causal" => assert_eq!(computed["details"]["estimate"].as_f64(), Some(6.0)),
            "counterfactual" => assert_eq!(computed["details"]["difference"].as_f64(), Some(6.0)),
            _ => unreachable!(),
        }
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the actual pinned EAL host/collector installation; set ENGARG_EAL_HOST"]
async fn actual_exported_graph_rejects_substituted_premise_values_and_unavailable_models() {
    let inductive = reasoning::cases().remove(1);
    let mut changed = inductive.payload.clone();
    changed["successes"] = json!(1);
    let mismatch = collect_native_graph(&inductive, Some(changed), false).await;
    assert_eq!(
        mismatch["collection"]["records"]["support_evaluation_726571756972656d656e745f4731"]["value"]
            ["criterion_satisfied"],
        true,
        "The independent source criterion must genuinely pass in this substitution regression: {mismatch}"
    );
    let input = mismatch["input_evidence"].as_str().unwrap();
    assert_eq!(
        mismatch["collection"]["records"][input]["status"], "error",
        "{mismatch}"
    );
    assert_eq!(
        mismatch["assessment"]["claims"][mismatch["claim"].as_str().unwrap()]["status"],
        "unsupported",
        "{mismatch}"
    );
    assert_eq!(
        mismatch["assessment"]["arguments"]["native_method_argument_5731"]["reasoning_result"]["status"],
        "not_run",
        "{mismatch}"
    );
    let counterfactual = reasoning::cases().remove(4);
    let missing = collect_native_graph(&counterfactual, None, true).await;
    assert_eq!(
        missing["collection"]["records"][missing["input_evidence"].as_str().unwrap()]["status"],
        "error",
        "{missing}"
    );
    assert_eq!(
        missing["assessment"]["claims"][missing["claim"].as_str().unwrap()]["status"],
        "unsupported",
        "{missing}"
    );
    assert_eq!(
        missing["assessment"]["arguments"]["native_method_argument_5731"]["reasoning_result"]["status"],
        "not_run",
        "{missing}"
    );
}
