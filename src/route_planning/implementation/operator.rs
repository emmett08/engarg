//! Apply explicit operator definitions without changing retained authoring history.
use super::{invalid, request};
use crate::{
    Error,
    route_planning::PlanningConfig,
    support_requirements::{AcquisitionBlueprint, SupportElaboration},
    tooling::capability::CapabilityImplementationRequest,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub(in crate::route_planning) fn configured_request(
    blueprint: &AcquisitionBlueprint,
    elaboration: &SupportElaboration,
    config: &PlanningConfig,
) -> Result<CapabilityImplementationRequest, Error> {
    let methods = &blueprint.contract.acceptable_methods;
    if !methods
        .iter()
        .any(|method| config.operation_specifications.contains_key(method))
    {
        return request(blueprint, elaboration);
    }
    if methods.len() != 1 {
        return Err(invalid(
            "An operator operation specification requires one exact acceptable method",
        ));
    }
    let mut pinned = config.operation_specifications[&methods[0]].clone();
    let authored = blueprint.operation_spec.as_ref().ok_or_else(|| {
        invalid(
            "An operator operation specification requires the authored input and output schemas",
        )
    })?;
    // Admit the original units, source bindings and arguments before applying
    // operator authority. Pinning must not hide an invalid authored interface.
    request(blueprint, elaboration)?;
    for (label, original, supplied) in [
        ("input", &authored.input_schema, &mut pinned.input_schema),
        ("output", &authored.output_schema, &mut pinned.output_schema),
    ] {
        let (original, old_units) = validation_schema(original)?;
        let (validation, new_units) = validation_schema(supplied)?;
        if original != validation
            || old_units
                .iter()
                .any(|(path, unit)| new_units.get(path).is_some_and(|new| new != unit))
        {
            return Err(invalid(format!(
                "Operator operation specification changes the authored {label} schema or units"
            )));
        }
        for (path, unit) in old_units {
            if !new_units.contains_key(&path) {
                let mut target = &mut *supplied;
                for segment in path {
                    target = match target {
                        Value::Array(items) => {
                            &mut items[segment.parse::<usize>().expect("schema index")]
                        }
                        Value::Object(properties) => {
                            properties.get_mut(&segment).expect("unchanged schema path")
                        }
                        _ => unreachable!("unchanged schema path"),
                    };
                }
                target
                    .as_object_mut()
                    .expect("unit schema object")
                    .insert("x-engarg-unit".into(), Value::String(unit));
            }
        }
    }
    let mut selected = blueprint.clone();
    selected.operation_spec = Some(pinned);
    request(&selected, elaboration)
}

/// Unit annotations carry contract metadata. Every JSON Schema constraint and
/// every literal value stays exact, including properties named x-engarg-unit.
fn validation_schema(schema: &Value) -> Result<(Value, BTreeMap<Vec<String>, String>), Error> {
    fn visit(
        value: &mut Value,
        path: &mut Vec<String>,
        units: &mut BTreeMap<Vec<String>, String>,
    ) -> Result<(), Error> {
        let Some(object) = value.as_object_mut() else {
            return Ok(());
        };
        if let Some(annotation) = object.remove("x-engarg-unit") {
            let unit = annotation
                .as_str()
                .filter(|unit| {
                    !unit.trim().is_empty()
                        && unit.len() <= 256
                        && !unit.chars().any(char::is_control)
                })
                .ok_or_else(|| {
                    invalid("Operation schema unit annotations require bounded unit names")
                })?;
            units.insert(path.clone(), unit.to_owned());
        }
        for (keyword, child) in object {
            path.push(keyword.clone());
            match keyword.as_str() {
                "properties" | "patternProperties" | "$defs" | "definitions"
                | "dependentSchemas" => {
                    if let Some(properties) = child.as_object_mut() {
                        for (name, schema) in properties {
                            path.push(name.clone());
                            visit(schema, path, units)?;
                            path.pop();
                        }
                    }
                }
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                    if let Some(schemas) = child.as_array_mut() {
                        for (index, schema) in schemas.iter_mut().enumerate() {
                            path.push(index.to_string());
                            visit(schema, path, units)?;
                            path.pop();
                        }
                    }
                }
                "items"
                | "additionalItems"
                | "additionalProperties"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "propertyNames"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else"
                | "contentSchema" => {
                    visit(child, path, units)?;
                }
                _ => {}
            }
            path.pop();
        }
        Ok(())
    }
    let mut schema = schema.clone();
    let mut units = BTreeMap::new();
    visit(&mut schema, &mut Vec::new(), &mut units)?;
    Ok((schema, units))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unit_metadata_does_not_erase_literal_data_or_property_names() {
        let schema = json!({"type":"object","properties":{
            "x-engarg-unit":{"type":"string","const":"a literal property"},
            "payload":{"const":{"x-engarg-unit":"literal data"},"x-engarg-unit":"json"}
        },"required":["x-engarg-unit","payload"],"additionalProperties":false});
        let (validation, units) = validation_schema(&schema).unwrap();
        assert_eq!(
            validation["properties"]["x-engarg-unit"],
            schema["properties"]["x-engarg-unit"]
        );
        assert_eq!(
            validation["properties"]["payload"]["const"],
            schema["properties"]["payload"]["const"]
        );
        assert!(
            validation["properties"]["payload"]
                .get("x-engarg-unit")
                .is_none()
        );
        assert_eq!(units.len(), 1);
    }
}
