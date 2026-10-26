//! Bind authored revision effects to immutable origins and exact current contexts.

use std::collections::{BTreeMap, BTreeSet};

use eal_collector_contracts::execution::canonical_digest;
use serde::Serialize;
use serde_json::{Value, json};

use super::{RetainedObjection, target_context};
use crate::domain::{
    ArgumentCase, ArgumentIndex, DISSENT_REVISIONS_SCHEMA, DefeaterStatus, DissentOrigin,
    DissentRevisionEffect,
};

fn digest(value: &Value) -> String {
    canonical_digest(value).expect("serialised target context has canonical JSON identity")
}

#[derive(Debug, Serialize)]
pub(crate) struct DissentChange {
    pub objection_id: String,
    pub target_id: String,
    pub original_context_sha256: String,
    pub current_context_sha256: String,
    pub original_context: Value,
    pub current_context: Value,
    pub changes: Vec<crate::domain::DissentFieldChange>,
}

pub(crate) fn dissent_changes(
    case: &ArgumentCase,
    retained: &[RetainedObjection],
) -> Vec<DissentChange> {
    let index = ArgumentIndex::new(case);
    retained
        .iter()
        .filter_map(|objection| {
            let original = objection.identity.as_ref()?;
            let current = target_context::capture(&index, &original.target_id);
            let changes = target_context::differences(&original.target_context, &current);
            Some(DissentChange {
                objection_id: objection.id.clone(),
                target_id: original.target_id.clone(),
                original_context_sha256: digest(&original.target_context),
                current_context_sha256: digest(&current),
                original_context: original.target_context.clone(),
                current_context: current,
                changes,
            })
        })
        .collect()
}

fn origins(retained: &[RetainedObjection]) -> Vec<DissentOrigin> {
    retained
        .iter()
        .filter_map(|objection| {
            let identity = objection.identity.as_ref()?;
            Some(DissentOrigin {
                objection_id: objection.id.clone(),
                challenge: objection.challenge.clone(),
                target_id: identity.target_id.clone(),
                kind: identity.kind,
                basis: identity.basis,
                source_role: objection.source_role.as_str().into(),
                must_remain_material: objection.must_remain_material,
                context_sha256: digest(&identity.target_context),
                target_context: identity.target_context.clone(),
            })
        })
        .collect()
}

pub(super) fn prepare(
    case: &ArgumentCase,
    retained: &[RetainedObjection],
) -> (ArgumentCase, Vec<String>) {
    let mut prepared = case.clone();
    let mut errors = Vec::new();
    let Some(history) = &mut prepared.dissent_revisions else {
        return (prepared, errors);
    };
    let expected = origins(retained);
    if !history.origins.is_empty()
        && serde_json::to_value(&history.origins).unwrap()
            != serde_json::to_value(&expected).unwrap()
    {
        errors.push(
            "DISSENT_ORIGIN_CHANGED persisted origins differ from the retained first attachments"
                .into(),
        );
    }
    history.origins = expected;
    let index = ArgumentIndex::new(case);
    for application in &mut history.applications {
        let Some(origin) = history
            .origins
            .iter()
            .find(|origin| origin.objection_id == application.objection_id)
        else {
            continue;
        };
        let current = target_context::capture(&index, &origin.target_id);
        let current_digest = digest(&current);
        let changes = target_context::differences(&origin.target_context, &current);
        if !application.current_context_sha256.is_empty()
            && application.current_context_sha256 != current_digest
        {
            errors.push(format!(
                "DISSENT_REVISION_CHANGED {} current context identity is incorrect",
                application.objection_id
            ));
        }
        if !application.changes.is_empty() && application.changes != changes {
            errors.push(format!(
                "DISSENT_REVISION_CHANGED {} nested change set is incorrect",
                application.objection_id
            ));
        }
        application.current_context_sha256 = current_digest;
        application.changes = changes;
    }
    (prepared, errors)
}

/// Called only after complete validation; this never changes a disposition.
pub(crate) fn install_history(case: &mut ArgumentCase, retained: &[RetainedObjection]) {
    case.dissent_revisions
        .get_or_insert_with(|| crate::domain::DissentRevisions {
            schema: DISSENT_REVISIONS_SCHEMA.into(),
            applications: Vec::new(),
            origins: Vec::new(),
        });
    *case = prepare(case, retained).0;
}

pub(super) fn application_covers(case: &ArgumentCase, id: &str) -> bool {
    case.dissent_revisions.as_ref().is_some_and(|history| {
        history
            .applications
            .iter()
            .any(|item| item.objection_id == id)
    })
}

/// Standalone validation proves internal consistency, not authenticity of history
/// or semantic adequacy of the model-authored rationale.
pub(crate) fn validate_history(case: &ArgumentCase) -> Vec<String> {
    let Some(history) = &case.dissent_revisions else {
        return Vec::new();
    };
    let mut errors = Vec::new();
    if history.schema != DISSENT_REVISIONS_SCHEMA {
        errors.push("unsupported dissent revision schema".into());
    }
    for defeater in &case.defeaters {
        if defeater.status == DefeaterStatus::InapplicableAfterRevision
            && !history
                .applications
                .iter()
                .any(|app| app.objection_id == defeater.id)
        {
            errors.push(format!(
                "dissent objection {} requires a removed_by_revision application",
                defeater.id
            ));
        }
    }
    let index = ArgumentIndex::new(case);
    let mut seen = BTreeSet::new();
    for origin in &history.origins {
        if !seen.insert(origin.objection_id.as_str()) {
            errors.push(format!("duplicate dissent origin {}", origin.objection_id));
        }
        if digest(&origin.target_context) != origin.context_sha256 {
            errors.push(format!(
                "dissent origin {} context digest mismatch",
                origin.objection_id
            ));
        }
        if origin.source_role.parse::<crate::provider::Role>().is_err() {
            errors.push(format!(
                "dissent origin {} source role invalid",
                origin.objection_id
            ));
        }
        if !target_context::valid_snapshot(&origin.target_context, &origin.target_id) {
            errors.push(format!(
                "dissent origin {} context is not a complete target snapshot",
                origin.objection_id
            ));
        }
        match case.defeaters.iter().find(|d| d.id == origin.objection_id) {
            Some(d)
                if d.challenge == origin.challenge
                    && d.target_id == origin.target_id
                    && d.kind == origin.kind
                    && d.basis == origin.basis
                    && (!origin.must_remain_material || d.material) => {}
            _ => errors.push(format!(
                "dissent origin {} identity or materiality changed",
                origin.objection_id
            )),
        }
        let current = target_context::capture(&index, &origin.target_id);
        let changed = !target_context::differences(&origin.target_context, &current).is_empty();
        if changed
            && !history
                .applications
                .iter()
                .any(|app| app.objection_id == origin.objection_id)
        {
            errors.push(format!(
                "dissent origin {} changed without an application record",
                origin.objection_id
            ));
        }
    }
    seen.clear();
    for application in &history.applications {
        let id = &application.objection_id;
        if !seen.insert(id.as_str()) {
            errors.push(format!("duplicate dissent application {id}"));
        }
        let Some(origin) = history
            .origins
            .iter()
            .find(|origin| origin.objection_id == *id)
        else {
            errors.push(format!("dissent application {id} lacks a retained origin"));
            continue;
        };
        let current = target_context::capture(&index, &origin.target_id);
        let changes = target_context::differences(&origin.target_context, &current);
        if application.origin_context_sha256 != origin.context_sha256
            || application.current_context_sha256 != digest(&current)
        {
            errors.push(format!(
                "dissent application {id} revision linkage mismatch"
            ));
        }
        if changes.is_empty() || application.changes != changes {
            errors.push(format!(
                "dissent application {id} must identify the exact nonempty revision"
            ));
        }
        if application.rationale.trim().is_empty() {
            errors.push(format!(
                "dissent application {id} needs an authored rationale"
            ));
        }
        let Some(defeater) = case.defeaters.iter().find(|d| d.id == *id) else {
            continue;
        };
        let valid = match application.effect {
            DissentRevisionEffect::StillApplies => matches!(
                defeater.status,
                DefeaterStatus::Open | DefeaterStatus::Sustained | DefeaterStatus::Refuted
            ),
            DissentRevisionEffect::RequiresReview => {
                defeater.status == DefeaterStatus::Open && defeater.material
            }
            DissentRevisionEffect::RemovedByRevision => {
                defeater.status == DefeaterStatus::InapplicableAfterRevision
                    && !defeater.response.trim().is_empty()
            }
        };
        if !valid {
            errors.push(format!(
                "dissent application {id} effect contradicts objection disposition"
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests;

/// Supply complete host-calculated choices before request-local compaction.
pub(crate) fn prompt_revisions(data: &mut Value) -> Result<(), crate::Error> {
    let Some(retained) = data.get("retained_objections") else {
        return Ok(());
    };
    let candidate = data
        .pointer("/candidate_json/argument")
        .or_else(|| data.get("candidate_json"))
        .or_else(|| data.get("draft"));
    let Some(candidate) = candidate else {
        return Ok(());
    };
    let retained: Vec<RetainedObjection> = serde_json::from_value(retained.clone())?;
    let mut candidate = candidate.clone();
    // Resolve only exact request-local references generated by context::argument.
    if let Some(object) = candidate.as_object_mut() {
        if object.remove("original_claim_ref").is_some() {
            object.insert("original_claim".into(), data["request"]["claim"].clone());
        }
        for defeater in object
            .get_mut("defeaters")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(id) = defeater
                .as_object_mut()
                .and_then(|d| d.remove("challenge_ref"))
                && let Some(source) = retained.iter().find(|d| Some(d.id.as_str()) == id.as_str())
            {
                defeater["challenge"] = json!(source.challenge);
            }
        }
    }
    if let Ok(case) = serde_json::from_value::<ArgumentCase>(candidate) {
        let mut contexts = BTreeMap::new();
        let mut candidates = Vec::new();
        for change in super::dissent_changes(&case, &retained) {
            contexts.insert(
                change.original_context_sha256.clone(),
                change.original_context.clone(),
            );
            contexts.insert(
                change.current_context_sha256.clone(),
                change.current_context.clone(),
            );
            let mut choice = serde_json::to_value(change)?;
            choice.as_object_mut().unwrap().remove("original_context");
            choice.as_object_mut().unwrap().remove("current_context");
            candidates.push(choice);
        }
        data["dissent_revision_candidates"] = json!(candidates);
        data["dissent_revision_contexts"] = json!(contexts);
    }
    Ok(())
}

/// Reconcile only host enrichment against a separately verified frozen case.
/// Authored application effects, rationale and original identities stay exact.
pub(crate) fn restore_host_fields(
    candidate: &mut ArgumentCase,
    canonical: &ArgumentCase,
) -> Result<(), crate::Error> {
    let invalid = |message: &str| crate::Error::InvalidInput(message.into());
    let errors = validate_history(canonical);
    if !errors.is_empty() {
        return Err(invalid(&errors.join("; ")));
    }
    match (
        &mut candidate.dissent_revisions,
        &canonical.dissent_revisions,
    ) {
        (None, None) => {}
        (None, Some(expected)) if expected.applications.is_empty() => {
            candidate.dissent_revisions = Some(expected.clone());
        }
        (Some(supplied), Some(expected)) => {
            if supplied.schema != expected.schema
                || (!supplied.origins.is_empty()
                    && serde_json::to_value(&supplied.origins)?
                        != serde_json::to_value(&expected.origins)?)
                || supplied.applications.len() != expected.applications.len()
            {
                return Err(invalid(
                    "retained dissent history differs from the frozen canonical history",
                ));
            }
            for (actual, expected) in supplied.applications.iter().zip(&expected.applications) {
                if actual.objection_id != expected.objection_id
                    || actual.origin_context_sha256 != expected.origin_context_sha256
                    || actual.effect != expected.effect
                    || actual.rationale != expected.rationale
                    || (!actual.current_context_sha256.is_empty()
                        && actual.current_context_sha256 != expected.current_context_sha256)
                    || (!actual.changes.is_empty() && actual.changes != expected.changes)
                {
                    return Err(invalid(
                        "retained dissent application differs from the frozen canonical application",
                    ));
                }
            }
            *supplied = expected.clone();
        }
        _ => {
            return Err(invalid(
                "retained authored dissent applications differ from the canonical argument",
            ));
        }
    }
    if serde_json::to_value(&*candidate)? != serde_json::to_value(canonical)? {
        return Err(invalid(
            "retained final case belongs to a different canonical argument",
        ));
    }
    Ok(())
}
