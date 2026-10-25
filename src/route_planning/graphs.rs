//! Compare a bounded family of qualified per-item and batch topologies.
use super::*;
use crate::support_graph::{BatchRouteProof, effective_blueprint};
use eal_collector_contracts::capability::canonical_identity;
use std::collections::BTreeMap;

const MAX_GRAPHS: usize = 32;
const MAX_CORRESPONDENCE_CHECKS: usize = 256;

fn graph_id(batches: &BTreeMap<String, BatchRouteProof>) -> Result<String, Error> {
    if batches.is_empty() {
        return Ok("authored".into());
    }
    canonical_identity(batches)
        .map(|digest| format!("batch_{}", &digest[..24]))
        .map_err(Error::InvalidInput)
}

fn transformations(batches: &BTreeMap<String, BatchRouteProof>) -> Vec<String> {
    batches
        .iter()
        .map(|(source, proof)| format!("{source}:{}", proof.correspondence.id))
        .collect()
}

pub fn compile(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    catalogue: &CapabilityCatalogue,
    config: &PlanningConfig,
    discovery: &DiscoverySnapshot,
) -> Result<RoutePlan, Error> {
    // Check the baseline before deriving alternatives so malformed authored
    // requirements cannot become acceptable by selecting a different topology.
    let baseline = compiler::compile_fixed(
        case,
        elaboration,
        catalogue,
        config,
        discovery,
        &BTreeMap::new(),
    )?;
    if config.batch_correspondences.is_empty() {
        return Ok(baseline);
    }
    let mut complete = true;
    let mut checks = 0;
    let mut rejected = Vec::new();
    let mut variants = vec![BTreeMap::new()];
    let mut blueprints = elaboration
        .acquisitions
        .iter()
        .filter(|blueprint| blueprint.for_each.is_some())
        .collect::<Vec<_>>();
    blueprints.sort_by(|left, right| left.id.cmp(&right.id));
    let mut correspondences = config.batch_correspondences.iter().collect::<Vec<_>>();
    correspondences.sort_by(|left, right| left.id.cmp(&right.id));
    'sources: for blueprint in blueprints {
        let mut applicable = Vec::new();
        for correspondence in &correspondences {
            if checks == MAX_CORRESPONDENCE_CHECKS {
                complete = false;
                break 'sources;
            }
            checks += 1;
            let result: Result<BatchRouteProof, Error> = (|| {
                let item = catalogue
                    .capabilities
                    .iter()
                    .find(|capability| capability.id == correspondence.item_capability_id)
                    .ok_or_else(|| {
                        Error::InvalidInput("qualified item capability is absent".into())
                    })?;
                let batch = catalogue
                    .capabilities
                    .iter()
                    .find(|capability| capability.id == correspondence.batch_capability_id)
                    .ok_or_else(|| {
                        Error::InvalidInput("qualified batch capability is absent".into())
                    })?;
                let proof = BatchRouteProof {
                    correspondence: (*correspondence).clone(),
                    item_capability: item.clone(),
                };
                effective_blueprint(blueprint, batch, Some(&proof))?;
                Ok(proof)
            })();
            match result {
                Ok(proof) => applicable.push(proof),
                Err(error) => rejected.push(AcquisitionGraphAlternative {
                    id: format!("{}:{}", blueprint.id, correspondence.id),
                    transformations: vec![format!("{}:{}", blueprint.id, correspondence.id)],
                    rejection: Some(error.to_string()),
                    total_costs: vec![],
                    unresolved_acquisitions: 0,
                    configuration_gaps: 0,
                    route_search_complete: true,
                }),
            }
        }
        let previous = variants.clone();
        for variant in previous {
            for proof in &applicable {
                if variants.len() == MAX_GRAPHS {
                    complete = false;
                    break;
                }
                let mut transformed = variant.clone();
                transformed.insert(blueprint.id.clone(), proof.clone());
                variants.push(transformed);
            }
        }
    }
    let mut plans = vec![("authored".to_owned(), baseline)];
    let mut alternatives = Vec::new();
    for variant in &variants {
        let id = graph_id(variant)?;
        if !variant.is_empty() {
            match compiler::compile_fixed(case, elaboration, catalogue, config, discovery, variant)
            {
                Ok(plan) => plans.push((id.clone(), plan)),
                Err(error) => {
                    rejected.push(AcquisitionGraphAlternative {
                        id,
                        transformations: transformations(variant),
                        rejection: Some(error.to_string()),
                        total_costs: vec![],
                        unresolved_acquisitions: 0,
                        configuration_gaps: 0,
                        route_search_complete: true,
                    });
                    continue;
                }
            }
        }
        let plan = &plans.last().expect("compiled graph").1;
        let selection = plan.selection.as_ref().expect("route selection");
        alternatives.push(AcquisitionGraphAlternative {
            id,
            transformations: transformations(variant),
            rejection: None,
            total_costs: selection.total_costs.clone(),
            unresolved_acquisitions: plan
                .routes
                .iter()
                .filter(|route| route.selected.is_none())
                .count(),
            configuration_gaps: plan
                .routes
                .iter()
                .map(|route| route.configuration_gaps.len())
                .sum(),
            route_search_complete: selection.search_complete,
        });
    }
    // Feasibility precedes cost: an unimplemented source with no cost estimate
    // must never win simply because the unavailable route contributes zero.
    let minimum_unresolved = alternatives
        .iter()
        .map(|entry| entry.unresolved_acquisitions)
        .min()
        .unwrap_or(0);
    let ready_exists = config.prefer_configured
        && alternatives.iter().any(|entry| {
            entry.unresolved_acquisitions == minimum_unresolved && entry.configuration_gaps == 0
        });
    let chosen = alternatives
        .iter()
        .filter(|entry| {
            entry.unresolved_acquisitions == minimum_unresolved
                && (!ready_exists || entry.configuration_gaps == 0)
        })
        .min_by(|left, right| {
            left.total_costs
                .cmp(&right.total_costs)
                .then_with(|| left.id.cmp(&right.id))
        })
        .expect("authored graph remains available")
        .id
        .clone();
    let states = plans
        .iter()
        .map(|(_, plan)| plan.selection.as_ref().expect("selection").candidate_states)
        .sum();
    complete &= alternatives.iter().all(|entry| entry.route_search_complete);
    alternatives.extend(rejected);
    let mut selected = plans
        .into_iter()
        .find(|(id, _)| *id == chosen)
        .expect("chosen graph")
        .1;
    let selection = selected.selection.as_mut().expect("selection");
    selection.candidate_states = states;
    selection.search_complete = complete;
    selection.method = "bounded_qualified_graphs_with_joint_route_selection/1".into();
    selection.graphs = Some(AcquisitionGraphComparison {
        selected: chosen,
        limit: MAX_GRAPHS,
        correspondence_check_limit: MAX_CORRESPONDENCE_CHECKS,
        correspondence_checks: checks,
        search_complete: complete,
        alternatives,
    });
    Ok(selected)
}

#[cfg(test)]
mod tests;
