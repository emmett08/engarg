//! Turn an explicit parameterised operation into a frozen, unit-aware function contract.
mod inputs;

use crate::{
    Error,
    support_requirements::{AcquisitionBlueprint, SupportElaboration},
    tooling::capability::CapabilityImplementationRequest,
};
use eal_collector_contracts::capability::SupportContract;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn output_schema(declared: &Value, contract: &SupportContract) -> Result<Value, Error> {
    if declared.get("type").and_then(Value::as_str) != Some("object") {
        return Err(invalid(
            "operation output schema requires an explicit object of logical quantities",
        ));
    }
    jsonschema::validator_for(declared)
        .map_err(|_| invalid("operation output schema is invalid"))?;
    let mut schema = declared.clone();
    let required = declared
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("operation output schema must require its logical quantities"))?;
    let properties = schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("operation output schema requires declared quantity properties"))?;
    for quantity in &contract.quantities {
        if !required
            .iter()
            .any(|name| name.as_str() == Some(&quantity.name))
        {
            return Err(invalid(format!(
                "operation output schema must require quantity {}",
                quantity.name
            )));
        }
        let value = properties
            .get_mut(&quantity.name)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                invalid(format!(
                    "operation output schema omits a schema for quantity {}",
                    quantity.name
                ))
            })?;
        if inputs::unit(&Value::Object(value.clone())).is_some_and(|unit| unit != quantity.unit) {
            return Err(invalid(format!(
                "operation output quantity {} has a conflicting unit",
                quantity.name
            )));
        }
        if value.contains_key("x-engarg-quantity") {
            return Err(invalid(
                "operation output cannot replace generated quantity annotations",
            ));
        }
        value.insert(
            "x-engarg-quantity".into(),
            json!({"name":quantity.name,"unit":quantity.unit}),
        );
    }
    // The collection bit belongs to the execution carrier. A logical quantity
    // with that name is preserved beneath measurements, never replaced.
    if contract
        .quantities
        .iter()
        .any(|quantity| quantity.name == "collected")
    {
        Ok(
            json!({"type":"object","properties":{"measurements":schema,"collected":{"type":"boolean"}},"required":["measurements","collected"],"additionalProperties":false}),
        )
    } else {
        if properties.contains_key("collected") {
            return Err(invalid(
                "operation output reserves collected for its execution carrier",
            ));
        }
        properties.insert("collected".into(), json!({"type":"boolean"}));
        schema
            .get_mut("required")
            .and_then(Value::as_array_mut)
            .expect("required validated")
            .push(json!("collected"));
        Ok(schema)
    }
}

/// Missing operation semantics are a clarification gap. Neither a method name
/// nor a consumer's subject description is enough to invent an implementation.
pub fn request(
    blueprint: &AcquisitionBlueprint,
    elaboration: &SupportElaboration,
) -> Result<CapabilityImplementationRequest, Error> {
    let spec = blueprint.operation_spec.as_ref().ok_or_else(|| {
        invalid(
            "missing operation implementation requires an explicit parameterised operation_spec",
        )
    })?;
    spec.validate().map_err(invalid)?;
    let mut contract = blueprint.contract.clone();
    contract.scope.selectors.clear();
    contract.scope.subject = format!("parameterised {} operation", contract.scope.kind);
    contract.time.historical_interval = None;
    // Each consumer retains its own runtime freshness and source interval.
    contract.time.max_age_seconds = None;
    contract.expected_output_schema_sha256 = None;
    let request = CapabilityImplementationRequest {
        support_contract: contract.clone(),
        input_schema: inputs::schema(blueprint, elaboration, &spec.input_schema)?,
        output_schema: output_schema(&spec.output_schema, &contract)?,
        operation_purpose: serde_json::to_string(&json!({
            "schema":spec.schema,"definition":spec.definition,
            "input_conditions":spec.input_conditions,"output_guarantees":spec.output_guarantees,
            "methods":contract.acceptable_methods,
            "provenance":"Original graph dependency provenance remains attached by the executor; implementations must not invent, replace or independently certify source evidence."
        }))?,
        declared_limits: BTreeMap::new(),
    };
    request.validate()?;
    Ok(request)
}

#[cfg(test)]
mod tests;
