//! Distinguishing tests remain explicit obligations without changing case formats.
use super::{LEGACY_SUPPORT_ELABORATION_SCHEMA, SupportElaboration};
use crate::domain::{ArgumentCase, ArgumentIndex};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct AlternativeObligation {
    pub alternative_id: String,
    pub explanation: String,
    pub distinguishing_test: String,
    pub claim_ids: Vec<String>,
}

/// Positional canonical IDs can be authored in the same response as the case;
/// models do not need to calculate digests of their own newly generated text.
pub fn alternative_obligations(case: &ArgumentCase) -> Vec<AlternativeObligation> {
    case.alternatives
        .iter()
        .enumerate()
        .map(|(index, alternative)| AlternativeObligation {
            alternative_id: format!("alternative_{}", index + 1),
            explanation: alternative.explanation.clone(),
            distinguishing_test: alternative.distinguishing_test.clone(),
            claim_ids: vec!["current_claim".into(), "stronger_target".into()],
        })
        .collect()
}

pub fn alternative_requirement_gates(
    elaboration: &SupportElaboration,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut gates = BTreeMap::<String, BTreeSet<String>>::new();
    for support in &elaboration.alternative_support {
        for requirement in &support.requirement_ids {
            gates
                .entry(requirement.clone())
                .or_default()
                .extend(support.claim_ids.iter().cloned());
        }
    }
    gates
}

/// Legacy records remain readable, but missing discriminating work is never
/// promoted into an executable or satisfied support obligation.
pub fn legacy_alternative_gaps(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
) -> Vec<AlternativeObligation> {
    if elaboration.schema != LEGACY_SUPPORT_ELABORATION_SCHEMA {
        return vec![];
    }
    let covered = elaboration
        .alternative_support
        .iter()
        .map(|support| support.alternative_id.as_str())
        .collect::<BTreeSet<_>>();
    alternative_obligations(case)
        .into_iter()
        .filter(|alternative| !covered.contains(alternative.alternative_id.as_str()))
        .collect()
}

pub(super) fn validate(case: &ArgumentCase, elaboration: &SupportElaboration) -> Vec<String> {
    let mut errors = Vec::new();
    let index = ArgumentIndex::new(case);
    let obligations = alternative_obligations(case)
        .into_iter()
        .map(|obligation| (obligation.alternative_id.clone(), obligation))
        .collect::<BTreeMap<_, _>>();
    let requirements = elaboration
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect::<BTreeMap<_, _>>();
    let mut covered = BTreeSet::new();
    for support in &elaboration.alternative_support {
        let Some(obligation) = obligations.get(&support.alternative_id) else {
            errors.push(format!(
                "unknown alternative obligation {}",
                support.alternative_id
            ));
            continue;
        };
        if !covered.insert(support.alternative_id.as_str()) {
            errors.push(format!(
                "alternative obligation {} is repeated",
                support.alternative_id
            ));
        }
        if support.distinguishing_test != obligation.distinguishing_test {
            errors.push(format!(
                "alternative obligation {} must preserve its exact original distinguishing test",
                support.alternative_id
            ));
        }
        if support.claim_ids.is_empty()
            || support.claim_ids.len() > 4096
            || support.claim_ids.iter().collect::<BTreeSet<_>>().len() != support.claim_ids.len()
            || support
                .claim_ids
                .iter()
                .any(|claim| index.claim(claim).is_none())
        {
            errors.push(format!(
                "alternative obligation {} requires distinct canonical claim IDs",
                support.alternative_id
            ));
        }
        if obligation
            .claim_ids
            .iter()
            .any(|claim| !support.claim_ids.contains(claim))
        {
            errors.push(format!("case-level alternative {} must gate current_claim and stronger_target; the original alternative has no independently established narrower scope",support.alternative_id));
        }
        if support.requirement_ids.is_empty()
            || support.requirement_ids.len() > 4096
            || support
                .requirement_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != support.requirement_ids.len()
        {
            errors.push(format!(
                "alternative obligation {} requires distinct support requirement IDs",
                support.alternative_id
            ));
        }
        for id in &support.requirement_ids {
            match requirements.get(id.as_str()) {
                None=>errors.push(format!("alternative obligation {} names unknown support requirement {id}",support.alternative_id)),
                Some(requirement) if !requirement.discriminating_evidence.contains(&obligation.distinguishing_test)=>errors.push(format!("support requirement {id} must explicitly retain alternative {} distinguishing test",support.alternative_id)),
                _=>{},
            }
        }
    }
    if elaboration.schema != LEGACY_SUPPORT_ELABORATION_SCHEMA {
        for alternative in obligations
            .keys()
            .filter(|id| !covered.contains(id.as_str()))
        {
            errors.push(format!(
                "alternative {alternative} requires a distinguishing support obligation"
            ));
        }
    }
    errors
}

fn bindings(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
) -> BTreeMap<String, BTreeSet<String>> {
    let obligations = alternative_obligations(case)
        .into_iter()
        .map(|obligation| (obligation.alternative_id.clone(), obligation))
        .collect::<BTreeMap<_, _>>();
    let mut bindings = BTreeMap::<String, BTreeSet<String>>::new();
    for support in &elaboration.alternative_support {
        let Some(obligation) = obligations.get(&support.alternative_id) else {
            continue;
        };
        let mut claims = support.claim_ids.clone();
        claims.sort();
        let signature=eal_collector_contracts::capability::canonical_identity(&serde_json::json!({"explanation":obligation.explanation,"distinguishing_test":obligation.distinguishing_test,"claim_ids":claims})).expect("bounded alternative serialises");
        for requirement in &support.requirement_ids {
            bindings
                .entry(requirement.clone())
                .or_default()
                .insert(signature.clone());
        }
    }
    bindings
}

pub(super) fn changed_requirement_bindings(
    previous_case: &ArgumentCase,
    previous: &SupportElaboration,
    current_case: &ArgumentCase,
    current: &SupportElaboration,
) -> BTreeSet<String> {
    let previous = bindings(previous_case, previous);
    let current = bindings(current_case, current);
    previous
        .keys()
        .chain(current.keys())
        .filter(|id| previous.get(*id) != current.get(*id))
        .cloned()
        .collect()
}

/// A dedicated alternative requirement can retire with its removed hypothesis;
/// a retained premise or evidence-need obligation cannot use this exception.
pub(super) fn retired_alternative_requirement(
    previous_case: &ArgumentCase,
    previous: &SupportElaboration,
    current_case: &ArgumentCase,
    requirement: &super::SupportRequirement,
) -> bool {
    if !requirement.evidence_need_ids.is_empty()
        || !requirement.original_acceptance_criteria.is_empty()
        || ArgumentIndex::new(previous_case)
            .claim(&requirement.target_id)
            .is_none()
    {
        return false;
    }
    let previous_obligations = alternative_obligations(previous_case)
        .into_iter()
        .map(|obligation| (obligation.alternative_id.clone(), obligation))
        .collect::<BTreeMap<_, _>>();
    let current_bodies = alternative_obligations(current_case)
        .into_iter()
        .map(|obligation| (obligation.explanation, obligation.distinguishing_test))
        .collect::<BTreeSet<_>>();
    let covered = previous
        .alternative_support
        .iter()
        .filter(|support| support.requirement_ids.contains(&requirement.id))
        .filter_map(|support| previous_obligations.get(&support.alternative_id))
        .collect::<Vec<_>>();
    !covered.is_empty()
        && covered.iter().all(|obligation| {
            !current_bodies.contains(&(
                obligation.explanation.clone(),
                obligation.distinguishing_test.clone(),
            ))
        })
}
