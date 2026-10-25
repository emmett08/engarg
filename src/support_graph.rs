//! Format-independent compilation of qualified support acquisition graphs.
use crate::{Error, support_requirements::SupportElaboration};
use eal_collector_contracts::{
    capability::{CapabilityDescriptor, compatible},
    execution::{
        DerivationOperation, DerivationStep, EvidenceExecutionPlan, OutputReference,
        SourceAvailability, canonical_digest, valid_pointer,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A qualified source route and its mechanically derived quantity projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRouteProof {
    pub blueprint_id: String,
    pub capability: CapabilityDescriptor,
    pub projection_id: Option<String>,
    /// Physical input pointer -> original logical argument pointer.
    #[serde(default)]
    pub input_bindings: BTreeMap<String, String>,
}

/// Compile the logical graph using only qualified output-pointer mappings.
/// Source authority and argument binding remain the route compiler's responsibility.
/// Calling this again produces the same graph, rather than stacking projections.
pub fn project_execution_graph(
    elaboration: &SupportElaboration,
    routes: &[AcquisitionRouteProof],
    execution: &mut EvidenceExecutionPlan,
) -> Result<(), Error> {
    let mut expected = expected_graph(elaboration, routes, execution)?;
    expected.validate().map_err(Error::InvalidInput)?;
    std::mem::swap(execution, &mut expected);
    Ok(())
}

pub fn validate_execution_graph(
    elaboration: &SupportElaboration,
    routes: &[AcquisitionRouteProof],
    execution: &EvidenceExecutionPlan,
) -> Result<(), Error> {
    let expected = expected_graph(elaboration, routes, execution)?;
    if canonical_digest(&expected).map_err(Error::InvalidInput)?
        != canonical_digest(execution).map_err(Error::InvalidInput)?
    {
        return Err(invalid(
            "routing changed source freshness, logical dependencies, derivations or requirement evaluations",
        ));
    }
    Ok(())
}

fn expected_graph(
    elaboration: &SupportElaboration,
    routes: &[AcquisitionRouteProof],
    execution: &EvidenceExecutionPlan,
) -> Result<EvidenceExecutionPlan, Error> {
    let logical: BTreeMap<_, _> = elaboration
        .acquisitions
        .iter()
        .map(|step| (step.id.as_str(), step))
        .collect();
    let actual: BTreeMap<_, _> = execution
        .acquisitions
        .iter()
        .map(|step| (step.id.as_str(), step))
        .collect();
    if actual.len() != execution.acquisitions.len()
        || logical.keys().collect::<BTreeSet<_>>() != actual.keys().collect::<BTreeSet<_>>()
    {
        return Err(invalid(
            "execution must retain every logical source operation identity",
        ));
    }
    let mut admitted_routes = BTreeMap::new();
    let mut projections = BTreeMap::new();
    let mut occupied: BTreeSet<_> = logical
        .keys()
        .copied()
        .chain(elaboration.derivations.iter().map(|step| step.id.as_str()))
        .collect();
    for route in routes {
        let Some(blueprint) = logical.get(route.blueprint_id.as_str()) else {
            return Err(invalid("route proof names an unknown logical source"));
        };
        if admitted_routes
            .insert(route.blueprint_id.as_str(), route)
            .is_some()
        {
            return Err(invalid("route proofs must be unique per logical source"));
        }
        route.capability.validate().map_err(Error::InvalidInput)?;
        if !compatible(&blueprint.contract, &route.capability).adequate {
            return Err(invalid(
                "selected source does not satisfy its tool-independent acquisition contract",
            ));
        }
        let step = actual[route.blueprint_id.as_str()];
        if route.input_bindings.values().collect::<BTreeSet<_>>().len()
            != route.input_bindings.len()
            || route.input_bindings.iter().any(|(physical, logical)| {
                !valid_pointer(physical)
                    || !valid_pointer(logical)
                    || blueprint.arguments.pointer(logical).is_none()
                    || step.arguments.pointer(physical).is_none()
                    || blueprint.arguments.pointer(logical) != step.arguments.pointer(physical)
            })
        {
            return Err(invalid(
                "input projection mappings require distinct existing logical and physical argument pointers",
            ));
        }
        let descriptor = &route.capability;
        let source = &step.source;
        if source.provider_id != descriptor.provider.id
            || source.capability_id != descriptor.id
            || source.capability_version != descriptor.version
            || source.input_schema_sha256 != descriptor.input_schema_sha256
            || source.output_schema_sha256 != descriptor.output_schema_sha256
            || source.qualified_contract_sha256
                != descriptor
                    .measurement_identity()
                    .map_err(Error::InvalidInput)?
            || source.configuration_sha256 != descriptor.provider.configuration_sha256
            || blueprint
                .contract
                .scope
                .selectors
                .iter()
                .any(|(key, value)| source.scope.get(key) != Some(value))
        {
            return Err(invalid(
                "execution source identity differs from its qualified route proof or required scope",
            ));
        }
        let semantics = descriptor
            .semantics
            .as_ref()
            .expect("compatible route is qualified");
        if semantics.quantities.is_empty() {
            if route.projection_id.is_some() {
                return Err(invalid(
                    "a source without qualified quantities cannot invent a quantity projection",
                ));
            }
        } else {
            let Some(id) = route.projection_id.as_deref() else {
                return Err(invalid(
                    "qualified quantities require an explicit projection node identity",
                ));
            };
            if id.trim().is_empty()
                || id.len() > 256
                || id.chars().any(char::is_control)
                || !occupied.insert(id)
            {
                return Err(invalid(
                    "quantity projection identities must be unique and bounded",
                ));
            }
            let fields = semantics
                .quantities
                .iter()
                .map(|quantity| (quantity.name.clone(), quantity.output_pointer.clone()))
                .collect();
            projections.insert(
                route.blueprint_id.as_str(),
                DerivationStep {
                    id: id.into(),
                    inputs: vec![OutputReference {
                        step_id: route.blueprint_id.clone(),
                        pointer: String::new(),
                    }],
                    operation: DerivationOperation::Project { fields },
                },
            );
        }
    }
    for step in &execution.acquisitions {
        if matches!(step.availability, SourceAvailability::Configured)
            && !admitted_routes.contains_key(step.id.as_str())
        {
            return Err(invalid(
                "a configured source requires a qualified route proof; missing routes must remain explicitly unavailable",
            ));
        }
    }
    let rewrite = |reference: &mut OutputReference| {
        if let Some(projection) = projections.get(reference.step_id.as_str()) {
            reference.step_id = projection.id.clone();
        }
    };
    let mut result = execution.clone();
    for step in &mut result.acquisitions {
        let blueprint = logical[step.id.as_str()];
        // This source limit is independent of downstream evaluation ages and
        // remains binding even for unavailable operations without route proofs.
        step.max_age_seconds = blueprint.contract.time.max_age_seconds;
        step.bindings = blueprint.bindings.clone();
        step.for_each = blueprint.for_each.clone();
        for binding in &mut step.bindings {
            rewrite(&mut binding.output);
            if let Some(route) = admitted_routes.get(step.id.as_str())
                && let Some((physical, _)) = route
                    .input_bindings
                    .iter()
                    .find(|(_, logical)| *logical == &binding.argument_pointer)
            {
                binding.argument_pointer = physical.clone();
            }
        }
        if let Some(binding) = &mut step.for_each {
            rewrite(&mut binding.output);
            if let Some(route) = admitted_routes.get(step.id.as_str())
                && let Some((physical, _)) = route
                    .input_bindings
                    .iter()
                    .find(|(_, logical)| *logical == &binding.argument_pointer)
            {
                binding.argument_pointer = physical.clone();
            }
            if let Some(predicate) = &mut binding.stop_on_witness {
                let quantity=blueprint.contract.quantities.iter().find(|quantity|logical_quantity_pointer(&quantity.name)==predicate.pointer).ok_or_else(||invalid("a witness predicate must select one complete declared logical quantity"))?;
                if let Some(route) = admitted_routes.get(step.id.as_str()) {
                    let qualified = route
                        .capability
                        .semantics
                        .as_ref()
                        .expect("admitted route is qualified")
                        .quantities
                        .iter()
                        .find(|offered| offered.name == quantity.name)
                        .ok_or_else(|| {
                            invalid("a witness predicate has no qualified source quantity mapping")
                        })?;
                    // The runtime checks the raw per-item response before the
                    // generated Project node normalises logical quantities.
                    predicate.pointer = qualified.output_pointer.clone();
                }
            }
        }
    }
    let mut authored_derivations = elaboration.derivations.clone();
    for step in &mut authored_derivations {
        for reference in &mut step.inputs {
            rewrite(reference);
        }
    }
    result.derivations = projections.into_values().collect();
    result.derivations.extend(authored_derivations);
    result.evaluations = elaboration.evaluations.clone();
    // Do not rewrite references to authored derivations: their meaning and output
    // pointers stay exactly as authored, including acceptance and gating clauses.
    let projection_ids: BTreeMap<_, _> = result
        .derivations
        .iter()
        .filter_map(|step| match &step.operation {
            DerivationOperation::Project { .. }
                if step.inputs.len() == 1
                    && admitted_routes
                        .get(step.inputs[0].step_id.as_str())
                        .is_some_and(|route| {
                            route.projection_id.as_deref() == Some(step.id.as_str())
                        }) =>
            {
                Some((step.inputs[0].step_id.as_str(), step.id.as_str()))
            }
            _ => None,
        })
        .collect();
    for evaluation in &mut result.evaluations {
        for reference in evaluation
            .inputs
            .iter_mut()
            .chain(&mut evaluation.applicability)
            .chain(&mut evaluation.challenges)
        {
            if let Some(id) = projection_ids.get(reference.step_id.as_str()) {
                reference.step_id = (*id).into();
            }
        }
    }
    Ok(result)
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("SUPPORT_EXECUTION_PROJECTION: {message}"))
}

fn logical_quantity_pointer(name: &str) -> String {
    format!("/{}", name.replace('~', "~0").replace('/', "~1"))
}
