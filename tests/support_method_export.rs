//! Direct compiler admission preserves formal scope and challenge targets.
use eal_core::{ExecutionLimits, composition, methods::default_registry, validation};
use engarg::{export_policy::ExportPolicy, support_export};
use std::collections::BTreeMap;
#[path = "common/reasoning_graph.rs"]
mod graph;
#[allow(dead_code)] // Shared observation builders are exercised by reasoning_methods.
#[path = "common/reasoning.rs"]
mod reasoning;

fn versions(
    elaboration: &engarg::support_requirements::SupportElaboration,
) -> BTreeMap<String, String> {
    elaboration
        .acquisitions
        .iter()
        .map(|source| (source.id.clone(), "1".into()))
        .collect()
}

#[test]
fn all_native_modes_compile_without_a_formal_input_acquisition() {
    for mode in reasoning::cases() {
        let (request, case, elaboration, _, _) = graph::graph_fixture(&mode);
        let output = support_export::compile_direct(
            &request,
            &case,
            &elaboration,
            &versions(&elaboration),
            &ExportPolicy::default(),
        )
        .unwrap();
        assert!(
            !output.bundle.source.contains("engarg_reasoning_input"),
            "{}",
            output.bundle.source
        );
        assert!(!output.bundle.source.contains("formal_source"));
        assert!(!output.bundle.source.contains("engarg_support_graph"));
        let program =
            composition::compile(&output.bundle.source, None, "", ExecutionLimits::default())
                .unwrap();
        let diagnostics = validation::validate(&program, default_registry());
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:?}\n{}",
            mode.kind,
            output.bundle.source
        );
        assert_eq!(
            program
                .arguments
                .values()
                .filter(|argument| program.reasoning[&argument.reasoning].method
                    == format!("{}/1", mode.kind))
                .count(),
            1
        );
    }
}

#[test]
fn declared_source_scope_cannot_be_relabelled_by_the_method_conclusion() {
    let mode = reasoning::cases().remove(0);
    let (request, mut case, elaboration, _, _) = graph::graph_fixture(&mode);
    let engarg::domain::ReasoningMethodBinding::Bound(bound) =
        case.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.conclusion.scope.selectors[0].value =
        serde_json::from_value(serde_json::json!("different-population")).unwrap();
    let error = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions(&elaboration),
        &ExportPolicy::default(),
    )
    .err()
    .expect("scope conflict must fail");
    assert!(error.to_string().contains("scope"), "{error}");
}

#[test]
fn typed_premises_keep_one_shared_source_and_explicit_input_bindings() {
    let mode = reasoning::cases().remove(1);
    let (request, case, elaboration, _, _) = graph::graph_fixture(&mode);
    let output = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions(&elaboration),
        &ExportPolicy::default(),
    )
    .unwrap();
    let program =
        composition::compile(&output.bundle.source, None, "", ExecutionLimits::default()).unwrap();
    let kind = program
        .evidence
        .values()
        .find(|e| e.kind == "sample")
        .unwrap();
    assert_eq!(kind.bindings.len(), 2);
    assert_eq!(kind.bindings[0].evidence, kind.bindings[1].evidence);
    assert!(kind.derivation.is_some());
    assert!(kind.tool.is_none());
}

#[test]
fn explicit_challenge_roles_reuse_canonical_checks_without_duplicate_reviews() {
    use eal_collector_contracts::execution::EvaluationCriterion;
    use engarg::support_requirements::SupportRequirementRole;
    use serde_json::json;
    let mode = reasoning::cases().remove(0);
    let (request, mut case, mut elaboration, _, _) = graph::graph_fixture(&mode);
    case.defeaters.push(
        serde_json::from_value(json!({
            "id":"D1","target_id":"A1","kind":"undercutting",
            "challenge":"The observed applicability condition may fail.",
            "basis":"hypothetical","evidence_ids":[],"material":true,"status":"open",
            "response":"Resolve only from the measured applicability condition."
        }))
        .unwrap(),
    );
    let mut need = case.evidence_plan[0].clone();
    need.id = "N8".into();
    need.target_id = "D1".into();
    let acceptance = need.acceptance_criterion.clone();
    case.evidence_plan.push(need);
    let base = elaboration
        .requirements
        .iter()
        .find(|r| r.target_id == "A1")
        .unwrap()
        .clone();
    let check = elaboration
        .evaluations
        .iter()
        .find(|e| e.requirement_id == base.id)
        .unwrap()
        .clone();
    for (id, role, criterion) in [
        (
            "activation",
            SupportRequirementRole::ChallengeActivation,
            EvaluationCriterion::Equals {
                expected: json!(false),
            },
        ),
        (
            "resolution",
            SupportRequirementRole::ChallengeResolution,
            EvaluationCriterion::AllTrue,
        ),
    ] {
        let mut requirement = base.clone();
        requirement.id = id.into();
        requirement.target_id = "D1".into();
        requirement.role = role;
        requirement.acceptance = criterion.clone();
        requirement.evidence_need_ids = if id == "activation" {
            vec!["N8".into()]
        } else {
            vec![]
        };
        requirement.original_acceptance_criteria = if id == "activation" {
            vec![acceptance.clone()]
        } else {
            vec![]
        };
        let mut evaluation = check.clone();
        evaluation.id = format!("{id}_evaluation");
        evaluation.requirement_id = id.into();
        evaluation.criterion = criterion;
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
    }
    let mut ambiguous = elaboration.clone();
    ambiguous
        .requirements
        .iter_mut()
        .find(|r| r.id == "activation")
        .unwrap()
        .role = SupportRequirementRole::Support;
    assert!(
        support_export::compile_direct(
            &request,
            &case,
            &ambiguous,
            &versions(&ambiguous),
            &ExportPolicy::default()
        )
        .err()
        .unwrap()
        .to_string()
        .contains("explicit challenge_activation")
    );
    let output = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions(&elaboration),
        &ExportPolicy::default(),
    )
    .unwrap();
    let program =
        composition::compile(&output.bundle.source, None, "", ExecutionLimits::default()).unwrap();
    assert!(validation::validate(&program, default_registry()).is_empty());
    assert!(
        !output
            .review_requests
            .iter()
            .any(|review| review.input["target"] == "D1")
    );
    assert!(
        program
            .objections
            .values()
            .any(|objection| objection.target == "assumption_4131")
    );
    assert!(
        !output
            .bundle
            .source
            .contains("evidence challenge_review_4431")
    );
    assert!(
        !output
            .bundle
            .source
            .contains("evidence resolution_review_4431")
    );
}
