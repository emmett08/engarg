//! Generation checks and retained-dissent checks used by adjudication and repair.

use serde::Serialize;
use serde_json::Value;

use crate::{
    argument_depth::ClaimDepths,
    domain::{
        ArgumentCase, ArgumentIndex, ArgumentNode, Defeater, DefeaterBasis, DefeaterKind,
        DefeaterStatus, Request, validate_argument,
    },
    provider::Role,
};

#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub(crate) struct ObjectionIdentity {
    target_id: String,
    kind: DefeaterKind,
    basis: DefeaterBasis,
    target_context: Value,
}

fn target_context(index: &ArgumentIndex<'_>, id: &str) -> Value {
    let value = match index.node(id) {
        Some(ArgumentNode::Root(value)) => serde_json::to_value(value),
        Some(ArgumentNode::Claim(value)) => serde_json::to_value(value),
        Some(ArgumentNode::Ground(value)) => serde_json::to_value(value),
        Some(ArgumentNode::Warrant(value)) => serde_json::to_value(value),
        Some(ArgumentNode::Assumption(value)) => serde_json::to_value(value),
        Some(ArgumentNode::Defeater(value)) => serde_json::to_value(value),
        None => return Value::Null,
    }
    .expect("target serialises");
    serde_json::json!({"scope":index.case().scope,"target":semantic_context(value)})
}

fn semantic_context(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        for key in ["status", "response", "evidence_ids", "backing_evidence_ids"] {
            object.remove(key);
        }
    }
    value
}

fn identity(index: &ArgumentIndex<'_>, defeater: &Defeater) -> ObjectionIdentity {
    ObjectionIdentity {
        target_id: defeater.target_id.clone(),
        kind: defeater.kind,
        basis: defeater.basis,
        target_context: target_context(index, &defeater.target_id),
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
    let validated = validate_argument(request, case);
    let mut errors: Vec<String> = validated
        .as_ref()
        .err()
        .map(|diagnostics| diagnostics.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
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
                } else if target_context(index, &defeater.target_id) != original.target_context
                    && !(defeater.status == DefeaterStatus::InapplicableAfterRevision
                        && !defeater.response.trim().is_empty())
                {
                    errors.push(format!("DISSENT_TARGET_CHANGED retained objection {} requires an explicit inapplicable_after_revision disposition for its revised target", objection.id));
                }
            }
        }
    }
    errors
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn fixture() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
            serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
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
}
