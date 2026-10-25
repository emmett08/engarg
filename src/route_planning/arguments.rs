use super::CapabilityInputBinding;
pub use crate::support_graph::historical_interval;
#[cfg(test)]
use crate::support_graph::set_pointer;
use crate::{Error, support_requirements::AcquisitionBlueprint};
use eal_collector_contracts::{capability::*, execution};
use serde_json::Value;

pub fn materialise(
    blueprint: &AcquisitionBlueprint,
    capability: &CapabilityDescriptor,
    binding: Option<&CapabilityInputBinding>,
) -> Result<(Value, Vec<String>), Error> {
    let empty = std::collections::BTreeMap::new();
    let (arguments, mut gaps) = crate::support_graph::materialise_source_arguments(
        blueprint,
        capability,
        binding.map(|binding| &binding.arguments),
        binding
            .map(|binding| &binding.input_bindings)
            .unwrap_or(&empty),
    )?;
    for prerequisite in &capability.configuration_requirements {
        if prerequisite.required
            && !binding
                .is_some_and(|binding| binding.configured_prerequisites.contains(&prerequisite.id))
        {
            gaps.push(format!("{}: {}", prerequisite.id, prerequisite.description));
        }
    }
    Ok((arguments, gaps))
}
pub fn physical_dependencies(
    blueprint: &AcquisitionBlueprint,
    binding: Option<&CapabilityInputBinding>,
) -> (
    Vec<execution::DependencyBinding>,
    Option<execution::ForEachBinding>,
) {
    let empty = std::collections::BTreeMap::new();
    crate::support_graph::physical_dependencies(
        blueprint,
        binding
            .map(|binding| &binding.input_bindings)
            .unwrap_or(&empty),
    )
}
#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;
