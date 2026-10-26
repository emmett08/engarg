//! Generation checks and retained-dissent checks used by adjudication and repair.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    argument_depth::ClaimDepths,
    domain::{
        ArgumentCase, ArgumentIndex, Defeater, DefeaterBasis, DefeaterKind, DefeaterStatus,
        InferenceKind, Request, validate_argument,
    },
    provider::Role,
};

mod revisions;
mod target_context;
pub(crate) use revisions::{
    dissent_changes, install_history, prompt_revisions, restore_host_fields, validate_history,
};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RetainedObjection {
    /// None only before a specialist challenge has first been attached to a case.
    pub(crate) identity: Option<ObjectionIdentity>,
    pub(crate) id: String,
    pub(crate) challenge: String,
    pub(crate) source_role: Role,
    /// Later revisions cannot evade an existing obligation by downgrading materiality.
    pub(crate) must_remain_material: bool,
}

/// Semantic identity is fixed at first attachment, while disposition may evolve.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ObjectionIdentity {
    target_id: String,
    kind: DefeaterKind,
    basis: DefeaterBasis,
    target_context: Value,
}

fn identity(index: &ArgumentIndex<'_>, defeater: &Defeater) -> ObjectionIdentity {
    ObjectionIdentity {
        target_id: defeater.target_id.clone(),
        kind: defeater.kind,
        basis: defeater.basis,
        target_context: target_context::capture(index, &defeater.target_id),
    }
}

/// Carry authored dissent into the next review without changing its disposition.
/// Existing identities and challenge text remain authoritative. A later case may
/// explain why a revision answers a challenge, while retaining its original record.
pub(crate) fn retain_objections(
    case: &ArgumentCase,
    source_role: Role,
    retained: &mut Vec<RetainedObjection>,
) {
    let index = ArgumentIndex::new(case);
    for defeater in &case.defeaters {
        if let Some(objection) = retained.iter_mut().find(|item| item.id == defeater.id) {
            objection.must_remain_material |= defeater.material;
            if objection.identity.is_none() && objection.challenge == defeater.challenge {
                objection.identity = Some(identity(&index, defeater));
            } else if objection.challenge == defeater.challenge
                && let Some(original) = &mut objection.identity
                && original.target_id == defeater.target_id
                && original.kind == defeater.kind
                && original.basis == defeater.basis
            {
                target_context::complete_target(
                    &mut original.target_context,
                    &target_context::capture(&index, &defeater.target_id),
                );
            }
        } else {
            retained.push(RetainedObjection {
                identity: Some(identity(&index, defeater)),
                id: defeater.id.clone(),
                challenge: defeater.challenge.clone(),
                source_role,
                must_remain_material: defeater.material,
            });
        }
    }
}

/// No corrective data is invented here. Errors go back to the model, whose full
/// replacement must pass exactly the same checks before it can be reported.
pub(crate) fn errors(
    request: &Request,
    case: &ArgumentCase,
    objections: &[RetainedObjection],
    max_depth: Option<u8>,
) -> Vec<String> {
    // Generated applications omit host-owned snapshots and computed fields.
    // Compare supplied values before filling absent host-owned values.
    let (prepared, mut history_errors) = revisions::prepare(case, objections);
    let case = &prepared;
    let validated = validate_argument(request, case);
    let mut errors: Vec<String> = validated
        .as_ref()
        .err()
        .map(|diagnostics| diagnostics.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    errors.append(&mut history_errors);
    for defeater in &case.defeaters {
        if defeater.status == DefeaterStatus::InapplicableAfterRevision
            && !revisions::application_covers(case, &defeater.id)
        {
            errors.push(format!("DISSENT_REVISION_REQUIRED objection {} needs an explicit removed_by_revision application linked to a real retained target change", defeater.id));
        }
    }
    if errors.is_empty()
        && let Some(limit) = max_depth
    {
        let actual =
            ClaimDepths::from_index(validated.as_ref().expect("validated argument").index())
                .maximum();
        if actual > usize::from(limit) {
            errors.push(format!(
                "argument depth is {actual} warrant layers, exceeding argument-depth {limit}; simplify decomposition while retaining conditions and evidence obligations"
            ));
        }
    }
    // Historical imports intentionally allow an absent method. Live generation
    // must make the provider schema's bound/unresolved choice explicit instead
    // of silently inheriting the historical authored-support interpretation.
    for warrant in &case.warrants {
        if warrant.reasoning_method.is_none()
            && matches!(
                warrant.inference,
                InferenceKind::Deductive
                    | InferenceKind::Inductive
                    | InferenceKind::Abductive
                    | InferenceKind::Causal
                    | InferenceKind::Counterfactual
            )
        {
            errors.push(format!(
                "warrant {} requires an explicit bound or unresolved reasoning_method for live generation",
                warrant.id,
            ));
        }
    }
    if case.grounds.is_empty()
        || !case.warrants.iter().any(|warrant| {
            matches!(
                warrant.conclusion_id.as_str(),
                "current_claim" | "stronger_target"
            )
        })
    {
        errors.push("generation must construct at least one argument from represented grounds through a warrant to current_claim or stronger_target; an assessment and evidence plan alone are insufficient".into());
    }
    let fallback = validated.is_err().then(|| ArgumentIndex::new(case));
    let index = validated
        .as_ref()
        .ok()
        .map(|view| view.index())
        .or(fallback.as_ref())
        .expect("index available");
    for objection in objections {
        let retained = case.defeaters.iter().find(|defeater| {
            defeater.id == objection.id && defeater.challenge == objection.challenge
        });
        if retained.is_none() {
            errors.push(format!(
                "adjudicator omitted or altered retained objection {}",
                objection.id
            ));
        } else if let Some(defeater) = retained {
            if objection.must_remain_material && !defeater.material {
                errors.push(format!(
                    "revision downgraded materiality of retained objection {}",
                    objection.id
                ));
            }
            if let Some(original) = &objection.identity {
                if defeater.target_id != original.target_id
                    || defeater.kind != original.kind
                    || defeater.basis != original.basis
                {
                    errors.push(format!("DISSENT_IDENTITY_CHANGED retained objection {} changed target, attack kind or basis", objection.id));
                } else {
                    let changed = target_context::changed_fields(
                        &original.target_context,
                        &target_context::capture(index, &defeater.target_id),
                    );
                    if (!changed.is_empty()
                        || defeater.status == DefeaterStatus::InapplicableAfterRevision)
                        && !revisions::application_covers(case, &objection.id)
                    {
                        errors.push(format!("DISSENT_TARGET_CHANGED retained objection {} changed {}; restore these fields from identity.target_context, or supply an explicit dissent_revisions application retaining its applicability or explaining the specific revision that removes the challenge", objection.id, changed.join(", ")));
                    }
                }
            }
        }
    }
    errors
}

#[cfg(test)]
mod generation_tests;

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn fixture() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
            crate::live_output::case(include_str!("../examples/case.json")),
        )
    }
    #[test]
    fn every_attack_identity_component_survives_revision() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        for mutation in 0..5 {
            let mut revised = original.clone();
            let defeater = &mut revised.defeaters[0];
            match mutation {
                0 => defeater.target_id = "stronger_target".into(),
                1 => defeater.kind = DefeaterKind::Rebutting,
                2 => defeater.basis = DefeaterBasis::Observed,
                3 => defeater.challenge.push_str(" A different attack."),
                _ => defeater.id.push_str("-replacement"),
            }
            assert!(
                !errors(&request, &revised, &retained, None).is_empty(),
                "mutation {mutation}"
            );
        }
    }
    #[test]
    fn target_meaning_change_requires_explicit_revision_disposition() {
        let (request, mut original) = fixture();
        original.defeaters[0].target_id = "current_claim".into();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised.current_claim.text =
            "A substantially different proposition under the same ID.".into();
        assert!(
            errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED"))
        );
        revised.defeaters[0].status = DefeaterStatus::InapplicableAfterRevision;
        revised.defeaters[0].response = "The revised proposition removes the original quantitative target and limits reliance to measured runners.".into();
        let choice = dissent_changes(&revised, &retained).remove(0);
        revised.dissent_revisions = Some(serde_json::from_value(serde_json::json!({
            "schema":"engarg.dissent-revisions/1", "applications":[{
                "objection_id":choice.objection_id,"origin_context_sha256":choice.original_context_sha256,
                "effect":"removed_by_revision","rationale":"The identified revision removes the original quantitative target."
            }]
        })).unwrap());
        assert!(
            !errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED"))
        );
    }
    #[test]
    fn scope_replacement_changes_the_meaning_of_an_unchanged_target() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised
            .scope
            .configuration
            .push_str(" A different deployed configuration.");
        assert!(
            errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED"))
        );
    }

    #[test]
    fn new_backing_can_answer_a_warrant_attack_without_changing_its_identity() {
        let (request, mut original) = fixture();
        original.defeaters[0].target_id = original.warrants[0].id.clone();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised.warrants[0]
            .backing_evidence_ids
            .push(request.evidence[0].id.clone());
        revised.defeaters[0].status = DefeaterStatus::Refuted;
        revised.defeaters[0].response =
            "The newly retained backing supplies the applicability argument challenged here."
                .into();
        revised.defeaters[0]
            .evidence_ids
            .push(request.evidence[0].id.clone());
        assert!(
            !errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED"))
        );
    }
    #[test]
    fn a_later_failed_candidate_cannot_replace_admitted_identity() {
        let (_, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut replacement = original.clone();
        replacement.defeaters[0].target_id = "stronger_target".into();
        retain_objections(&replacement, Role::Adjudicator, &mut retained);
        assert_eq!(
            retained[0].identity.as_ref().unwrap().target_id,
            original.defeaters[0].target_id
        );
    }

    #[test]
    fn investigation_refinement_and_attachment_order_preserve_assumption_identity() {
        let (request, mut original) = fixture();
        original.defeaters[0].target_id = original.assumptions[0].id.clone();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised.assumptions[0].validation =
            "Inspect each fixture owner and compare isolated runs with paired concurrent runs."
                .into();
        assert!(revised.assumptions[0].applies_to.len() > 1);
        revised.assumptions[0].applies_to.reverse();
        assert_eq!(validate_argument(&request, &revised).map(|_| ()), Ok(()));
        assert!(errors(&request, &revised, &retained, None).is_empty());
        revised.assumptions[0]
            .statement
            .push_str(" A different condition.");
        assert!(
            errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED")
                    && message.contains("target.statement"))
        );
        let mut changed_attachment = original.clone();
        changed_attachment.assumptions[0].applies_to.pop();
        assert!(
            errors(&request, &changed_attachment, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED")
                    && message.contains("target.applies_to"))
        );
    }

    #[test]
    fn joint_premise_order_preserves_warrant_identity_but_membership_does_not() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        let target = original.defeaters[0].target_id.clone();
        let warrant = revised
            .warrants
            .iter_mut()
            .find(|w| w.id == target)
            .unwrap();
        assert!(warrant.premise_ids.len() > 1);
        warrant.premise_ids.reverse();
        assert_eq!(validate_argument(&request, &revised).map(|_| ()), Ok(()));
        assert!(errors(&request, &revised, &retained, None).is_empty());
        revised
            .warrants
            .iter_mut()
            .find(|w| w.id == target)
            .unwrap()
            .premise_ids
            .pop();
        assert!(
            errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED")
                    && message.contains("target.premise_ids"))
        );
    }

    #[test]
    fn target_diagnostics_name_all_changed_fields_without_echoing_source_values() {
        let (request, mut original) = fixture();
        original.defeaters[0].target_id = "current_claim".into();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised.current_claim.qualifier =
            "A distinct qualification absent from diagnostics.".into();
        revised.scope.configuration = "A distinct configuration absent from diagnostics.".into();
        let diagnostics = errors(&request, &revised, &retained, None);
        let diagnostic = diagnostics
            .iter()
            .find(|message| message.contains("DISSENT_TARGET_CHANGED retained objection D1"))
            .unwrap();
        assert!(diagnostic.contains("scope.configuration, target.qualifier"));
        assert!(diagnostic.contains("restore these fields from identity.target_context"));
        assert!(!diagnostic.contains("absent from diagnostics"));
    }

    #[test]
    fn supplying_a_missing_target_does_not_replace_a_fictitious_null_identity() {
        let (request, mut original) = fixture();
        original.defeaters[0].target_id = "pending-ground".into();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Adjudicator, &mut retained);
        let mut repaired = original.clone();
        let mut ground = repaired.grounds[0].clone();
        ground.id = "pending-ground".into();
        repaired.grounds.push(ground);
        assert!(
            !errors(&request, &repaired, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED"))
        );
        let mut wrong_identity = repaired.clone();
        wrong_identity.defeaters[0].kind = DefeaterKind::Rebutting;
        retain_objections(&wrong_identity, Role::Adjudicator, &mut retained);
        assert!(retained[0].identity.as_ref().unwrap().target_context["target"].is_null());
        retain_objections(&repaired, Role::Adjudicator, &mut retained);
        assert!(!retained[0].identity.as_ref().unwrap().target_context["target"].is_null());
        repaired
            .grounds
            .last_mut()
            .unwrap()
            .statement
            .push_str(" A replacement premise.");
        assert!(
            errors(&request, &repaired, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED")
                    && message.contains("target.statement"))
        );
        // Known scope remains authoritative even before the target is available.
        original
            .scope
            .configuration
            .push_str(" A replacement scope.");
        assert!(
            errors(&request, &original, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED")
                    && message.contains("scope.configuration"))
        );
    }

    #[test]
    fn removing_a_known_target_still_changes_its_context() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised
            .warrants
            .retain(|w| w.id != original.defeaters[0].target_id);
        assert!(
            errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message
                    .contains("DISSENT_TARGET_CHANGED retained objection D1 changed target"))
        );
    }

    #[test]
    fn related_premise_support_updates_preserve_the_warrant_attack_identity() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        let mut revised = original.clone();
        revised.grounds[1].status = crate::domain::GroundStatus::Reported;
        revised.grounds[1]
            .evidence_ids
            .push(request.evidence[0].id.clone());
        assert!(
            !errors(&request, &revised, &retained, None)
                .iter()
                .any(|message| message.contains("DISSENT_TARGET_CHANGED retained objection D1"))
        );
    }

    #[test]
    fn a_changed_relation_cannot_fill_a_missing_endpoint_context() {
        let (_, repaired) = fixture();
        let mut original = repaired.clone();
        original.grounds.retain(|ground| ground.id != "G2");
        let mut retained = Vec::new();
        retain_objections(&original, Role::Adjudicator, &mut retained);
        let mut changed_relation = repaired.clone();
        changed_relation.warrants[2]
            .rule
            .push_str(" A replacement inference.");
        retain_objections(&changed_relation, Role::Adjudicator, &mut retained);
        let context = &retained[0].identity.as_ref().unwrap().target_context;
        assert!(context["related_targets"]["G2"].is_null());
        retain_objections(&repaired, Role::Adjudicator, &mut retained);
        let context = &retained[0].identity.as_ref().unwrap().target_context;
        assert_eq!(
            context["related_targets"]["G2"]["statement"],
            repaired.grounds[1].statement
        );
    }

    #[test]
    fn cyclic_attachment_snapshots_stop_at_the_direct_endpoint() {
        let (_, mut case) = fixture();
        let assumption_id = case.assumptions[0].id.clone();
        case.assumptions[0].applies_to.push(assumption_id.clone());
        case.defeaters[0].target_id = case.defeaters[0].id.clone();
        let index = ArgumentIndex::new(&case);
        let assumption = target_context::capture(&index, &assumption_id);
        assert!(assumption["related_targets"][&assumption_id].is_object());
        assert!(
            assumption["related_targets"][&assumption_id]
                .get("related_targets")
                .is_none()
        );
        let attack_id = &case.defeaters[0].id;
        let attack = target_context::capture(&index, attack_id);
        assert!(attack["related_targets"][attack_id].is_object());
        assert!(
            attack["related_targets"][attack_id]
                .get("related_targets")
                .is_none()
        );
    }

    #[test]
    fn a_warrant_attack_binds_the_meaning_of_its_direct_premises_and_conclusion() {
        let (request, original) = fixture();
        let mut retained = Vec::new();
        retain_objections(&original, Role::Synthesiser, &mut retained);
        for target in ["G2", "stronger_target"] {
            let mut revised = original.clone();
            if target == "G2" {
                revised.grounds[1].statement =
                    "A different empirical premise under the same ID.".into();
            } else {
                revised.stronger_target.text =
                    "A different intended conclusion under the same ID.".into();
            }
            assert!(
                errors(&request, &revised, &retained, None)
                    .iter()
                    .any(|message| message
                        .contains("DISSENT_TARGET_CHANGED retained objection D1")
                        && message.contains(&format!("related_targets.{target}"))),
                "the retained W3 attack did not detect the changed {target} proposition"
            );
        }
    }

    #[test]
    fn a_missing_warrant_endpoint_acquires_context_when_the_same_relation_is_repaired() {
        let (request, repaired) = fixture();
        let mut original = repaired.clone();
        original.grounds.retain(|ground| ground.id != "G2");
        let mut retained = Vec::new();
        retain_objections(&original, Role::Adjudicator, &mut retained);
        assert!(errors(&request, &repaired, &retained, None).is_empty());
        retain_objections(&repaired, Role::Adjudicator, &mut retained);
        let mut replacement = repaired.clone();
        replacement.grounds[1]
            .statement
            .push_str(" A different proposition.");
        assert!(
            errors(&request, &replacement, &retained, None)
                .iter()
                .any(
                    |message| message.contains("DISSENT_TARGET_CHANGED retained objection D1")
                        && message.contains("related_targets.G2")
                )
        );
    }
}
