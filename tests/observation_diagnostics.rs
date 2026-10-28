//! Incomplete authored obligations remain inspectable without invented evidence.
use eal_core::{ExecutionLimits, composition, expressions::evaluate_expression};
use engarg::{
    domain::{ArgumentCase, Request},
    support_export,
    support_requirements::{self, SupportRequirementRole},
};
use serde_json::json;

#[allow(dead_code)]
#[path = "common/scoped_review_gates.rs"]
mod scoped_review_gates;

#[test]
fn resolution_only_target_coverage_leaves_activation_unavailable_and_unbindable() {
    let request: Request = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let mut elaboration = scoped_review_gates::elaboration(&case);
    let activation_ids = elaboration
        .requirements
        .iter()
        .filter(|requirement| requirement.role == SupportRequirementRole::ChallengeActivation)
        .map(|requirement| requirement.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!activation_ids.is_empty());
    elaboration
        .requirements
        .retain(|requirement| !activation_ids.contains(&requirement.id));
    elaboration
        .evaluations
        .retain(|evaluation| !activation_ids.contains(&evaluation.requirement_id));
    // Admission records target coverage, not completion of every executable role.
    assert!(support_requirements::validate_elaboration(&case, &elaboration).is_empty());
    let projection = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    let program = composition::compile(
        &projection.bundle.source,
        None,
        "",
        ExecutionLimits::default(),
    )
    .unwrap();
    for defeater in &case.defeaters {
        let diagnostic = projection
            .bundle
            .limitations
            .iter()
            .find(|message| {
                message.starts_with(&format!(
                    "{}: no canonical challenge_activation observation requirement was elaborated",
                    defeater.id
                ))
            })
            .expect("the missing role must remain visible");
        assert!(diagnostic.contains("author that explicit role"));
        assert!(!projection.review_requests.iter().any(|review| {
            review.input["target"] == defeater.id && review.input["role"] == "challenge_activation"
        }));
        let unavailable = program
            .evidence
            .values()
            .find(|evidence| {
                evidence.kind == "unavailable_condition"
                    && evidence.input["target"] == defeater.id
                    && evidence.input["role"] == "challenge_activation"
            })
            .unwrap();
        assert!(unavailable.tool.is_none());
        assert!(unavailable.bindings.is_empty());
        let result = evaluate_expression(
            unavailable.derivation.as_ref().unwrap(),
            &json!({"review":{"verdict":"supported"}}),
        );
        assert!(result.value.is_none());
        assert_eq!(result.issue.as_deref(), Some("missing_field"));
    }
}
