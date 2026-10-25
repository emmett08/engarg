//! Preserve declared input schemas and the units carried by graph dependencies.
use crate::{
    Error,
    support_requirements::{AcquisitionBlueprint, SupportElaboration, logical_output_schema},
};
use eal_collector_contracts::execution::OutputReference;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

pub(super) fn unit(schema: &Value) -> Option<&str> {
    schema
        .pointer("/x-engarg-quantity/unit")
        .and_then(Value::as_str)
        .or_else(|| schema.get("x-engarg-unit").and_then(Value::as_str))
        .or_else(|| {
            (schema
                .pointer("/properties/schema/const")
                .and_then(Value::as_str)
                == Some("eal.exact-number/1"))
            .then(|| {
                schema
                    .pointer("/properties/unit/const")
                    .and_then(Value::as_str)
            })
            .flatten()
        })
        .or_else(|| schema.get("items").and_then(unit))
}

fn types(schema: &Value) -> Option<BTreeSet<String>> {
    match schema.get("type")? {
        Value::String(value) => Some(BTreeSet::from([value.clone()])),
        Value::Array(values) => values
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect(),
        _ => None,
    }
}

fn known_compatible(expected: &Value, source: &Value) -> bool {
    let (Some(expected), Some(source)) = (types(expected), types(source)) else {
        return true;
    };
    source
        .iter()
        .all(|kind| expected.contains(kind) || kind == "integer" && expected.contains("number"))
}

fn schema_pointer(schema: &Value, pointer: &str) -> Option<String> {
    let mut current = schema;
    let mut result = String::new();
    for segment in pointer.strip_prefix('/')?.split('/') {
        let name = segment.replace("~1", "/").replace("~0", "~");
        if current.get("type").and_then(Value::as_str) == Some("array") {
            name.parse::<usize>().ok()?;
            current = current.get("items")?;
            result.push_str("/items");
        } else {
            current = current.get("properties")?.get(&name)?;
            result.push_str("/properties/");
            result.push_str(&name.replace('~', "~0").replace('/', "~1"));
        }
    }
    Some(result)
}

fn below(pointer: &str, ancestor: &str) -> bool {
    ancestor.is_empty()
        || pointer == ancestor
        || pointer
            .strip_prefix(ancestor)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn item_schema(mut schema: Value) -> Result<Value, Error> {
    match schema.get("type").and_then(Value::as_str) {
        Some("array") => {
            let mut item = schema.get("items").cloned().unwrap_or_else(|| json!({}));
            if item.get("x-engarg-quantity").is_none()
                && let Some(quantity) = schema.get("x-engarg-quantity")
            {
                item.as_object_mut()
                    .ok_or_else(|| invalid("dependency item schema must be an object"))?
                    .insert("x-engarg-quantity".into(), quantity.clone());
            }
            Ok(item)
        }
        Some(_) => Err(invalid(
            "for_each implementation input is selected from a known non-array source",
        )),
        None => {
            schema
                .as_object_mut()
                .ok_or_else(|| invalid("dependency schema must be an object"))?
                .insert(
                    "x-engarg-cardinality".into(),
                    json!("one item selected from a runtime-checked source collection"),
                );
            Ok(schema)
        }
    }
}

fn annotate(schema: &mut Value, pointer: &str, source: Value) -> Result<(), Error> {
    let target = if pointer.is_empty() {
        schema
    } else {
        let physical = schema_pointer(schema, pointer).ok_or_else(|| {
            invalid(format!(
                "operation input schema does not declare dependency destination {pointer}"
            ))
        })?;
        schema
            .pointer_mut(&physical)
            .ok_or_else(|| invalid("operation input schema destination disappeared"))?
    };
    if !known_compatible(target, &source) {
        return Err(invalid(format!(
            "operation input schema conflicts with the known dependency value shape at {pointer}"
        )));
    }
    if let Some(expected) = unit(target)
        && unit(&source).is_some_and(|actual| actual != expected)
    {
        return Err(invalid(format!(
            "operation input unit differs from its dependency at {pointer}; add an explicit conversion"
        )));
    }
    let target = target
        .as_object_mut()
        .ok_or_else(|| invalid("operation dependency destinations require object schemas"))?;
    if target.contains_key("x-engarg-source-contract") {
        return Err(invalid(
            "operation input schema cannot replace generated dependency contract annotations",
        ));
    }
    target.insert("x-engarg-source-contract".into(), source);
    Ok(())
}

/// The logical schemas are fixed by the operation definition. Dependency metadata
/// supplies units and known cardinality without importing a consumer's source IDs.
pub(super) fn schema(
    blueprint: &AcquisitionBlueprint,
    elaboration: &SupportElaboration,
    declared: &Value,
) -> Result<Value, Error> {
    if declared.get("type").and_then(Value::as_str) != Some("object")
        || !declared.get("properties").is_some_and(Value::is_object)
    {
        return Err(invalid(
            "operation input schema requires an explicit object and properties",
        ));
    }
    let validator = jsonschema::validator_for(declared)
        .map_err(|_| invalid("operation input schema is invalid"))?;
    let deferred = blueprint
        .bindings
        .iter()
        .map(|binding| binding.argument_pointer.as_str())
        .chain(
            blueprint
                .for_each
                .iter()
                .map(|binding| binding.argument_pointer.as_str()),
        )
        .collect::<Vec<_>>();
    if let Some(error) = validator.iter_errors(&blueprint.arguments).find(|error| {
        !deferred
            .iter()
            .any(|pointer| below(&error.instance_path().to_string(), pointer))
    }) {
        return Err(invalid(format!(
            "operation arguments differ from its declared input schema at {}",
            error.instance_path()
        )));
    }
    let mut schema = declared.clone();
    let mut annotated = BTreeSet::new();
    let mut bind =
        |pointer: &str, reference: &OutputReference, foreach: bool| -> Result<(), Error> {
            if !annotated.insert(pointer.to_owned()) {
                return Err(invalid(format!(
                    "operation input {pointer} has conflicting dependency bindings"
                )));
            }
            let source = logical_output_schema(elaboration, reference).map_err(invalid)?;
            annotate(
                &mut schema,
                pointer,
                if foreach {
                    item_schema(source)?
                } else {
                    source
                },
            )
        };
    for binding in &blueprint.bindings {
        bind(&binding.argument_pointer, &binding.output, false)?;
    }
    if let Some(binding) = &blueprint.for_each {
        bind(&binding.argument_pointer, &binding.output, true)?;
    }
    Ok(schema)
}
