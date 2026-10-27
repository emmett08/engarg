//! Explicit revisions retain dissent and make stale executable artefacts visible.
#[path = "common/live_output.rs"]
mod live_output;

use engarg::{
    domain::{ArgumentCase, DissentApplication, DissentRevisionEffect, DissentRevisions, Request},
    engine::{
        Mode, RunReport,
        support::revision::{preview, revise},
    },
};
use serde_json::{Value, json};

fn report() -> RunReport {
    let request: Request =
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap();
    let argument: ArgumentCase =
        live_output::case(include_str!("../examples/claim-only-case.json"));
    RunReport {
        schema_version: "engarg.run.v3".into(),
        execution_mode: "explicit_fixture".into(),
        input_sha256: eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
        support_elaboration: None,
        direct_bindings: None,
        design_review: None,
        argument_revisions: Vec::new(),
        target_runtime: None,
        authoring_recovery: None,
        evidence_planning: None,
        replanning: None,
        support_recovery: None,
        limitations: vec![],
    }
}

#[test]
fn revision_retains_the_original_and_invalidates_derived_execution() {
    let source = report();
    let before = serde_json::to_value(&source).unwrap();
    let mut candidate = source.argument.clone();
    candidate
        .adjudication
        .push_str(" The next assessment will use the revised source.");
    let revised = revise(&source, candidate).unwrap();
    assert_eq!(serde_json::to_value(&source).unwrap(), before);
    let revision = revised.argument_revisions.last().unwrap();
    assert_eq!(
        serde_json::to_value(&revision.previous_report.argument).unwrap(),
        before["argument"]
    );
    assert_eq!(
        revision.parent_report_sha256,
        eal_collector_contracts::execution::canonical_digest(&source).unwrap()
    );
    assert!(revision.invalidated.contains(&"collection".into()));
    assert!(revision.invalidated.contains(&"direct_bindings".into()));
    assert!(revised.argument.dissent_revisions.is_some());
}

#[test]
fn changing_a_challenged_target_requires_explicit_dissent_application() {
    let source = report();
    let mut candidate = source.argument.clone();
    candidate.warrants[0]
        .applicability
        .push_str(" Restrict this route to an explicitly frozen runner revision.");
    assert!(
        revise(&source, candidate.clone())
            .unwrap_err()
            .to_string()
            .contains("DISSENT_TARGET_CHANGED")
    );
    let inspection = preview(&source, &candidate).unwrap();
    let applications: Vec<DissentApplication> = inspection["dissent_revision_candidates"].as_array().unwrap()
        .iter().filter(|entry| !entry["changes"].as_array().unwrap().is_empty())
        .map(|entry| DissentApplication {
            objection_id:entry["objection_id"].as_str().unwrap().into(),
            origin_context_sha256:entry["original_context_sha256"].as_str().unwrap().into(),
            effect:DissentRevisionEffect::RequiresReview,
            rationale:"The scope is narrower, but the original concern still requires review in that scope.".into(),
            current_context_sha256:String::new(), changes:vec![],
        }).collect();
    candidate.dissent_revisions = Some(DissentRevisions {
        schema: "engarg.dissent-revisions/1".into(),
        applications,
        origins: vec![],
    });
    let revised = revise(&source, candidate).unwrap();
    let history = revised.argument.dissent_revisions.unwrap();
    assert_eq!(history.origins.len(), source.argument.defeaters.len());
    assert!(
        history
            .applications
            .iter()
            .all(|entry| !entry.current_context_sha256.is_empty() && !entry.changes.is_empty())
    );
}

#[test]
fn revision_cannot_drop_dissent_or_promote_an_unperformed_observation() {
    let source = report();
    let mut candidate = source.argument.clone();
    candidate.defeaters.clear();
    assert!(revise(&source, candidate).is_err());
    let mut candidate: Value = serde_json::to_value(&source.argument).unwrap();
    candidate["grounds"][0]["status"] = json!("reported");
    assert!(revise(&source, serde_json::from_value(candidate).unwrap()).is_err());
}

#[path = "common/pending_support.rs"]
mod pending_support;

#[test]
fn explicit_criterion_revision_requires_prior_identity_and_can_be_revised_again() {
    use engarg::{
        direct_planning::DirectPlan, engine::support::revision::revise_with_support,
        support_requirements::RequirementRevision,
    };
    let mut source = report();
    let support = pending_support::pending(&source.argument);
    source.direct_bindings = Some(DirectPlan::unbound(&support));
    source.support_elaboration = Some(support.clone());
    let mut candidate = source.argument.clone();
    candidate.evidence_plan[0]
        .acceptance_criterion
        .push_str(" The measurement must identify the selected revision.");
    let mut current = pending_support::pending(&candidate);
    assert!(revise_with_support(&source, candidate.clone(), Some(current.clone()), None).is_err());
    for requirement in &current.requirements {
        let previous = support
            .requirements
            .iter()
            .find(|r| r.id == requirement.id)
            .unwrap();
        let before = eal_collector_contracts::execution::canonical_digest(previous).unwrap();
        if before != eal_collector_contracts::execution::canonical_digest(requirement).unwrap() {
            current.revisions.push(RequirementRevision {
                requirement_id: requirement.id.clone(),
                previous_requirement_sha256: before,
                reason:
                    "The explicit argument revision identifies the measurement's source revision."
                        .into(),
            });
        }
    }
    let revised = revise_with_support(&source, candidate, Some(current), None).unwrap();
    engarg::report::read_direct(&serde_json::to_value(&revised).unwrap()).unwrap();
    let next = revise(&revised, revised.argument.clone()).unwrap();
    assert_eq!(next.argument_revisions.len(), 2);
    engarg::report::read_direct(&serde_json::to_value(next).unwrap()).unwrap();
}
