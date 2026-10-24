//! Capture the meaning of an objection's target and explain later changes.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::domain::{ArgumentIndex, ArgumentNode};

fn semantic_fields<T: Serialize>(value: &T, mutable: &[&str], sets: &[&str]) -> Value {
    let mut value = serde_json::to_value(value).expect("target serialises");
    let object = value.as_object_mut().expect("target is an object");
    for key in mutable {
        object.remove(*key);
    }
    // These relationships express membership, not a sequence of inference steps.
    // Keep duplicates: ordinary graph admission remains responsible for them.
    for key in sets {
        if let Some(Value::Array(items)) = object.get_mut(*key) {
            items.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
        }
    }
    value
}

fn semantic_node(node: Option<ArgumentNode<'_>>) -> Value {
    match node {
        Some(ArgumentNode::Root(value)) => semantic_fields(value, &[], &[]),
        Some(ArgumentNode::Claim(value)) => semantic_fields(value, &[], &[]),
        Some(ArgumentNode::Ground(value)) => {
            semantic_fields(value, &["status", "evidence_ids"], &[])
        }
        Some(ArgumentNode::Warrant(value)) => {
            semantic_fields(value, &["backing_evidence_ids"], &["premise_ids"])
        }
        Some(ArgumentNode::Assumption(value)) => semantic_fields(
            value,
            &["status", "evidence_ids", "validation"],
            &["applies_to"],
        ),
        Some(ArgumentNode::Defeater(value)) => {
            semantic_fields(value, &["status", "response", "evidence_ids"], &[])
        }
        None => Value::Null,
    }
}

pub(super) fn capture(index: &ArgumentIndex<'_>, id: &str) -> Value {
    let node = index.node(id);
    // Bind the propositions at direct relation endpoints as well as their IDs.
    // One hop preserves these local relations without copying dependency closures
    // or recursing through cyclic assumption/defeater attachments.
    let related: BTreeSet<&str> = match node {
        Some(ArgumentNode::Warrant(warrant)) => warrant
            .premise_ids
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(warrant.conclusion_id.as_str()))
            .collect(),
        Some(ArgumentNode::Assumption(assumption)) => {
            assumption.applies_to.iter().map(String::as_str).collect()
        }
        Some(ArgumentNode::Defeater(defeater)) => {
            std::iter::once(defeater.target_id.as_str()).collect()
        }
        _ => BTreeSet::new(),
    };
    let mut context = json!({"scope": index.case().scope, "target": semantic_node(node)});
    if !related.is_empty() {
        context["related_targets"] = Value::Object(
            related
                .into_iter()
                .map(|id| (id.into(), semantic_node(index.node(id))))
                .collect(),
        );
    }
    context
}

/// A missing target in an invalid candidate has no meaning to freeze yet.
/// Its identifier and the known scope are still retained by the caller.
pub(super) fn complete_target(original: &mut Value, current: &Value) {
    if original["target"].is_null() && !current["target"].is_null() {
        original["target"] = current["target"].clone();
        if let Some(related) = current.get("related_targets") {
            original["related_targets"] = related.clone();
        }
    } else if original["target"] == current["target"]
        && let Some(related) = original
            .get_mut("related_targets")
            .and_then(Value::as_object_mut)
    {
        for (id, value) in related {
            if value.is_null() && !current["related_targets"][id.as_str()].is_null() {
                *value = current["related_targets"][id.as_str()].clone();
            }
        }
    }
}

/// Report field names rather than copying potentially large source text into
/// diagnostics. Exact text comparison is conservative, not a semantic oracle.
pub(super) fn changed_fields(original: &Value, current: &Value) -> Vec<String> {
    let mut changed = Vec::new();
    for group in ["scope", "target", "related_targets"] {
        let before = &original[group];
        let after = &current[group];
        if group != "scope" && before.is_null() {
            continue;
        }
        if before == after {
            continue;
        }
        if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
            let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            for key in keys {
                if group == "related_targets" && before.get(key).is_some_and(Value::is_null) {
                    continue;
                }
                if before.get(key) != after.get(key) {
                    changed.push(format!("{group}.{key}"));
                }
            }
        } else {
            changed.push(group.into());
        }
    }
    changed
}
