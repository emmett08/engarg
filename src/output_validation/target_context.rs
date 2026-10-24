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

pub(super) fn capture(index: &ArgumentIndex<'_>, id: &str) -> Value {
    let target = match index.node(id) {
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
    };
    json!({"scope": index.case().scope, "target": target})
}

/// A missing target in an invalid candidate has no meaning to freeze yet.
/// Its identifier and the known scope are still retained by the caller.
pub(super) fn complete_target(original: &mut Value, current: &Value) {
    if original["target"].is_null() && !current["target"].is_null() {
        original["target"] = current["target"].clone();
    }
}

/// Report field names rather than copying potentially large source text into
/// diagnostics. Exact text comparison is conservative, not a semantic oracle.
pub(super) fn changed_fields(original: &Value, current: &Value) -> Vec<String> {
    let mut changed = Vec::new();
    for group in ["scope", "target"] {
        let before = &original[group];
        let after = &current[group];
        if group == "target" && before.is_null() {
            continue;
        }
        if before == after {
            continue;
        }
        if let (Some(before), Some(after)) = (before.as_object(), after.as_object()) {
            let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            for key in keys {
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
