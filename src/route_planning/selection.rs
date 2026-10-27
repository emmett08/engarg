//! Bounded joint estimates, charging equivalent symbolic request groups once.
use super::*;
use crate::{Error, support_requirements::SupportElaboration};
use eal_collector_contracts::capability::*;
use std::collections::BTreeMap;
mod beam;
use beam::{OptionCost, Search};

pub(super) struct PreparedRoute {
    pub ranked: Vec<RankedCapability>,
    pub assessments: Vec<CandidateAssessment>,
    pub selected: Option<String>,
}
fn per_capability(dimension: CostDimension) -> bool {
    matches!(
        dimension,
        CostDimension::ConfigurationEffortSeconds | CostDimension::ImplementationEffortSeconds
    )
}

pub(super) fn prepare(
    elaboration: &SupportElaboration,
    catalogue: &CapabilityCatalogue,
    config: &PlanningConfig,
    discovery: &DiscoverySnapshot,
    allowed_capabilities: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> Result<(Vec<PreparedRoute>, PlanSelection), Error> {
    let capability_ids = catalogue
        .capabilities
        .iter()
        .map(|capability| capability.id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let capability_indices = capability_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut source_indices = BTreeMap::new();
    let mut rows = Vec::new();
    let mut search = Search::new(
        config
            .ranking
            .dimensions
            .iter()
            .copied()
            .map(per_capability)
            .collect(),
    );
    for blueprint in &elaboration.acquisitions {
        let array_shape = source_item_array_shape(elaboration, catalogue, blueprint);
        let allowed = |id: &str| {
            allowed_capabilities
                .get(&blueprint.id)
                .is_none_or(|allowed| allowed.contains(id))
                && !array_shape.is_some_and(|expected_array| {
                    let capability = catalogue
                        .capabilities
                        .iter()
                        .find(|capability| capability.id == id)
                        .expect("catalogue capability");
                    let binding = config
                        .bindings
                        .iter()
                        .find(|binding| binding.capability_id == id);
                    let logical = &blueprint
                        .for_each
                        .as_ref()
                        .expect("known fan-out shape")
                        .argument_pointer;
                    let physical = binding
                        .and_then(|binding| {
                            binding
                                .input_bindings
                                .iter()
                                .find(|(_, source)| *source == logical)
                                .map(|(physical, _)| physical)
                        })
                        .unwrap_or(logical);
                    crate::support_graph::schema_at(&capability.input_schema, physical)
                        .and_then(array_schema)
                        .is_some_and(|actual_array| actual_array != expected_array)
                })
        };
        let ranked = rank_adequate(
            &blueprint.contract,
            &catalogue.capabilities,
            &config.ranking,
        )
        .map_err(Error::InvalidInput)?
        .into_iter()
        .filter(|rank| allowed(&rank.capability_id))
        .collect::<Vec<_>>();
        let mut assessments = Vec::new();
        let mut physical = BTreeMap::new();
        for capability in catalogue
            .capabilities
            .iter()
            .filter(|capability| allowed(&capability.id))
        {
            let compatibility = compatible(&blueprint.contract, capability);
            let binding = config
                .bindings
                .iter()
                .find(|binding| binding.capability_id == capability.id);
            let mut gaps = Vec::new();
            if compatibility.adequate {
                let (arguments, missing) = arguments::materialise(blueprint, capability, binding)?;
                gaps = missing;
                let dependencies = arguments::physical_dependencies(blueprint, binding);
                // Unresolved references remain symbolic. Different logical
                // references may converge to one actual request at execution.
                let key = canonical_identity(&(
                    &capability.id,
                    arguments,
                    &blueprint.contract.scope.selectors,
                    &blueprint.contract.time.historical_interval,
                    dependencies,
                ))
                .map_err(Error::InvalidInput)?;
                let next_index = source_indices.len();
                let index = *source_indices.entry(key).or_insert(next_index);
                physical.insert(capability.id.clone(), index);
                if !discovery.admitted_capabilities.contains(&capability.id) {
                    gaps.push("Live source admission or its connection/credential references remain unresolved".into());
                }
            }
            let executable = compatibility.adequate && gaps.is_empty();
            assessments.push(CandidateAssessment {
                capability_id: capability.id.clone(),
                compatibility,
                configuration_gaps: gaps,
                executable,
            });
        }
        let require_ready = config.prefer_configured && assessments.iter().any(|c| c.executable);
        let options = ranked
            .iter()
            .filter(|rank| {
                !require_ready
                    || assessments
                        .iter()
                        .any(|c| c.capability_id == rank.capability_id && c.executable)
            })
            .map(|rank| OptionCost {
                capability: capability_indices[rank.capability_id.as_str()],
                source: physical[&rank.capability_id],
                maximum_calls: blueprint
                    .for_each
                    .as_ref()
                    .map_or(1, |fanout| fanout.maximum_items),
                exact_source_count: blueprint.bindings.is_empty() && blueprint.for_each.is_none(),
                estimates: rank
                    .estimates
                    .iter()
                    .zip(&config.ranking.dimensions)
                    .map(|(estimate, dimension)| {
                        // A per-invocation descriptor cannot predict how many IDs a
                        // dependency returns or how early a witness will be found.
                        if blueprint.for_each.is_some() && !per_capability(*dimension) {
                            None
                        } else {
                            estimate.value
                        }
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        search.advance(&options);
        rows.push(PreparedRoute {
            ranked,
            assessments,
            selected: None,
        });
    }
    let best = search.finish();
    for (row, selected) in rows.iter_mut().zip(best.choices) {
        row.selected = selected.map(|index| capability_ids[index].clone());
    }
    Ok((
        rows,
        PlanSelection {
            method: "bounded_joint_cost_with_symbolic_request_sharing/2".into(),
            graphs: None,
            search_complete: best.complete,
            candidate_states: best.considered,
            estimated_distinct_acquisitions: best.exact_source_count,
            estimated_distinct_acquisitions_upper_bound: Some(best.maximum_calls),
            dynamic_acquisition_groups: best.dynamic_sources,
            total_costs: best.costs,
        },
    ))
}
#[cfg(test)]
mod tests;

fn array_schema(schema: &serde_json::Value) -> Option<bool> {
    schema
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(|kind| kind == "array")
}

/// Reject only a proved array/scalar mismatch. Unknown or mixed source shapes
/// retain the authored candidates, including legitimate fan-out over arrays.
fn source_item_array_shape(
    elaboration: &SupportElaboration,
    catalogue: &CapabilityCatalogue,
    blueprint: &crate::support_requirements::AcquisitionBlueprint,
) -> Option<bool> {
    let fanout = blueprint.for_each.as_ref()?;
    let upstream = elaboration
        .acquisitions
        .iter()
        .find(|source| source.id == fanout.output.step_id)?;
    if upstream.for_each.is_some() {
        return None;
    }
    let mut shape = None;
    for capability in catalogue
        .capabilities
        .iter()
        .filter(|capability| compatible(&upstream.contract, capability).adequate)
    {
        let quantity = capability
            .semantics
            .as_ref()?
            .quantities
            .iter()
            .find(|quantity| {
                format!("/{}", quantity.name.replace('~', "~0").replace('/', "~1"))
                    == fanout.output.pointer
            })?;
        let schema =
            crate::support_graph::schema_at(&capability.output_schema, &quantity.output_pointer)?;
        if schema.get("type").and_then(serde_json::Value::as_str) != Some("array") {
            return None;
        }
        let candidate = array_schema(schema.get("items")?)?;
        if shape.is_some_and(|shape| shape != candidate) {
            return None;
        }
        shape = Some(candidate);
    }
    shape
}
