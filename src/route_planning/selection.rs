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
    pub composition: Option<crate::support_graph::composition::IdentifierAuditProof>,
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
    let mut compositions = Vec::new();
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
        let mut ranked = rank_adequate(
            &blueprint.contract,
            &catalogue.capabilities,
            &config.ranking,
        )
        .map_err(Error::InvalidInput)?
        .into_iter()
        .filter(|rank| allowed(&rank.capability_id))
        .collect::<Vec<_>>();
        // Composite qualification is admitted only when exactly one producer
        // capability could be selected. The final selected pair is rechecked.
        let source = blueprint.bindings.first().and_then(|first| {
            elaboration
                .acquisitions
                .iter()
                .find(|source| source.id == first.output.step_id)
        });
        let producers = source
            .map(|source| {
                catalogue
                    .capabilities
                    .iter()
                    .filter(|candidate| compatible(&source.contract, candidate).adequate)
                    .filter(|candidate| {
                        allowed_capabilities
                            .get(&source.id)
                            .is_none_or(|ids| ids.contains(&candidate.id))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut composed = BTreeMap::new();
        if let (Some(source), [producer]) = (source, producers.as_slice()) {
            for capability in catalogue
                .capabilities
                .iter()
                .filter(|capability| allowed(&capability.id))
            {
                if compatible(&blueprint.contract, capability).adequate {
                    continue;
                }
                let binding = config
                    .bindings
                    .iter()
                    .find(|binding| binding.capability_id == capability.id);
                let mappings = binding
                    .map(|binding| &binding.input_bindings)
                    .cloned()
                    .unwrap_or_default();
                if let Ok((proof, local)) =
                    crate::support_graph::composition::identifier_audit_proof(
                        elaboration,
                        blueprint,
                        source,
                        capability,
                        producer,
                        &mappings,
                    )
                {
                    let candidate =
                        rank_adequate(&local, std::slice::from_ref(capability), &config.ranking)
                            .map_err(Error::InvalidInput)?;
                    if !candidate.is_empty() {
                        ranked.extend(candidate);
                        composed.insert(capability.id.clone(), (proof, local));
                    }
                }
            }
        }
        if !composed.is_empty() {
            // Merge the independently ranked composite candidates using the
            // same order as the pinned capability ranker. Legacy routes retain
            // their original ranking without any additional sorting.
            ranked.sort_by(|left, right| {
                left.estimates
                    .iter()
                    .map(|estimate| (estimate.value.is_none(), estimate.value.unwrap_or(0)))
                    .cmp(
                        right.estimates.iter().map(|estimate| {
                            (estimate.value.is_none(), estimate.value.unwrap_or(0))
                        }),
                    )
                    .then_with(|| left.capability_id.cmp(&right.capability_id))
                    .then_with(|| left.capability_version.cmp(&right.capability_version))
            });
        }
        let mut assessments = Vec::new();
        let mut physical = BTreeMap::new();
        for capability in catalogue
            .capabilities
            .iter()
            .filter(|capability| allowed(&capability.id))
        {
            let mut compatibility = composed.get(&capability.id).map_or_else(
                || compatible(&blueprint.contract, capability),
                |(_, local)| compatible(local, capability),
            );
            if let Some((proof, _)) = composed.get(&capability.id) {
                compatibility.limitations.push(format!(
                    "Adequacy composes this calculator with selected qualified source {}; it does not assign that source's authority to the calculator.",
                    proof.source_step_id
                ));
            }
            let binding = config
                .bindings
                .iter()
                .find(|binding| binding.capability_id == capability.id);
            let mut gaps = Vec::new();
            if compatibility.adequate {
                let (arguments, missing) = arguments::materialise_with_composition(
                    blueprint,
                    capability,
                    binding,
                    composed.get(&capability.id).map(|(proof, _)| proof),
                )?;
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
            composition: None,
        });
        compositions.push(composed);
    }
    let best = search.finish();
    for (row, selected) in rows.iter_mut().zip(best.choices) {
        row.selected = selected.map(|index| capability_ids[index].clone());
    }
    for index in 0..rows.len() {
        let proof = rows[index]
            .selected
            .as_ref()
            .and_then(|id| compositions[index].get(id))
            .map(|(proof, _)| proof.clone());
        if let Some(proof) = proof {
            let source = elaboration
                .acquisitions
                .iter()
                .position(|source| source.id == proof.source_step_id);
            if source.is_some_and(|source| {
                rows[source].selected.as_deref() == Some(proof.source_capability_id.as_str())
            }) {
                rows[index].composition = Some(proof);
            } else {
                // Do not publish the chosen path's costs for a different route.
                return Err(Error::InvalidInput("composed audit requires the exact selected qualified source; route selection cannot use its costs for a different producer".into()));
            }
        }
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
