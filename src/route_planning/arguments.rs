use super::CapabilityInputBinding;
use crate::{Error, support_requirements::AcquisitionBlueprint};
use eal_collector_contracts::{capability::*, execution};
use serde_json::Value;

/// Writes object fields only; operator-owned array structure cannot be resized by a model.
pub fn set_pointer(value: &mut Value, pointer: &str, replacement: Value) -> Result<(), Error> {
    if pointer.is_empty() {
        *value = replacement;
        return Ok(());
    }
    if !execution::valid_pointer(pointer) {
        return Err(Error::InvalidInput("Invalid input pointer".into()));
    }
    let mut components = pointer
        .split('/')
        .skip(1)
        .map(|key| key.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let key = components
        .pop()
        .ok_or_else(|| Error::InvalidInput("Invalid input pointer".into()))?;
    let mut parent = value;
    for component in components {
        if parent.is_null() {
            *parent = Value::Object(Default::default());
        }
        parent = match parent {
            Value::Object(object) => object.entry(component).or_insert(Value::Null),
            Value::Array(array) => {
                let index = component
                    .parse::<usize>()
                    .ok()
                    .filter(|index| *index < array.len())
                    .ok_or_else(|| {
                        Error::InvalidInput("Binding array index is unresolved".into())
                    })?;
                &mut array[index]
            }
            _ => {
                return Err(Error::InvalidInput(
                    "Binding parent conflicts with a configured scalar".into(),
                ));
            }
        };
    }
    if parent.is_null() {
        *parent = Value::Object(Default::default());
    }
    match parent {
        Value::Object(object) => {
            object.insert(key, replacement);
        }
        Value::Array(array) => {
            let i = key
                .parse::<usize>()
                .ok()
                .filter(|i| *i < array.len())
                .ok_or_else(|| Error::InvalidInput("Binding array index is unresolved".into()))?;
            array[i] = replacement;
        }
        _ => {
            return Err(Error::InvalidInput(
                "Binding parent is not an object/array".into(),
            ));
        }
    }
    Ok(())
}

pub fn materialise(
    blueprint: &AcquisitionBlueprint,
    capability: &CapabilityDescriptor,
    binding: Option<&CapabilityInputBinding>,
) -> Result<(Value, Vec<String>), Error> {
    let mut arguments = binding
        .map(|b| b.arguments.clone())
        .unwrap_or_else(|| blueprint.arguments.clone());
    let mut gaps = Vec::new();
    if let Some(binding) = binding {
        for (to, from) in &binding.input_bindings {
            if let Some(value) = blueprint.arguments.pointer(from) {
                if let Err(error) = set_pointer(&mut arguments, to, value.clone()) {
                    gaps.push(error.to_string());
                }
            } else {
                gaps.push(format!(
                    "Logical input {from} is unresolved for {}",
                    capability.id
                ));
            }
        }
    }
    if let Some(semantics) = &capability.semantics {
        for (dimension, pointer) in &semantics.scope_bindings {
            if let Some(value) = blueprint.contract.scope.selectors.get(dimension) {
                // Explicit conflicting operator values require correction, never replacement.
                if arguments
                    .pointer(pointer)
                    .is_some_and(|old| !old.is_null() && old != value)
                {
                    gaps.push(format!(
                        "Configured selector {dimension} conflicts with the required scope"
                    ));
                } else if let Err(error) = set_pointer(&mut arguments, pointer, value.clone()) {
                    gaps.push(error.to_string());
                }
            }
        }
    }
    let scope = capability
        .semantics
        .as_ref()
        .map(|semantics| semantics.scope_constraints.clone())
        .unwrap_or_default();
    let mut scope = scope;
    scope.extend(blueprint.contract.scope.selectors.clone());
    let (dependencies, fanout) = physical_dependencies(blueprint, binding);
    let dynamic: Vec<_> = dependencies
        .iter()
        .map(|item| item.argument_pointer.as_str())
        .chain(fanout.iter().map(|item| item.argument_pointer.as_str()))
        .collect();
    // Only graph-declared slots are deferred. Retain a placeholder when a declared
    // slot has no literal value yet, so unrelated required properties still fail.
    for pointer in &dynamic {
        if arguments.pointer(pointer).is_none()
            && let Err(error) = set_pointer(&mut arguments, pointer, Value::Null)
        {
            gaps.push(error.to_string());
        }
    }
    let validator = jsonschema::validator_for(&capability.input_schema)
        .map_err(|_| Error::InvalidInput("invalid qualified capability input schema".into()))?;
    for error in validator.iter_errors(&arguments) {
        let location = error.instance_path().to_string();
        if !dynamic.iter().any(|pointer| below(&location, pointer)) {
            gaps.push(format!(
                "Static capability input differs from its schema at {location}"
            ));
        }
    }
    if let Some(semantics) = &capability.semantics {
        for (dimension, pointer) in &semantics.scope_bindings {
            let overwritten = dynamic.iter().any(|dynamic| overlaps(pointer, dynamic));
            if overwritten {
                if scope.contains_key(dimension) {
                    gaps.push(format!(
                        "Dynamic input may replace fixed scope selector {dimension}"
                    ));
                }
                continue;
            }
            let value = arguments.pointer(pointer).filter(|value| {
                !value.is_null() && !value.as_str().is_some_and(|value| value.trim().is_empty())
            });
            match value {
                None => gaps.push(format!("Source scope selector {dimension} is unresolved")),
                Some(value)
                    if scope
                        .get(dimension)
                        .is_some_and(|expected| expected != value) =>
                {
                    gaps.push(format!(
                        "Source scope selector {dimension} differs from the required scope"
                    ));
                }
                _ => {}
            }
        }
    }
    if let Some(interval) = historical_interval(&blueprint.contract)?
        && let Err(error) =
            validate_historical_input(capability, &arguments, &interval.start, &interval.end)
    {
        gaps.push(error);
    }
    if blueprint.contract.time.historical_interval.is_some()
        && let Some(history) = capability
            .semantics
            .as_ref()
            .and_then(|semantics| semantics.time.historical_input.as_ref())
        && dynamic.iter().any(|pointer| {
            overlaps(pointer, &history.start_pointer) || overlaps(pointer, &history.end_pointer)
        })
    {
        gaps.push("Dynamic input may replace the fixed historical query interval".into());
    }
    for prerequisite in &capability.configuration_requirements {
        if prerequisite.required
            && !binding.is_some_and(|b| b.configured_prerequisites.contains(&prerequisite.id))
        {
            gaps.push(format!("{}: {}", prerequisite.id, prerequisite.description));
        }
    }
    Ok((arguments, gaps))
}

fn below(pointer: &str, ancestor: &str) -> bool {
    ancestor.is_empty()
        || pointer == ancestor
        || pointer
            .strip_prefix(ancestor)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn overlaps(left: &str, right: &str) -> bool {
    below(left, right) || below(right, left)
}

/// Use the same exact logical-to-physical mapping as support graph projection.
/// This is only input admission; project_execution_graph retains and rewrites the
/// canonical dependency declarations before execution.
pub fn physical_dependencies(
    blueprint: &AcquisitionBlueprint,
    binding: Option<&CapabilityInputBinding>,
) -> (
    Vec<execution::DependencyBinding>,
    Option<execution::ForEachBinding>,
) {
    let physical = |logical: &str| {
        binding
            .and_then(|binding| {
                binding
                    .input_bindings
                    .iter()
                    .find(|(_, source)| source.as_str() == logical)
                    .map(|(target, _)| target.clone())
            })
            .unwrap_or_else(|| logical.to_owned())
    };
    let mut dependencies = blueprint.bindings.clone();
    for dependency in &mut dependencies {
        dependency.argument_pointer = physical(&dependency.argument_pointer);
    }
    let mut fanout = blueprint.for_each.clone();
    if let Some(fanout) = &mut fanout {
        fanout.argument_pointer = physical(&fanout.argument_pointer);
    }
    (dependencies, fanout)
}

#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;
pub fn historical_interval(
    contract: &SupportContract,
) -> Result<Option<execution::HistoricalInterval>, Error> {
    contract
        .time
        .historical_interval
        .as_ref()
        .map(|interval| {
            let encode = |seconds| {
                chrono::DateTime::from_timestamp(seconds, 0)
                    .map(|time| time.to_rfc3339())
                    .ok_or_else(|| {
                        Error::InvalidInput(
                            "Historical interval is outside supported timestamp range".into(),
                        )
                    })
            };
            Ok(execution::HistoricalInterval {
                start: encode(interval.start_unix)?,
                end: encode(interval.end_unix)?,
            })
        })
        .transpose()
}
