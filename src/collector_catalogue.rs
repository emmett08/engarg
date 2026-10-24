//! EngArg evidence-need mapping over the authoritative EAL collector contract.
use crate::{Error, acquisition::AcquisitionPlan};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

pub use eal_collector_contracts::{
    CommandBinding as RegistryCommand, CommandTemplate as CommandBinding,
    InputConstraint as WindowConstraint, PreparedCommandRegistry as ToolsRegistry,
};
pub const SCHEMA: &str = eal_collector_contracts::CATALOGUE_SCHEMA;

/// Local authoring facade. Its serialised representation is the EAL wire model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollectorCatalogue(pub eal_collector_contracts::Catalogue);
impl Deref for CollectorCatalogue {
    type Target = eal_collector_contracts::Catalogue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for CollectorCatalogue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl CollectorCatalogue {
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 1_048_576 {
            return Err(Error::Budget("collector catalogue exceeds 1 MiB".into()));
        }
        let value = eal_collector_contracts::strict_json_exact(
            bytes,
            eal_collector_contracts::JsonLimits {
                max_bytes: 1_048_576,
                ..Default::default()
            },
        )
        .map_err(Error::InvalidInput)?;
        let catalogue = Self(serde_json::from_value(value)?);
        catalogue.validate()?;
        Ok(catalogue)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.collectors.len() > 128 {
            return Err(Error::Budget(
                "EngArg authoring catalogue exceeds 128 entries".into(),
            ));
        }
        self.0
            .validate_for(eal_collector_contracts::AuthoringProfile::RuntimeExportable)
            .map_err(Error::InvalidInput)
    }
    pub fn get(&self, id: &str) -> Option<CollectorDescriptor<'_>> {
        self.collectors
            .iter()
            .find(|descriptor| descriptor.id == id)
            .map(CollectorDescriptor)
    }
    /// Executable selections and credential names stay outside model prompts.
    pub fn planning_view(&self) -> Value {
        json!(self.collectors.iter().map(|collector| json!({"id":collector.id,"version":collector.version,
            "purpose":collector.purpose,"arguments":collector.parameters,"required_arguments":collector.required_parameters().expect("admitted descriptor"),
            "input_schema":collector.input_schema,"result_schema":collector.result_schema,"constraints":collector.constraints})).collect::<Vec<_>>())
    }
}

/// A borrowed descriptor with only EngArg's proposal-to-input mapping added.
#[derive(Clone, Copy)]
pub struct CollectorDescriptor<'a>(pub(crate) &'a eal_collector_contracts::CollectorDescriptor);
impl Deref for CollectorDescriptor<'_> {
    type Target = eal_collector_contracts::CollectorDescriptor;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl CollectorDescriptor<'_> {
    pub fn input(&self, plan: &AcquisitionPlan) -> Result<Value, Error> {
        let arguments: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.as_str(), arg.value.as_str()))
            .collect();
        if arguments.len() != plan.arguments.len() {
            return Err(Error::InvalidInput("collector repeats an argument".into()));
        }
        let mut input = serde_json::Map::new();
        let mut unresolved = Vec::new();
        fn string_property(schema: &Value, root: &Value, remaining: u8) -> bool {
            schema["type"] == "string"
                || (remaining > 0
                    && schema
                        .get("$ref")
                        .and_then(Value::as_str)
                        .and_then(|reference| reference.strip_prefix('#'))
                        .and_then(|pointer| root.pointer(pointer))
                        .is_some_and(|target| string_property(target, root, remaining - 1)))
        }
        for (field, schema) in self.input_schema["properties"]
            .as_object()
            .ok_or_else(|| Error::InvalidInput("missing input properties".into()))?
        {
            let value = match arguments.get(field.as_str()) {
                Some(value) if string_property(schema, &self.input_schema, 16) => json!(value),
                Some(value) => {
                    eal_collector_contracts::strict_json_exact(value.as_bytes(), Default::default())
                        .map_err(Error::InvalidInput)?
                }
                None => {
                    if plan.unresolved_arguments.contains(field) {
                        unresolved.push(field.clone());
                    } else {
                        continue;
                    }
                    Value::Null
                }
            };
            input.insert(field.clone(), value);
        }
        let input = Value::Object(input);
        self.0
            .validate_prepared_input(&input, &unresolved)
            .map_err(Error::InvalidInput)?;
        Ok(input)
    }
    pub fn binding(&self, plan: &AcquisitionPlan) -> Result<RegistryCommand, Error> {
        let arguments: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.clone(), arg.value.clone()))
            .collect();
        self.0
            .prepare_binding(&arguments, &plan.unresolved_arguments)
            .map_err(Error::InvalidInput)
    }
}
