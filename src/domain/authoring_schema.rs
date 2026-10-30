//! Adapt the canonical argument schema to the strict model-authoring dialect.
use super::ArgumentCase;

pub fn case_schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ArgumentCase))
        .expect("a statically derived schema is serialisable");
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
    }
    schema["$defs"]["MethodSpecification"] = super::reasoning::method_specification_schema();
    supported_schema(&mut schema);
    // New generation must make the computational/unresolved choice explicit.
    // Deserialising historical reports continues to admit the absent field.
    let warrant = schema["$defs"]["Warrant"].take();
    let mut computational = warrant.clone();
    computational["properties"]["inference"] = serde_json::json!({"type":"string",
        "enum":["deductive","inductive","abductive","causal","counterfactual","analogical"]});
    computational["properties"]["reasoning_method"] =
        serde_json::json!({"$ref":"#/$defs/ReasoningMethodBinding"});
    let mut authored = warrant;
    authored["properties"]["inference"] = serde_json::json!({"type":"string",
        "enum":["practical"]});
    authored["properties"]["reasoning_method"] = serde_json::json!({"type":"null"});
    schema["$defs"]["Warrant"] = serde_json::json!({"anyOf":[computational,authored]});
    schema
}

/// Schemars emits disjoint tagged unions as oneOf. The provider's strict
/// structured-output dialect uses anyOf, requires every object property and
/// represents optional values through an explicit nullable alternative.
fn supported_schema(value: &mut serde_json::Value) {
    use serde_json::{Value, json};
    match value {
        Value::Object(object) => {
            object.remove("default");
            object.remove("format");
            // The strict provider dialect disallows annotation siblings on a
            // reference; the referenced definition retains its documentation.
            if object.get("$ref").is_some_and(Value::is_string) {
                object.remove("description");
                object.remove("title");
            }
            if let Some(variants) = object.remove("oneOf") {
                object.insert("anyOf".into(), variants);
            }
            if let Some(constant) = object.remove("const") {
                object.insert("enum".into(), json!([constant]));
            }
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                let keys: Vec<_> = properties.keys().cloned().map(Value::String).collect();
                object.insert("required".into(), Value::Array(keys));
                object.insert("additionalProperties".into(), Value::Bool(false));
            }
            for child in object.values_mut() {
                supported_schema(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                supported_schema(child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn strict_references_keep_documentation_on_their_definitions() {
        fn check(value: &Value) {
            match value {
                Value::Object(object) => {
                    if object.get("$ref").is_some_and(Value::is_string) {
                        assert_eq!(object.len(), 1, "strict provider reference: {object:?}");
                    }
                    object.values().for_each(check);
                }
                Value::Array(array) => array.iter().for_each(check),
                _ => {}
            }
        }
        let schema = case_schema();
        check(&schema);
        assert!(schema["$defs"]["MethodClaimMode"]["description"].is_string());
    }
}
