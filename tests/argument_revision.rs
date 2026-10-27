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
        schema_version: "engarg.run.v2".into(),
        execution_mode: "explicit_fixture".into(),
        input_sha256: eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
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
    assert_eq!(
        serde_json::to_value(&revised.previous_argument).unwrap(),
        before["argument"]
    );
    assert_eq!(
        revised.parent_report_sha256,
        eal_collector_contracts::execution::canonical_digest(&source).unwrap()
    );
    assert!(revised.invalidated.contains(&"collection".into()));
    assert!(revised.invalidated.contains(&"route_plan".into()));
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
