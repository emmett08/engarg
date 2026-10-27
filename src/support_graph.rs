//! Format-independent compilation of qualified support acquisition graphs.
mod batch;
mod inputs;
use crate::{Error, support_requirements::SupportElaboration};
pub use batch::{BatchRouteProof, QualifiedBatchCorrespondence};
pub(crate) use batch::{effective_blueprint, schema_at, validate_batch_population};
use eal_collector_contracts::{
    capability::{
        CapabilityDescriptor, SupportContract, canonical_identity, compatible,
        validate_historical_input,
    },
    execution::{
        DerivationOperation, DerivationStep, EvidenceExecutionPlan, OutputReference,
        SourceAvailability, canonical_digest, valid_pointer,
    },
};
pub use inputs::{materialise_source_arguments, physical_dependencies, set_pointer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A qualified source route and its mechanically derived quantity projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRouteProof {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<BatchRouteProof>,
    pub blueprint_id: String,
    pub capability: CapabilityDescriptor,
    pub projection_id: Option<String>,
    /// Physical input pointer -> original logical argument pointer.
    #[serde(default)]
    pub input_bindings: BTreeMap<String, String>,
    /// Operator-owned physical constants, before logical bindings are applied.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "eal_collector_contracts::deserialize_optional_value_exact"
    )]
    pub operator_arguments: Option<serde_json::Value>,
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
    let mut effective = BTreeMap::new();
    let mut projections = BTreeMap::new();
    let mut comparison_controls = BTreeMap::new();
    let mut batch_checks = BTreeMap::new();
    let mut generated_ids = BTreeSet::new();
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
        let transformed = effective_blueprint(blueprint, &route.capability, route.batch.as_ref())?;
        effective.insert(route.blueprint_id.as_str(), transformed);
        let blueprint = &effective[route.blueprint_id.as_str()];
        if let Some(batch) = &route.batch {
            let qualification = &batch.correspondence;
            let physical = route
                .input_bindings
                .iter()
                .find(|(_, logical)| *logical == &qualification.logical_argument_pointer)
                .map_or(
                    qualification.logical_argument_pointer.as_str(),
                    |(physical, _)| physical.as_str(),
                );
            if physical != qualification.batch_argument_pointer {
                return Err(invalid(
                    "batch input mapping differs from its qualified array slot",
                ));
            }
        }
        let step = actual[route.blueprint_id.as_str()];
        let (expected_arguments, gaps) = materialise_source_arguments(
            blueprint,
            &route.capability,
            route.operator_arguments.as_ref(),
            &route.input_bindings,
        )?;
        if canonical_digest(&expected_arguments).map_err(Error::InvalidInput)?
            != canonical_digest(&step.arguments).map_err(Error::InvalidInput)?
        {
            return Err(invalid(
                "physical arguments differ from the original logical inputs and operator projection",
            ));
        }
        if matches!(step.availability, SourceAvailability::Configured) && !gaps.is_empty() {
            return Err(invalid(
                "a configured source has unresolved logical input projection requirements",
            ));
        }
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
        let mut expected_scope = descriptor
            .semantics
            .as_ref()
            .expect("compatible route is qualified")
            .scope_constraints
            .clone();
        expected_scope.extend(blueprint.contract.scope.selectors.clone());
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
            || source.authentication_scope_sha256
                != canonical_identity(&descriptor.provider.authentication_scope)
                    .map_err(Error::InvalidInput)?
            || canonical_digest(&source.scope).map_err(Error::InvalidInput)?
                != canonical_digest(&expected_scope).map_err(Error::InvalidInput)?
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
                || !generated_ids.insert(id.to_owned())
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
                    operation: if blueprint.for_each.is_some() {
                        DerivationOperation::Project { fields }
                    } else {
                        DerivationOperation::ProjectValue { fields }
                    },
                },
            );
            if route.batch.is_some() {
                let original = logical[route.blueprint_id.as_str()];
                let (cardinality, comparison) = batch::projection_checks(original, id);
                for step in cardinality.iter().chain(&comparison) {
                    if step.id.len() > 256
                        || occupied.contains(step.id.as_str())
                        || !generated_ids.insert(step.id.clone())
                    {
                        return Err(invalid("batch projection identities collide"));
                    }
                }
                batch_checks.insert(route.blueprint_id.as_str(), cardinality);
                if !comparison.is_empty() {
                    comparison_controls.insert(route.blueprint_id.as_str(), comparison);
                }
            }
        }
    }
    for route in routes {
        if let Some(batch) = &route.batch {
            let original = logical[route.blueprint_id.as_str()];
            let upstream_id = &original
                .for_each
                .as_ref()
                .expect("validated batch fan-out")
                .output
                .step_id;
            let upstream = logical
                .get(upstream_id.as_str())
                .ok_or_else(|| invalid("batch population bound requires a source operation"))?;
            let producer = admitted_routes.get(upstream_id.as_str()).ok_or_else(|| {
                invalid("batch population bound requires a qualified producer route")
            })?;
            validate_batch_population(original, batch, upstream, &producer.capability)?;
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
            reference.step_id = comparison_controls
                .get(reference.step_id.as_str())
                .map_or_else(
                    || projection.id.clone(),
                    |steps| steps.last().expect("batch controls").id.clone(),
                );
        }
    };
    let mut result = execution.clone();
    for step in &mut result.acquisitions {
        let blueprint = effective
            .get(step.id.as_str())
            .unwrap_or(logical[step.id.as_str()]);
        if !admitted_routes.contains_key(step.id.as_str())
            && canonical_digest(&step.arguments).map_err(Error::InvalidInput)?
                != canonical_digest(&blueprint.arguments).map_err(Error::InvalidInput)?
        {
            return Err(invalid(
                "an unavailable source without a route proof must retain its logical arguments",
            ));
        }
        // This source limit is independent of downstream evaluation ages and
        // remains binding even for unavailable operations without route proofs.
        step.max_age_seconds = blueprint.contract.time.max_age_seconds;
        step.coverage_proof = None;
        step.source.historical_interval = historical_interval(&blueprint.contract)?;
        if matches!(step.availability, SourceAvailability::Configured)
            && let Some(interval) = &step.source.historical_interval
        {
            let route = admitted_routes
                .get(step.id.as_str())
                .expect("configured source has a route proof");
            validate_historical_input(
                &route.capability,
                &step.arguments,
                &interval.start,
                &interval.end,
            )
            .map_err(|error| {
                invalid(&format!(
                    "historical input differs from its original acquisition contract: {error}"
                ))
            })?;
        }
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
                if let Some(output) = &mut predicate.equals_output {
                    rewrite(output);
                }
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
    let mut controls = comparison_controls
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    controls.extend(batch_checks.values().flatten().cloned());
    for step in &mut controls {
        for reference in &mut step.inputs {
            rewrite(reference);
        }
    }
    result.derivations = projections.values().cloned().collect();
    result.derivations.extend(controls);
    result.derivations.extend(authored_derivations);
    result.evaluations = elaboration.evaluations.clone();
    // Do not rewrite references to authored derivations: their meaning and output
    // pointers stay exactly as authored, including acceptance and gating clauses.
    for evaluation in &mut result.evaluations {
        let cardinality_checks = batch_checks
            .iter()
            .filter(|(source, _)| batch::evaluation_depends_on(elaboration, evaluation, source))
            .map(|(_, checks)| OutputReference {
                step_id: checks.last().expect("batch cardinality check").id.clone(),
                pointer: String::new(),
            })
            .collect::<Vec<_>>();
        for reference in evaluation
            .inputs
            .iter_mut()
            .chain(&mut evaluation.applicability)
            .chain(&mut evaluation.challenges)
            .chain(&mut evaluation.coverage)
        {
            rewrite(reference);
        }
        evaluation.applicability.extend(cardinality_checks);
    }
    Ok(result)
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("SUPPORT_EXECUTION_PROJECTION: {message}"))
}

fn logical_quantity_pointer(name: &str) -> String {
    format!("/{}", name.replace('~', "~0").replace('/', "~1"))
}

/// The original observation interval is independent of physical request details.
pub fn historical_interval(
    contract: &SupportContract,
) -> Result<Option<eal_collector_contracts::execution::HistoricalInterval>, Error> {
    contract
        .time
        .historical_interval
        .as_ref()
        .map(|interval| {
            let encode = |seconds| {
                chrono::DateTime::from_timestamp(seconds, 0)
                    .map(|time| time.to_rfc3339())
                    .ok_or_else(|| {
                        invalid("historical interval is outside supported timestamp range")
                    })
            };
            Ok(eal_collector_contracts::execution::HistoricalInterval {
                start: encode(interval.start_unix)?,
                end: encode(interval.end_unix)?,
            })
        })
        .transpose()
}
