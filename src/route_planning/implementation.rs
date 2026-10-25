//! A missing operation produces a parameterised function contract, never claim evidence.
use crate::{
    Error, support_requirements::AcquisitionBlueprint,
    tooling::capability::CapabilityImplementationRequest,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn interface(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            json!({"type":"object","properties":fields.iter().map(|(key,value)|(key.clone(),interface(value))).collect::<BTreeMap<_,_>>(),"required":fields.keys().collect::<Vec<_>>(),"additionalProperties":false})
        }
        Value::Array(values) => {
            let schemas = values
                .iter()
                .map(interface)
                .map(|schema| (schema.to_string(), schema))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect::<Vec<_>>();
            let items = match schemas.len() {
                0 => json!({}),
                1 => schemas[0].clone(),
                _ => json!({"anyOf":schemas}),
            };
            json!({"type":"array","items":items})
        }
        Value::String(_) => json!({"type":"string"}),
        Value::Number(number) => {
            let text = number.to_string();
            let integer = text
                .strip_prefix('-')
                .unwrap_or(&text)
                .bytes()
                .all(|byte| byte.is_ascii_digit());
            json!({"type":if integer{"integer"}else{"number"}})
        }
        Value::Bool(_) => json!({"type":"boolean"}),
        Value::Null => json!({}),
    }
}
pub fn request(blueprint: &AcquisitionBlueprint) -> Result<CapabilityImplementationRequest, Error> {
    let mut contract = blueprint.contract.clone();
    contract.scope.selectors.clear();
    contract.scope.subject = format!("parameterised {} operation", contract.scope.kind);
    contract.time.historical_interval = None;
    // Freshness is a consumer evaluation, not an implementation cache boundary.
    contract.time.max_age_seconds = None;
    contract.expected_output_schema_sha256 = None;
    let mut quantities = contract
        .quantities
        .iter()
        .map(|quantity| (quantity.name.clone(), json!({})))
        .collect::<BTreeMap<_, _>>();
    if quantities.contains_key("collected") {
        let payload = json!({"type":"object","properties":quantities,"required":quantities.keys().collect::<Vec<_>>(),"additionalProperties":false});
        quantities = BTreeMap::from([("measurements".into(), payload)]);
    } else if quantities.is_empty() {
        quantities.insert("value".into(), json!({}));
    }
    quantities.insert("collected".into(), json!({"type":"boolean"}));
    let output_schema = json!({"type":"object","properties":quantities,"required":quantities.keys().collect::<Vec<_>>(),"additionalProperties":false});
    let operation_purpose = format!(
        "Implement a parameterised {:?} operation for subject kind {} using qualified method identifiers {:?}; return the declared quantities with their original units, timestamps and provenance. Report unavailable input/source as errors; never invent measurements. Scientific qualification is separate from these function fixtures.",
        contract.support_type, contract.scope.kind, contract.acceptable_methods
    );
    Ok(CapabilityImplementationRequest {
        support_contract: contract,
        input_schema: interface(&blueprint.arguments),
        output_schema,
        operation_purpose,
        declared_limits: BTreeMap::new(),
    })
}
