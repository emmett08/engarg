//! Bounded structural interfaces let the critic recognise batch composition.
use super::RoutePlan;
use eal_collector_contracts::capability::CapabilityCatalogue;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const MAX_INTERFACES: usize = 32;
const MAX_BYTES: usize = 65_536;

// Descriptions, examples, defaults, constants and external references can
// contain operator values. The critic receives shape information only.
fn shape(schema: &Value, remaining: &mut usize, depth: usize) -> Value {
    if *remaining == 0 || depth == 0 {
        return json!({"shape_omitted":true});
    }
    *remaining -= 1;
    let mut result = Map::new();
    if let Some(value) = schema.as_bool() {
        return json!(value);
    }
    for key in ["type", "required"] {
        if let Some(value) = schema.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    for key in ["properties", "$defs"] {
        if let Some(fields) = schema.get(key).and_then(Value::as_object) {
            let mut children = Map::new();
            for (name, value) in fields {
                if *remaining == 0 {
                    break;
                }
                children.insert(name.clone(), shape(value, remaining, depth - 1));
            }
            if children.len() != fields.len() {
                result.insert("shape_truncated".into(), json!(true));
            }
            result.insert(key.into(), Value::Object(children));
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(value) = schema.get(key) {
            result.insert(key.into(), shape(value, remaining, depth - 1));
        }
    }
    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(values) = schema.get(key).and_then(Value::as_array) {
            let mut children = Vec::new();
            for value in values {
                if *remaining == 0 {
                    break;
                }
                children.push(shape(value, remaining, depth - 1));
            }
            if children.len() != values.len() {
                result.insert("shape_truncated".into(), json!(true));
            }
            result.insert(key.into(), json!(children));
        }
    }
    if let Some(reference) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with('#'))
    {
        result.insert("$ref".into(), json!(reference));
    }
    Value::Object(result)
}

pub(super) fn alternatives(catalogue: &CapabilityCatalogue, plan: &RoutePlan) -> Value {
    let mut seen = BTreeSet::new();
    let mut interfaces = Vec::new();
    let mut bytes = 0;
    let mut omitted = 0;
    let ids = plan
        .projections
        .iter()
        .map(|route| &route.capability.id)
        .chain(
            plan.routes
                .iter()
                .flat_map(|route| route.ranked.iter().map(|rank| &rank.capability_id)),
        );
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(capability) = catalogue.capabilities.iter().find(|cap| cap.id == *id) else {
            continue;
        };
        let interface = json!({"capability_id":capability.id,"version":capability.version,
            "input_schema_sha256":capability.input_schema_sha256,"output_schema_sha256":capability.output_schema_sha256,
            "input_shape":shape(&capability.input_schema,&mut 128,8),
            "output_shape":shape(&capability.output_schema,&mut 128,8),
            "semantics":capability.semantics,"limits":capability.limits});
        let size = serde_json::to_vec(&interface)
            .expect("interface serialises")
            .len();
        if interfaces.len() >= MAX_INTERFACES || bytes + size > MAX_BYTES {
            omitted += 1;
            continue;
        }
        bytes += size;
        interfaces.push(interface);
    }
    json!({"interfaces":interfaces,"omitted_interfaces":omitted,
        "limitations":"Shapes omit predicates, constant/default/example values, descriptions and external schema references. They are composition hints, not full contracts. Deterministic route admission and runtime validation still require the exact qualified schemas."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_interfaces_remain_visible_without_configuration_literals() {
        let schema = json!({"type":"object","properties":{"ids":{"type":"array","items":{"type":"string"},"default":["PRIVATE"]},"token":{"type":"string","const":"PRIVATE"}},"required":["ids"],"description":"PRIVATE","$ref":"https://PRIVATE/schema"});
        let summary = shape(&schema, &mut 128, 8);
        assert_eq!(summary["properties"]["ids"]["items"]["type"], "string");
        assert!(!summary.to_string().contains("PRIVATE"));
        let truncated = shape(&schema, &mut 1, 8);
        assert_eq!(truncated["shape_truncated"], true);
    }
}
