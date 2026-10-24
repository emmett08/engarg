//! Intern repeated authoritative objection contexts within one generated prompt.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::Error;

const TABLE: &str = "retained_target_contexts";

/// Every reference resolves to a complete, exact context in the same request.
/// Keep the expanded form unless the final JSON request shrinks after paying
/// for the additional fixed instruction. This never edits source case data.
pub(in crate::engine) fn compact_target_contexts(
    data: &mut Value,
    instruction: &str,
) -> Result<bool, Error> {
    if data.get(TABLE).is_some() {
        return Ok(false);
    }
    let Some(objections) = data["retained_objections"].as_array() else {
        return Ok(false);
    };
    let mut contexts = BTreeMap::<String, (usize, &Value)>::new();
    for objection in objections {
        if let Some(context) = objection["identity"].get("target_context")
            && context.is_object()
        {
            let key = serde_json::to_string(context)?;
            contexts
                .entry(key)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, context));
        }
    }
    let mut references = BTreeMap::<String, String>::new();
    let mut table = serde_json::Map::new();
    for (key, (count, context)) in contexts {
        if count < 2 {
            continue;
        }
        let reference = format!("tc{}", table.len() + 1);
        // Exclude groups that cannot pay for their own table entry. The final
        // envelope check below additionally charges the table field and primer.
        let reference_bytes = serde_json::to_vec(&reference)?.len();
        let Some(expanded) = ("target_context".len() + 3 + key.len()).checked_mul(count) else {
            continue;
        };
        let Some(compact) = ("target_context_ref".len() + 3 + reference_bytes)
            .checked_mul(count)
            .and_then(|size| size.checked_add(reference_bytes + 2 + key.len()))
        else {
            continue;
        };
        if compact < expanded {
            references.insert(key, reference.clone());
            table.insert(reference, context.clone());
        }
    }
    if table.is_empty() {
        return Ok(false);
    }
    // CompletionRequest carries user context as a JSON string. Count that outer
    // escaping as well as the context document itself.
    let before = serde_json::to_vec(&serde_json::to_string(data)?)?.len();
    let mut candidate = data.clone();
    for objection in candidate["retained_objections"]
        .as_array_mut()
        .expect("checked objection list")
    {
        let Some(identity) = objection.get_mut("identity").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(context) = identity.get("target_context")
            && let Some(reference) = references.get(&serde_json::to_string(context)?)
        {
            identity.remove("target_context");
            identity.insert("target_context_ref".into(), json!(reference));
        }
    }
    candidate[TABLE] = Value::Object(table);
    // Adding a suffix to the system string has this exact JSON encoding cost.
    let instruction_bytes = serde_json::to_vec(&format!("\n\n{instruction}"))?.len() - 2;
    if serde_json::to_vec(&serde_json::to_string(&candidate)?)?
        .len()
        .saturating_add(instruction_bytes)
        >= before
    {
        return Ok(false);
    }
    *data = candidate;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(label: &str) -> Value {
        json!({"scope":{"configuration":format!("{label} {}", "unchanged supplied context ".repeat(25))},
            "target":{"id":"W1","premise_ids":["G1","G2"],"rule":"Exact warrant."},
            "related_targets":{"G1":{"statement":"Exact premise."}},
            "future":{"numeric":"1e-1000","quoted":"\"α\"\n"}})
    }

    fn envelope(contexts: &[Value]) -> Value {
        json!({"request":{"untrusted":{"retained_target_contexts":{"tc1":"literal source"}}},
            "retained_objections":contexts.iter().enumerate().map(|(index,context)|
                json!({"id":format!("critic-{index}"),"identity":{"target_id":"W1","target_context":context}})).collect::<Vec<_>>()})
    }

    // Independent reconstruction uses only the documented request-local table,
    // not the producer's grouping or reference selection implementation.
    fn expand(mut data: Value) -> Value {
        if let Some(table) = data.as_object_mut().unwrap().remove(TABLE) {
            for objection in data["retained_objections"].as_array_mut().unwrap() {
                let Some(identity) = objection.get_mut("identity").and_then(Value::as_object_mut)
                else {
                    continue;
                };
                if let Some(reference) = identity.remove("target_context_ref") {
                    identity.insert(
                        "target_context".into(),
                        table[reference.as_str().unwrap()].clone(),
                    );
                }
            }
        }
        data
    }

    #[test]
    fn complete_distinct_and_future_contexts_round_trip_exactly() {
        let first = context("one");
        let second = context("two");
        let original = envelope(&[first.clone(), second.clone(), first, second]);
        let mut compact = original.clone();
        assert!(compact_target_contexts(&mut compact, "Resolve exact contexts.").unwrap());
        assert_eq!(compact[TABLE].as_object().unwrap().len(), 2);
        let before = json!({"system":"Base", "user":serde_json::to_string(&original).unwrap()});
        let after = json!({"system":"Base\n\nResolve exact contexts.", "user":serde_json::to_string(&compact).unwrap()});
        assert!(
            serde_json::to_vec(&after).unwrap().len() < serde_json::to_vec(&before).unwrap().len()
        );
        assert_eq!(expand(compact), original);
    }

    #[test]
    fn exact_numbers_survive_context_interning() {
        let mut snapshot = context("exact numbers");
        snapshot["future"]["numeric"] =
            serde_json::from_str("1.2345678901234567890123456789e-1000").unwrap();
        let original = envelope(&[snapshot.clone(), snapshot]);
        let mut compact = original.clone();
        assert!(compact_target_contexts(&mut compact, "Resolve exact contexts.").unwrap());
        assert_eq!(
            serde_json::to_string(&expand(compact)).unwrap(),
            serde_json::to_string(&original).unwrap()
        );
    }

    #[test]
    fn unique_and_small_contexts_keep_their_original_representation() {
        for original in [
            envelope(&[context("unique")]),
            envelope(&[json!({}), json!({})]),
        ] {
            let mut candidate = original.clone();
            assert!(!compact_target_contexts(&mut candidate, "Resolve exact contexts.").unwrap());
            assert_eq!(candidate, original);
        }
    }

    #[test]
    fn unbound_and_unrelated_objection_fields_remain_unchanged() {
        let mut original = envelope(&[context("shared"), context("shared")]);
        original["retained_objections"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"unbound","identity":null}),
                json!({"id":"future","other":"Exact future field."}),
            ]);
        let mut compact = original.clone();
        assert!(compact_target_contexts(&mut compact, "Resolve exact contexts.").unwrap());
        assert_eq!(expand(compact), original);
    }

    #[test]
    fn primer_cost_and_existing_tables_cannot_inflate_or_reinterpret_context() {
        let original = envelope(&[context("shared"), context("shared")]);
        let mut candidate = original.clone();
        assert!(!compact_target_contexts(&mut candidate, &"long primer ".repeat(1000)).unwrap());
        assert_eq!(candidate, original);
        assert!(compact_target_contexts(&mut candidate, "Resolve exact contexts.").unwrap());
        let compact = candidate.clone();
        assert!(!compact_target_contexts(&mut candidate, "Resolve exact contexts.").unwrap());
        assert_eq!(candidate, compact);
    }

    #[test]
    fn unrelated_source_objects_cannot_activate_context_references() {
        let original = json!({"request":{"retained_objections":[{"identity":{"target_context":context("source")}}],TABLE:{"tc1":"source"}}});
        let mut candidate = original.clone();
        assert!(!compact_target_contexts(&mut candidate, "Resolve exact contexts.").unwrap());
        assert_eq!(candidate, original);
    }
}
