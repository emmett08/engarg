//! Current model-output fixtures retain explicit unresolved computational methods.
//!
//! The source examples remain historical import/export fixtures. Their missing
//! method declarations must never be normalised by production authoring code.

#![allow(dead_code)]

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub fn value(source: &str) -> Value {
    let mut case: Value = serde_json::from_str(source).expect("valid case fixture");
    for warrant in case["warrants"].as_array_mut().expect("fixture warrants") {
        let method = warrant["inference"].as_str().expect("fixture inference");
        if matches!(
            method,
            "deductive" | "inductive" | "abductive" | "causal" | "counterfactual"
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
