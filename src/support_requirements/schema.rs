//! Strict provider envelopes retain typed validation for decoded support JSON.
use super::{SupportElaboration, SupportedArgument};
use crate::{
    Error,
    domain::{ArgumentCase, case_schema},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn elaboration_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(SupportElaboration))
        .expect("support schema serialises");
    schema
        .as_object_mut()
        .expect("schema object")
        .remove("$schema");
    schema["properties"]["schema"] =
        json!({"type":"string", "enum":[super::SUPPORT_ELABORATION_SCHEMA]});
    schema["$defs"]["SupportContract"]["properties"]["schema"] = json!({
        "type":"string", "enum":[eal_collector_contracts::capability::SUPPORT_CONTRACT_SCHEMA]
    });
    schema["$defs"]["OperationSpecification"]["properties"]["schema"] = json!({
        "type":"string", "enum":[super::OPERATION_SPECIFICATION_SCHEMA]
    });
    schema
}

pub fn envelope_schema() -> Value {
    let mut schema = case_schema();
    let root = schema.as_object_mut().expect("case schema object");
    let definitions = root.remove("$defs").expect("case definitions");
    let mut definitions = definitions.as_object().expect("definitions object").clone();
    definitions.insert("ArgumentCase".into(), Value::Object(root.clone()));
    json!({"type":"object","additionalProperties":false,
        "properties":{"argument":{"$ref":"#/$defs/ArgumentCase"},"support_elaboration_json":{"type":"string"}},
        "required":["argument","support_elaboration_json"],"$defs":definitions})
}

pub fn parse_supported_argument(
    content: &str,
    maximum_bytes: usize,
) -> Result<SupportedArgument, Error> {
    let value = eal_collector_contracts::strict_json_exact(
        content.as_bytes(),
        eal_collector_contracts::JsonLimits {
            max_bytes: maximum_bytes,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidOutput)?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        argument: ArgumentCase,
        support_elaboration_json: String,
    }
    let envelope: Envelope = serde_json::from_value(value).map_err(|error| {
        Error::InvalidOutput(format!("invalid support response envelope: {error}"))
    })?;
    let elaboration = eal_collector_contracts::strict_json_exact(
        envelope.support_elaboration_json.as_bytes(),
        eal_collector_contracts::JsonLimits {
            max_bytes: maximum_bytes,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidOutput)?;
    let support_elaboration = serde_json::from_value(elaboration)
        .map_err(|error| Error::InvalidOutput(format!("invalid support elaboration: {error}")))?;
    Ok(SupportedArgument {
        argument: envelope.argument,
        support_elaboration,
    })
}
