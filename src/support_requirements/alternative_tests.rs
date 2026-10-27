use super::{
    tests::{case, pending},
    *,
};
use eal_collector_contracts::capability::canonical_identity;

#[test]
fn every_new_alternative_requires_its_exact_test_and_real_requirement_gate() {
    let mut case = case();
    let mut elaboration = pending(&case);
    let mut extra = case.alternatives[0].clone();
    extra.explanation = "Omitted mandatory checks can create an apparent speedup".into();
    extra.distinguishing_test =
        "Independently enumerate every required check and compare executed membership".into();
    case.alternatives.push(extra.clone());
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("requires a distinguishing support obligation"))
    );
    let requirement = elaboration.requirements.last_mut().unwrap();
    requirement
        .discriminating_evidence
        .push(extra.distinguishing_test.clone());
    elaboration.alternative_support.push(AlternativeSupport {
        alternative_id: format!("alternative_{}", case.alternatives.len()),
        distinguishing_test: extra.distinguishing_test,
        claim_ids: vec!["current_claim".into(), "stronger_target".into()],
        requirement_ids: vec![requirement.id.clone()],
    });
    assert!(validate_elaboration(&case, &elaboration).is_empty());
    elaboration
        .alternative_support
        .last_mut()
        .unwrap()
        .distinguishing_test = "Only convenient samples need checking".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("exact original distinguishing test"))
    );
}

#[test]
fn case_alternatives_cannot_be_hidden_on_an_unused_claim() {
    let case = case();
    let mut elaboration = pending(&case);
    elaboration.alternative_support[0].claim_ids = vec![case.subclaims[0].id.clone()];
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("must gate current_claim and stronger_target"))
    );
}

#[test]
fn historical_support_formats_are_not_executed_as_current_requirements() {
    let case = case();
    let mut elaboration = pending(&case);
    elaboration.schema = "engarg.support-elaboration/1".into();
    assert!(
        validate_elaboration(&case, &elaboration)
            .iter()
            .any(|error| error.contains("unsupported support elaboration schema"))
    );
}

#[test]
fn changed_hypothesis_needs_a_requirement_revision_even_if_its_test_is_unchanged() {
    let previous_case = case();
    let previous = pending(&previous_case);
    let mut current_case = previous_case.clone();
    current_case.alternatives[0].explanation =
        "A different material confound produces the same apparent outcome".into();
    let mut current = pending(&current_case);
    assert!(
        validate_revision(&previous_case, &previous, &current_case, &current)
            .iter()
            .any(|error| error.contains("exact prior-digest"))
    );
    let id = previous.alternative_support[0].requirement_ids[0].clone();
    let requirement = previous
        .requirements
        .iter()
        .find(|requirement| requirement.id == id)
        .unwrap();
    current.revisions.push(RequirementRevision{requirement_id:id,previous_requirement_sha256:canonical_identity(requirement).unwrap(),reason:"The revised hypothesis shares the same declared distinguishing protocol; its applicability still requires the linked independent work.".into()});
    assert!(
        validate_revision(&previous_case, &previous, &current_case, &current).is_empty(),
        "{:?}",
        validate_revision(&previous_case, &previous, &current_case, &current)
    );
}
