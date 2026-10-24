//! Read operator-selected collector descriptions without granting execution authority.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, acquisition::AcquisitionPlan};

pub const SCHEMA: &str = "eal.collector-catalogue/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectorCatalogue {
    pub schema: String,
    pub acquisition_contract: String,
    pub collectors: Vec<CollectorDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectorDescriptor {
    pub id: String,
    pub version: String,
    pub purpose: String,
    pub parameters: Vec<String>,
    pub input_schema: Value,
    pub result_schema: Value,
    pub command: CommandBinding,
    pub acquisition_predicate: String,
    #[serde(default)]
    pub constraints: Vec<WindowConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowConstraint {
    pub kind: String,
    pub start: String,
    pub end: String,
    pub max_seconds: u64,
}

/// Operator-owned command template. Model responses can only fill parameter tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandBinding {
    pub argv: Vec<String>,
    pub inherit_env: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub parallel_safe: bool,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && value.len() <= 128
}

fn placeholder(value: &str) -> Option<&str> {
    value.strip_prefix('{')?.strip_suffix('}')
}

impl CollectorCatalogue {
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 1_048_576 {
            return Err(Error::Budget("collector catalogue exceeds 1 MiB".into()));
        }
        let value = crate::retained_collectors::parse_json(bytes)?;
        let catalogue: Self = serde_json::from_value(value)?;
        catalogue.validate()?;
        Ok(catalogue)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA || self.acquisition_contract != "eal.acquisition/1" {
            return Err(invalid("unsupported collector catalogue contract"));
        }
        if self.collectors.len() > 128 {
            return Err(invalid("collector catalogue exceeds 128 entries"));
        }
        let mut ids = BTreeSet::new();
        for collector in &self.collectors {
            if [
                "custom",
                "artifact_json",
                "http_json_get",
                "scoped_json",
                "kubectl_get",
                "gh_api_get",
            ]
            .contains(&collector.id.as_str())
            {
                return Err(invalid(
                    "collector ID is reserved by an existing EngArg adapter",
                ));
            }
            if !identifier(&collector.id) || !ids.insert(&collector.id) {
                return Err(invalid("collector IDs must be unique bounded identifiers"));
            }
            collector.validate()?;
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&CollectorDescriptor> {
        self.collectors.iter().find(|collector| collector.id == id)
    }

    /// Descriptions include schemas, but executable paths and environment names stay local.
    pub fn planning_view(&self) -> Value {
        json!(
            self.collectors
                .iter()
                .map(|collector| json!({
                    "id": collector.id, "version": collector.version,
                    "purpose": collector.purpose, "arguments": collector.parameters,
                    "input_schema": collector.input_schema,
                }))
                .collect::<Vec<_>>()
        )
    }
}

impl CollectorDescriptor {
    fn validate(&self) -> Result<(), Error> {
        if self.version.is_empty()
            || self.version.len() > 128
            || self.purpose.trim().is_empty()
            || self.parameters.len() > 64
        {
            return Err(invalid(
                "collector requires bounded version, purpose and parameters",
            ));
        }
        let mut parameters = BTreeSet::new();
        if self
            .parameters
            .iter()
            .any(|p| !identifier(p) || !parameters.insert(p))
        {
            return Err(invalid("collector parameters must be unique identifiers"));
        }
        if self.input_schema["type"] != "object"
            || self.input_schema["additionalProperties"] != false
            || !self.input_schema["properties"].is_object()
        {
            return Err(invalid(
                "collector input schema must describe a closed object",
            ));
        }
        for key in self.input_schema["properties"]
            .as_object()
            .expect("checked object")
            .keys()
        {
            if !self.parameters.contains(key) {
                return Err(invalid("collector input field is absent from parameters"));
            }
        }
        // External references could perform IO during schema evaluation. This protocol uses local schemas.
        fn external_ref(value: &Value) -> bool {
            match value {
                Value::Object(object) => object.iter().any(|(key, value)| {
                    ((key == "$ref" || key == "$dynamicRef")
                        && value.as_str().is_none_or(|s| !s.starts_with('#')))
                        || external_ref(value)
                }),
                Value::Array(values) => values.iter().any(external_ref),
                _ => false,
            }
        }
        if external_ref(&self.input_schema) || external_ref(&self.result_schema) {
            return Err(invalid("collector schemas must use local references"));
        }
        jsonschema::validator_for(&self.input_schema)
            .map_err(|_| invalid("invalid collector input schema"))?;
        jsonschema::validator_for(&self.result_schema)
            .map_err(|_| invalid("invalid collector result schema"))?;
        self.command.validate()?;
        for token in &self.command.argv {
            if let Some(name) = placeholder(token) {
                if let Some(operator) = name.strip_prefix("operator.") {
                    if !identifier(operator) {
                        return Err(invalid("invalid operator placeholder"));
                    }
                } else if !self.parameters.iter().any(|p| p == name) {
                    return Err(invalid("command template names an unknown parameter"));
                }
            } else if token.contains(['{', '}']) {
                return Err(invalid(
                    "command placeholders must occupy an entire argument",
                ));
            }
        }
        if placeholder(&self.command.argv[0]).is_some_and(|p| !p.starts_with("operator.")) {
            return Err(invalid("collector parameters cannot choose the executable"));
        }
        checked_predicate(&self.acquisition_predicate)?;
        for constraint in &self.constraints {
            if constraint.kind != "ordered_window"
                || constraint.max_seconds == 0
                || !self.parameters.contains(&constraint.start)
                || !self.parameters.contains(&constraint.end)
                || self.input_schema["properties"][&constraint.start]["type"] != "integer"
                || self.input_schema["properties"][&constraint.end]["type"] != "integer"
            {
                return Err(invalid("unsupported collector constraint"));
            }
        }
        Ok(())
    }

    pub fn input(&self, plan: &AcquisitionPlan) -> Result<Value, Error> {
        let arguments: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.as_str(), arg.value.as_str()))
            .collect();
        let mut input = serde_json::Map::new();
        let mut partial_schema = self.input_schema.clone();
        fn string_property(schema: &Value, root: &Value, remaining: u8) -> bool {
            if schema["type"] == "string" {
                return true;
            }
            if remaining == 0 {
                return false;
            }
            schema
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix('#'))
                .and_then(|pointer| root.pointer(pointer))
                .is_some_and(|target| string_property(target, root, remaining - 1))
        }
        for (field, schema) in self.input_schema["properties"]
            .as_object()
            .ok_or_else(|| invalid("missing input properties"))?
        {
            let value = match arguments.get(field.as_str()) {
                Some(value) if string_property(schema, &self.input_schema, 16) => json!(value),
                Some(value) => {
                    crate::retained_collectors::parse_json(value.as_bytes()).map_err(|_| {
                        invalid(format!(
                            "collector input {field} requires encoded JSON of its declared type"
                        ))
                    })?
                }
                None => Value::Null,
            };
            if !arguments.contains_key(field.as_str()) && plan.unresolved_arguments.contains(field)
            {
                // Only this unresolved field is exempt. Every supplied value remains checked
                // in the original root schema context, including local definitions.
                partial_schema["properties"][field] = json!({});
            }
            input.insert(field.clone(), value);
        }
        let input = Value::Object(input);
        let validator = jsonschema::validator_for(&partial_schema)
            .map_err(|_| invalid("invalid collector input schema"))?;
        if !validator.is_valid(&input) {
            return Err(invalid("collector input violates its schema"));
        }
        for constraint in &self.constraints {
            let endpoint = |field: &str| -> Result<Option<u64>, Error> {
                if !arguments.contains_key(field)
                    && plan.unresolved_arguments.iter().any(|name| name == field)
                {
                    Ok(None)
                } else {
                    input[field].as_u64().map(Some).ok_or_else(|| {
                        invalid("collector window endpoints require unsigned 64-bit seconds")
                    })
                }
            };
            if let (Some(start), Some(end)) =
                (endpoint(&constraint.start)?, endpoint(&constraint.end)?)
                && (end <= start || end - start > constraint.max_seconds)
            {
                return Err(invalid(
                    "collector window must be ordered and within its declared bound",
                ));
            }
        }
        Ok(input)
    }

    pub fn binding(&self, plan: &AcquisitionPlan) -> Result<RegistryCommand, Error> {
        let arguments: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.as_str(), arg.value.as_str()))
            .collect();
        let argv = self
            .command
            .argv
            .iter()
            .map(|token| match placeholder(token) {
                Some(name) if name.starts_with("operator.") => {
                    format!("/REPLACE_{}", name[9..].to_uppercase())
                }
                Some(name) => arguments
                    .get(name)
                    .map(|s| (*s).to_owned())
                    .unwrap_or_else(|| format!("REPLACE_{}", name.to_uppercase())),
                None => token.clone(),
            })
            .collect();
        Ok(RegistryCommand {
            kind: "command".into(),
            version: self.version.clone(),
            argv,
            inherit_env: self.command.inherit_env.clone(),
            timeout_seconds: self.command.timeout_seconds,
            max_output_bytes: self.command.max_output_bytes,
            parallel_safe: self.command.parallel_safe,
        })
    }
}

impl CommandBinding {
    fn validate(&self) -> Result<(), Error> {
        if self.argv.is_empty()
            || self.argv.len() > 256
            || self
                .argv
                .iter()
                .any(|s| s.is_empty() || s.len() > 8192 || s.chars().any(char::is_control))
            || self.inherit_env.len() > 64
            || self.inherit_env.iter().any(|name| !identifier(name))
            || !(1..=3600).contains(&self.timeout_seconds)
            || !(1..=16_777_216).contains(&self.max_output_bytes)
        {
            return Err(invalid("invalid bounded collector command template"));
        }
        Ok(())
    }
}

/// A single comparison prevents descriptor text from introducing EAL declarations.
fn checked_predicate(text: &str) -> Result<(), Error> {
    let parts: Vec<_> = text.splitn(3, ' ').collect();
    if parts.len() != 3
        || !parts[0].split('.').all(identifier)
        || !["==", "!=", ">=", "<=", ">", "<"].contains(&parts[1])
        || text.chars().any(char::is_control)
    {
        return Err(invalid(
            "collector acquisition predicate must be one scalar comparison",
        ));
    }
    let value: Value = serde_json::from_str(parts[2])
        .map_err(|_| invalid("invalid predicate comparison value"))?;
    if value.is_array() || value.is_object() || value.is_null() {
        return Err(invalid(
            "collector acquisition predicate requires a scalar value",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCommand {
    pub kind: String,
    pub version: String,
    pub argv: Vec<String>,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub inherit_env: Vec<String>,
    pub parallel_safe: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsRegistry {
    pub tools: BTreeMap<String, RegistryCommand>,
}

impl ToolsRegistry {
    pub fn to_toml(&self) -> Result<String, Error> {
        toml::to_string_pretty(self)
            .map_err(|error| invalid(format!("cannot serialise collector registry: {error}")))
    }
}
