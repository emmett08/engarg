//! Capture the meaning of an objection's target and explain later changes.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::domain::{ArgumentIndex, ArgumentNode, DissentFieldChange};

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

/// Admit the exact serialised shapes produced by capture, including every direct
/// endpoint. Invalid historical candidates may have a missing (null) endpoint.
pub(super) fn valid_snapshot(context: &Value, id: &str) -> bool {
    fn target(value: &Value, id: &str) -> Option<Vec<String>> {
        use crate::domain::{Assumption, ClaimNode, ClaimVersion, Defeater, Ground, Warrant};
        if value.is_null() {
            return Some(Vec::new());
        }
        let mut full = value.clone();
        let object = full.as_object_mut()?;
        let (canonical, related) = if matches!(id, "current_claim" | "stronger_target") {
            let node: ClaimVersion = serde_json::from_value(full).ok()?;
            (semantic_fields(&node, &[], &[]), Vec::new())
        } else if object.contains_key("rule") {
            object.insert("backing_evidence_ids".into(), json!([]));
            let node: Warrant = serde_json::from_value(full).ok()?;
            if node.id != id {
                return None;
            }
            let related = node
                .premise_ids
                .iter()
                .cloned()
                .chain(std::iter::once(node.conclusion_id.clone()))
                .collect();
            (semantic_node(Some(ArgumentNode::Warrant(&node))), related)
        } else if object.contains_key("critical") {
            object.insert("status".into(), json!("untested"));
            object.insert("evidence_ids".into(), json!([]));
            object.insert("validation".into(), json!("historical snapshot"));
            let node: Assumption = serde_json::from_value(full).ok()?;
            if node.id != id {
                return None;
            }
            (
                semantic_node(Some(ArgumentNode::Assumption(&node))),
                node.applies_to.clone(),
            )
        } else if object.contains_key("challenge") {
            object.insert("status".into(), json!("open"));
            object.insert("evidence_ids".into(), json!([]));
            object.insert("response".into(), json!("historical snapshot"));
            let node: Defeater = serde_json::from_value(full).ok()?;
            if node.id != id {
                return None;
            }
            (
                semantic_node(Some(ArgumentNode::Defeater(&node))),
                vec![node.target_id.clone()],
            )
        } else if object.contains_key("statement") {
            object.insert("status".into(), json!("missing"));
            object.insert("evidence_ids".into(), json!([]));
            let node: Ground = serde_json::from_value(full).ok()?;
            if node.id != id {
                return None;
            }
            (semantic_node(Some(ArgumentNode::Ground(&node))), Vec::new())
        } else {
            let node: ClaimNode = serde_json::from_value(full).ok()?;
            if node.id != id {
                return None;
            }
            (semantic_node(Some(ArgumentNode::Claim(&node))), Vec::new())
        };
        (canonical == *value).then_some(related)
    }
    let Some(object) = context.as_object() else {
        return false;
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "scope" | "target" | "related_targets"))
        || serde_json::from_value::<crate::domain::EngineeringScope>(context["scope"].clone())
            .is_err()
        || !context["target"].is_object()
    {
        return false;
    }
    let Some(related) = target(&context["target"], id) else {
        return false;
    };
    let related: BTreeSet<_> = related.iter().map(String::as_str).collect();
    if related.is_empty() {
        return !object.contains_key("related_targets");
    }
    let Some(endpoints) = context["related_targets"].as_object() else {
        return false;
    };
    endpoints
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        == related
        && endpoints
            .iter()
            .all(|(id, value)| target(value, id).is_some())
}

pub(crate) fn capture(index: &ArgumentIndex<'_>, id: &str) -> Value {
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

/// Complete nested differences over exactly the fields protected by capture.
pub(super) fn differences(original: &Value, current: &Value) -> Vec<DissentFieldChange> {
    fn walk(
        path: String,
        before: Option<&Value>,
        after: Option<&Value>,
        changes: &mut Vec<DissentFieldChange>,
    ) {
        if before == after {
            return;
        }
        if let (Some(Value::Object(before)), Some(Value::Object(after))) = (before, after) {
            let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                walk(
                    format!("{path}/{escaped}"),
                    before.get(key),
                    after.get(key),
                    changes,
                );
            }
        } else {
            changes.push(DissentFieldChange {
                path,
                before_present: before.is_some(),
                before: before.cloned().unwrap_or(Value::Null),
                after_present: after.is_some(),
                after: after.cloned().unwrap_or(Value::Null),
            });
        }
    }
    let mut changes = Vec::new();
    for group in ["scope", "target", "related_targets"] {
        let before = &original[group];
        if group != "scope" && before.is_null() {
            continue;
        }
        if group == "related_targets"
            && let Some(related) = before.as_object()
        {
            let mut completed = before.clone();
            for (id, value) in related {
                if value.is_null() {
                    completed[id] = current[group][id].clone();
                }
            }
            walk(
                format!("/{group}"),
                Some(&completed),
                current.get(group),
                &mut changes,
            );
        } else {
            walk(
                format!("/{group}"),
                original.get(group),
                current.get(group),
                &mut changes,
            );
        }
    }
    changes
}
