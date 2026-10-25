//! Bounded operation semantics freeze function inputs before implementation.
use super::{OPERATION_SPECIFICATION_SCHEMA, OperationSpecification};

impl OperationSpecification {
    pub fn validate(&self) -> Result<(), String> {
        let text = |value: &str| {
            !value.trim().is_empty()
                && value.len() <= 8192
                && !value
                    .chars()
                    .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        };
        if self.schema != OPERATION_SPECIFICATION_SCHEMA
            || !text(&self.definition)
            || self.input_conditions.len() > 128
            || self.output_guarantees.is_empty()
            || self.output_guarantees.len() > 128
            || self
                .input_conditions
                .iter()
                .chain(&self.output_guarantees)
                .any(|value| !text(value))
        {
            return Err("operation_spec requires a supported schema and bounded explicit definition, conditions and output guarantees".into());
        }
        for schema in [&self.input_schema, &self.output_schema] {
            if !schema.is_object()
                || serde_json::to_vec(schema)
                    .map_err(|error| error.to_string())?
                    .len()
                    > 262_144
            {
                return Err(
                    "operation_spec requires bounded explicit input and output JSON schemas".into(),
                );
            }
            eal_collector_contracts::capability::canonical_identity(schema)?;
            jsonschema::validator_for(schema)
                .map_err(|_| "operation_spec contains an invalid JSON schema".to_owned())?;
        }
        Ok(())
    }
}
