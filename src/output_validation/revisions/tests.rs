use super::*;
use crate::{
    domain::{DissentApplication, DissentRevisions, Request, validate_case},
    output_validation::{errors, retain_objections},
    provider::Role,
};

fn fixture() -> (Request, ArgumentCase, Vec<RetainedObjection>) {
    let request = serde_json::from_str(include_str!("../../../examples/request.json")).unwrap();
    let case = serde_json::from_str(include_str!("../../../examples/case.json")).unwrap();
    let mut retained = Vec::new();
    retain_objections(&case, Role::Synthesiser, &mut retained);
    (request, case, retained)
}

fn apply(case: &mut ArgumentCase, retained: &[RetainedObjection], effect: DissentRevisionEffect) {
    let changes = dissent_changes(case, retained);
    case.dissent_revisions = Some(DissentRevisions {
        schema: DISSENT_REVISIONS_SCHEMA.into(), origins: Vec::new(),
        applications: changes.into_iter().filter(|change| !change.changes.is_empty()).map(|change| DissentApplication {
            objection_id: change.objection_id, origin_context_sha256: change.original_context_sha256,
            effect, rationale: "The revised inference retains the challenged resource applicability condition; acquire the original planned measurements before relying on it.".into(),
            current_context_sha256: String::new(), changes: Vec::new(),
        }).collect(),
    });
}

#[test]
fn an_explicit_revision_retains_open_material_dissent_and_exact_origin() {
    let (request, original, retained) = fixture();
    let mut revised = original.clone();
    revised.warrants[2]
        .rule
        .push_str(" Evaluate this correspondence for the identified paired runs.");
    assert!(!errors(&request, &revised, &retained, None).is_empty());
    apply(&mut revised, &retained, DissentRevisionEffect::StillApplies);
    assert_eq!(
        errors(&request, &revised, &retained, None),
        Vec::<String>::new()
    );
    install_history(&mut revised, &retained);
    assert_eq!(validate_case(&request, &revised), Ok(()));
    let history = revised.dissent_revisions.as_ref().unwrap();
    assert_eq!(
        history.origins[0].target_context["target"]["rule"],
        original.warrants[2].rule
    );
    assert_eq!(history.applications[0].changes[0].path, "/target/rule");
    let round_trip: ArgumentCase =
        serde_json::from_value(serde_json::to_value(&revised).unwrap()).unwrap();
    assert_eq!(validate_case(&request, &round_trip), Ok(()));
}

#[test]
fn different_first_attachment_versions_do_not_require_contradictory_restoration() {
    let (request, original, mut retained) = fixture();
    let mut candidate = original.clone();
    candidate.warrants[2]
        .rule
        .push_str(" First candidate correction.");
    let mut new_objection = candidate.defeaters[0].clone();
    new_objection.id = "critic-new".into();
    new_objection.challenge =
        "The applicability condition still needs the planned measurement.".into();
    candidate.defeaters.push(new_objection);
    let mut plan = candidate.evidence_plan[0].clone();
    plan.id = "critic-new-plan".into();
    plan.target_id = "critic-new".into();
    candidate.evidence_plan.push(plan);
    retain_objections(&candidate, Role::Adjudicator, &mut retained);
    // Both objections remain on a further correction rather than pretending
    // that one has ceased to apply to whichever version is chosen.
    candidate.warrants[2]
        .rule
        .push_str(" Further candidate correction.");
    apply(
        &mut candidate,
        &retained,
        DissentRevisionEffect::StillApplies,
    );
    assert_eq!(
        errors(&request, &candidate, &retained, None),
        Vec::<String>::new()
    );
    install_history(&mut candidate, &retained);
    let history = candidate.dissent_revisions.as_ref().unwrap();
    assert_ne!(
        history.origins[0].context_sha256,
        history.origins.last().unwrap().context_sha256
    );
    assert!(
        candidate
            .defeaters
            .iter()
            .all(|d| d.material && d.status == DefeaterStatus::Open)
    );
}

#[test]
fn removed_and_requires_review_records_cannot_hide_an_applicable_open_concern() {
    let (request, mut case, retained) = fixture();
    apply(
        &mut case,
        &retained,
        DissentRevisionEffect::RemovedByRevision,
    );
    let first = &retained[0];
    let app = DissentApplication {
        objection_id: first.id.clone(),
        origin_context_sha256: digest(&first.identity.as_ref().unwrap().target_context),
        effect: DissentRevisionEffect::RemovedByRevision,
        rationale: "Revision removed it.".into(),
        current_context_sha256: String::new(),
        changes: Vec::new(),
    };
    case.dissent_revisions
        .as_mut()
        .unwrap()
        .applications
        .push(app);
    case.defeaters[0].status = DefeaterStatus::InapplicableAfterRevision;
    assert!(
        errors(&request, &case, &retained, None)
            .iter()
            .any(|e| e.contains("nonempty revision"))
    );
    case.warrants[2].rule.push_str(" Actual revised inference.");
    apply(&mut case, &retained, DissentRevisionEffect::RequiresReview);
    assert!(
        errors(&request, &case, &retained, None)
            .iter()
            .any(|e| e.contains("contradicts objection disposition"))
    );
    case.defeaters[0].status = DefeaterStatus::Open;
    assert!(errors(&request, &case, &retained, None).is_empty());
    case.evidence_plan
        .retain(|need| need.target_id != case.defeaters[0].id);
    assert!(!errors(&request, &case, &retained, None).is_empty());
}

#[test]
fn supplied_host_fields_and_incomplete_application_coverage_are_rejected() {
    let (request, mut case, retained) = fixture();
    case.warrants[2].rule.push_str(" Corrected inference.");
    apply(&mut case, &retained, DissentRevisionEffect::StillApplies);
    install_history(&mut case, &retained);
    for mutation in 0..5 {
        let mut altered = case.clone();
        let history = altered.dissent_revisions.as_mut().unwrap();
        match mutation {
            0 => history.applications[0].origin_context_sha256 = "forged".into(),
            1 => history.applications[0].current_context_sha256 = "forged".into(),
            2 => history.origins[0].challenge = "Replacement objection.".into(),
            3 => history.applications[0].changes[0].path = "/target/other".into(),
            _ => history.applications.clear(),
        }
        assert!(
            !errors(&request, &altered, &retained, None).is_empty(),
            "mutation {mutation}"
        );
        assert!(
            validate_case(&request, &altered).is_err(),
            "persisted mutation {mutation}"
        );
    }
}

#[test]
fn nested_method_changes_keep_their_complete_before_and_after_values() {
    let before = json!({"scope":{},"target":{"reasoning_method":{"method":{"kind":"deductive"},"input_evidence_need_id":"input-a"}}});
    let mut after = before.clone();
    after["target"]["reasoning_method"]["input_evidence_need_id"] = json!("input-b");
    let changes = target_context::differences(&before, &after);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].path,
        "/target/reasoning_method/input_evidence_need_id"
    );
    assert_eq!(changes[0].before, json!("input-a"));
    assert_eq!(changes[0].after, json!("input-b"));
}

#[test]
fn missing_and_explicit_null_changes_round_trip_without_conflation() {
    let before = json!({"scope":{},"target":{"rule":"original"}});
    let after = json!({"scope":{},"target":{"rule":"original","extra":null}});
    let changes = target_context::differences(&before, &after);
    assert!(!changes[0].before_present);
    assert!(changes[0].after_present);
    assert_eq!(
        serde_json::from_value::<Vec<crate::domain::DissentFieldChange>>(json!(changes)).unwrap(),
        changes
    );
}

#[test]
fn prompts_supply_host_identities_and_deduplicate_complete_contexts() {
    let (request, mut case, retained) = fixture();
    case.warrants[2].rule.push_str(" Revised inference.");
    let mut prompt =
        json!({"request":request,"candidate_json":case,"retained_objections":retained});
    prompt_revisions(&mut prompt).unwrap();
    let choices = prompt["dissent_revision_candidates"].as_array().unwrap();
    assert_eq!(choices.len(), retained.len());
    for choice in choices {
        let original = choice["original_context_sha256"].as_str().unwrap();
        let current = choice["current_context_sha256"].as_str().unwrap();
        assert_eq!(
            digest(&prompt["dissent_revision_contexts"][original]),
            original
        );
        assert_eq!(
            digest(&prompt["dissent_revision_contexts"][current]),
            current
        );
    }
}

#[test]
fn new_generation_cannot_use_the_legacy_inapplicable_exception() {
    let (request, mut case, retained) = fixture();
    case.defeaters[0].status = DefeaterStatus::InapplicableAfterRevision;
    case.defeaters[0].response = "The revision removed the challenge.".into();
    // Legacy standalone files retain their documented admission rules.
    assert_eq!(validate_case(&request, &case), Ok(()));
    assert!(
        errors(&request, &case, &retained, None)
            .iter()
            .any(|error| error.contains("DISSENT_REVISION_REQUIRED"))
    );
    case.warrants[2]
        .rule
        .push_str(" A changed target still needs explicit linkage.");
    assert!(
        errors(&request, &case, &retained, None)
            .iter()
            .any(|error| error.contains("DISSENT_REVISION_REQUIRED"))
    );
}

#[test]
fn incomplete_or_mistargeted_snapshots_fail_even_with_recomputed_hashes() {
    let (request, mut case, retained) = fixture();
    case.warrants[2].rule.push_str(" Revised inference.");
    apply(&mut case, &retained, DissentRevisionEffect::StillApplies);
    install_history(&mut case, &retained);
    for mutation in 0..4 {
        let mut forged = case.clone();
        let history = forged.dissent_revisions.as_mut().unwrap();
        let origin = &mut history.origins[0];
        match mutation {
            0 => origin.target_context["target"] = json!({}),
            1 => origin.target_context["target"]["id"] = json!("different-warrant"),
            2 => {
                origin
                    .target_context
                    .as_object_mut()
                    .unwrap()
                    .remove("related_targets");
            }
            _ => origin.target_context["target"] = Value::Null,
        }
        origin.context_sha256 = digest(&origin.target_context);
        let app = &mut history.applications[0];
        app.origin_context_sha256 = origin.context_sha256.clone();
        let current = target_context::capture(&ArgumentIndex::new(&case), &origin.target_id);
        app.changes = target_context::differences(&origin.target_context, &current);
        assert!(
            validate_case(&request, &forged).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn typed_saved_history_cannot_close_a_concern_without_a_real_revision() {
    let (request, mut case, retained) = fixture();
    install_history(&mut case, &retained);
    case.defeaters[0].status = DefeaterStatus::InapplicableAfterRevision;
    assert!(validate_case(&request, &case).is_err());
    case.dissent_revisions.as_mut().unwrap().origins.clear();
    assert!(validate_case(&request, &case).is_err());
}

#[test]
fn frozen_case_hydration_preserves_authored_applications_and_rejects_supplied_tampering() {
    let (_, mut canonical, retained) = fixture();
    canonical.warrants[2].rule.push_str(" Revised inference.");
    apply(
        &mut canonical,
        &retained,
        DissentRevisionEffect::StillApplies,
    );
    let mut provider = canonical.clone();
    install_history(&mut canonical, &retained);
    restore_host_fields(&mut provider, &canonical).unwrap();
    assert_eq!(
        serde_json::to_value(&provider).unwrap(),
        serde_json::to_value(&canonical).unwrap()
    );
    for mutation in 0..3 {
        let mut tampered = canonical.clone();
        let history = tampered.dissent_revisions.as_mut().unwrap();
        match mutation {
            0 => history.origins[0].source_role = "adjudicator".into(),
            1 => history.applications[0].rationale = "A different authored judgement.".into(),
            _ => history.applications[0].current_context_sha256 = "forged".into(),
        }
        assert!(restore_host_fields(&mut tampered, &canonical).is_err());
    }
}
