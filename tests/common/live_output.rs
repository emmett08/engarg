//! Current model-output fixtures retain explicit unresolved computational methods.
//!
//! The source examples remain historical import/export fixtures. Their missing
//! method declarations must never be normalised by production authoring code.

#![allow(dead_code)]

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

/// Model responses use explicit nulls required by the strict generation schema.
/// Historical storage may omit these fields. Preserve supplied revision records.
pub fn generation_value(case: &impl Serialize) -> Value {
    let mut value = serde_json::to_value(case).expect("serialisable case fixture");
    value
        .as_object_mut()
        .expect("case fixture object")
        .entry("dissent_revisions")
        .or_insert(Value::Null);
    if let Some(warrants) = value["warrants"].as_array_mut() {
        for warrant in warrants {
            if let Some(method) = warrant
                .pointer_mut("/reasoning_method/method")
                .and_then(Value::as_object_mut)
                && let Some(query) = method.remove("query")
            {
                method.insert("query_json".into(), Value::String(query.to_string()));
            }
        }
    }
    value
}

pub fn value(source: &str) -> Value {
    let mut case: Value = serde_json::from_str(source).expect("valid case fixture");
    for warrant in case["warrants"].as_array_mut().expect("fixture warrants") {
        let method = warrant["inference"].as_str().expect("fixture inference");
        if matches!(
            method,
            "deductive" | "inductive" | "abductive" | "causal" | "counterfactual" | "analogical"
        ) && warrant.get("reasoning_method").is_none_or(Value::is_null)
        {
            warrant["reasoning_method"] = json!({
                "status": "unresolved",
                "method": method,
                "reasons": ["The fixture supplies no bounded formal model for this inference."],
                "evidence_need_ids": [],
            });
        }
    }
    case
}

pub fn case<T: DeserializeOwned>(source: &str) -> T {
    serde_json::from_value(value(source)).expect("typed current case fixture")
}

pub fn json(source: &str) -> String {
    value(source).to_string()
}
