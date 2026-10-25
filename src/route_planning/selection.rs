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
        let ranked = rank_adequate(
            &blueprint.contract,
            &catalogue.capabilities,
            &config.ranking,
        )
        .map_err(Error::InvalidInput)?;
        let mut assessments = Vec::new();
        let mut physical = BTreeMap::new();
        for capability in &catalogue.capabilities {
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
