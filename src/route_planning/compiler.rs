//! Compile qualified source routes while preserving the logical graph.
use super::*;
use crate::{
    Error,
    domain::ArgumentCase,
    support_graph::{AcquisitionRouteProof, project_execution_graph},
    support_requirements::SupportElaboration,
};
use eal_collector_contracts::{capability::*, execution::*};
use serde_json::json;

pub(super) fn compile_fixed(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    catalogue: &CapabilityCatalogue,
    config: &PlanningConfig,
    discovery: &DiscoverySnapshot,
    batches: &std::collections::BTreeMap<String, crate::support_graph::BatchRouteProof>,
) -> Result<RoutePlan, Error> {
    let errors = crate::support_requirements::validate_elaboration(case, elaboration);
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    catalogue.validate().map_err(Error::InvalidInput)?;
    config.validate()?;
    let original_elaboration = elaboration;
    let mut effective = elaboration.clone();
    let mut allowed = std::collections::BTreeMap::new();
    for blueprint in &mut effective.acquisitions {
        if let Some(batch) = batches.get(&blueprint.id) {
            let capability = catalogue
                .capabilities
                .iter()
                .find(|capability| capability.id == batch.correspondence.batch_capability_id)
                .ok_or_else(|| {
                    Error::InvalidInput("batch capability is absent from the catalogue".into())
                })?;
            *blueprint =
                crate::support_graph::effective_blueprint(blueprint, capability, Some(batch))?;
            allowed.insert(
                blueprint.id.clone(),
                std::collections::BTreeSet::from([capability.id.clone()]),
            );
        }
    }
    for (id, batch) in batches {
        let original = original_elaboration
            .acquisitions
            .iter()
            .find(|blueprint| &blueprint.id == id)
            .expect("batch source");
        let upstream_id = &original
            .for_each
            .as_ref()
            .expect("batch fan-out")
            .output
            .step_id;
        let upstream = original_elaboration
            .acquisitions
            .iter()
            .find(|blueprint| &blueprint.id == upstream_id)
            .ok_or_else(|| {
                Error::InvalidInput(
                    "batch population bound requires a directly qualified source".into(),
                )
            })?;
        let admitted = catalogue
            .capabilities
            .iter()
            .filter(|capability| {
                crate::support_graph::validate_batch_population(
                    original, batch, upstream, capability,
                )
                .is_ok()
            })
            .map(|capability| capability.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let entry = allowed
            .entry(upstream.id.clone())
            .or_insert_with(|| admitted.clone());
        *entry = entry.intersection(&admitted).cloned().collect();
        if entry.is_empty() {
            return Err(Error::InvalidInput("batch list source cannot establish the authored fan-out cardinality and item schema".into()));
        }
    }
    let elaboration = &effective;
    let (prepared, selection) =
        selection::prepare(elaboration, catalogue, config, discovery, &allowed)?;
    let mut result = RoutePlan {
        schema: ROUTE_PLAN_SCHEMA.into(),
        execution: EvidenceExecutionPlan {
            schema: PLAN_SCHEMA.into(),
            acquisitions: vec![],
            derivations: elaboration.derivations.clone(),
            evaluations: elaboration.evaluations.clone(),
            scheduling: config.scheduling.clone(),
        },
        routes: vec![],
        projections: vec![],
        gaps: vec![],
        discovery: discovery.clone(),
        configuration: config.clone(),
        selection: Some(selection),
    };
    for alternative in crate::support_requirements::legacy_alternative_gaps(case, elaboration) {
        result.gaps.push(PlanningGap {
            acquisition_id: None,
            requirement_id: None,
            kind: GapKind::Clarification,
            description: format!(
                "Legacy support planning has no obligation for {}: {}",
                alternative.alternative_id, alternative.distinguishing_test
            ),
            implementation: None,
        });
    }
    for requirement in &elaboration.requirements {
        for gap in &requirement.gaps {
            result.gaps.push(PlanningGap {
                acquisition_id: None,
                requirement_id: Some(requirement.id.clone()),
                kind: match gap.kind {
                    crate::support_requirements::RequirementGapKind::Clarification => {
                        GapKind::Clarification
                    }
                    crate::support_requirements::RequirementGapKind::HistoricalUnavailable => {
                        GapKind::HistoricalUnavailable
                    }
                    crate::support_requirements::RequirementGapKind::Judgement => {
                        GapKind::Judgement
                    }
                    _ => GapKind::SourceUnavailable,
                },
                description: gap.description.clone(),
                implementation: None,
            });
        }
    }
    for (blueprint, prepared) in elaboration.acquisitions.iter().zip(prepared) {
        let ranked = prepared.ranked;
        let assessments = prepared.assessments;
        let selected = prepared
            .selected
            .as_ref()
            .and_then(|id| catalogue.capabilities.iter().find(|cap| cap.id == *id));
        let Some(capability) = selected else {
            let (mut kind, description) = candidates::missing(blueprint, catalogue, discovery);
            let mut description = description.to_owned();
            let implementation = if matches!(
                kind,
                GapKind::OperationImplementation | GapKind::TransformationImplementation
            ) {
                match implementation::configured_request(blueprint, elaboration, config) {
                    Ok(request) => Some(request),
                    Err(Error::InvalidInput(reason)) => {
                        kind = GapKind::Clarification;
                        description = reason;
                        None
                    }
                    Err(error) => return Err(error),
                }
            } else {
                None
            };
            result.gaps.push(PlanningGap {
                acquisition_id: Some(blueprint.id.clone()),
                requirement_id: None,
                kind,
                description: description.clone(),
                implementation,
            });
            result.execution.acquisitions.push(AcquisitionStep {
                id: blueprint.id.clone(),
                max_age_seconds: blueprint.contract.time.max_age_seconds,
                source: SourceIdentity {
                    provider_id: "unavailable".into(),
                    capability_id: "unavailable".into(),
                    capability_version: "0".into(),
                    input_schema_sha256: "0".repeat(64),
                    output_schema_sha256: "0".repeat(64),
                    qualified_contract_sha256: canonical_identity(&blueprint.contract)
                        .map_err(Error::InvalidInput)?,
                    configuration_sha256: "0".repeat(64),
                    authentication_scope_sha256: "0".repeat(64),
                    scope: json!(blueprint.contract.scope.selectors),
                    historical_interval: arguments::historical_interval(&blueprint.contract)?,
                },
                arguments: blueprint.arguments.clone(),
                bindings: blueprint.bindings.clone(),
                for_each: blueprint.for_each.clone(),
                availability: SourceAvailability::Unavailable {
                    reason: description.clone(),
                },
                reuse: ReusePolicy::FreshOrAcquire,
            });
            result.routes.push(AcquisitionRoute {
                acquisition_id: blueprint.id.clone(),
                selected: None,
                candidates: assessments,
                ranked,
                configuration_gaps: vec![description],
            });
            continue;
        };
        let binding = config
            .bindings
            .iter()
            .find(|binding| binding.capability_id == capability.id);
        let (mut arguments, mut gaps) = arguments::materialise(blueprint, capability, binding)?;
        let mappings_resolved = binding.is_none_or(|binding| {
            binding.input_bindings.iter().all(|(physical, logical)| {
                arguments.pointer(physical).is_some()
                    && blueprint.arguments.pointer(logical).is_some()
            })
        });
        if !mappings_resolved {
            gaps.push("Input projection contains a conflicting or unresolved physical destination. The pending graph retains logical inputs until configuration is corrected.".into());
            arguments = blueprint.arguments.clone();
        }
        if !discovery.admitted_capabilities.contains(&capability.id) {
            gaps.push("Capability is qualified but its connection, credentials or current interface have not been admitted by discovery.".into());
        }
        let mut scope = capability
            .semantics
            .as_ref()
            .map(|semantics| semantics.scope_constraints.clone())
            .unwrap_or_default();
        scope.extend(blueprint.contract.scope.selectors.clone());
        let source = SourceIdentity {
            provider_id: capability.provider.id.clone(),
            capability_id: capability.id.clone(),
            capability_version: capability.version.clone(),
            input_schema_sha256: capability.input_schema_sha256.clone(),
            output_schema_sha256: capability.output_schema_sha256.clone(),
            qualified_contract_sha256: capability
                .measurement_identity()
                .map_err(Error::InvalidInput)?,
            configuration_sha256: capability.provider.configuration_sha256.clone(),
            authentication_scope_sha256: canonical_identity(
                &capability.provider.authentication_scope,
            )
            .map_err(Error::InvalidInput)?,
            scope: json!(scope),
            historical_interval: arguments::historical_interval(&blueprint.contract)?,
        };
        for gap in &gaps {
            let source_record_gap = (capability.provider.transport
                == TransportKind::RetainedArtifact
                && !discovery.admitted_capabilities.contains(&capability.id))
                || capability
                    .configuration_requirements
                    .iter()
                    .any(|requirement| {
                        requirement.kind == ConfigurationKind::SourceRecord
                            && gap.starts_with(&format!("{}:", requirement.id))
                    });
            result.gaps.push(PlanningGap {
                acquisition_id: Some(blueprint.id.clone()),
                requirement_id: None,
                kind: if source_record_gap {
                    GapKind::SourceUnavailable
                } else {
                    GapKind::Configuration
                },
                description: gap.clone(),
                implementation: None,
            });
        }
        result.execution.acquisitions.push(AcquisitionStep {
            id: blueprint.id.clone(),
            max_age_seconds: blueprint.contract.time.max_age_seconds,
            source,
            arguments,
            bindings: blueprint.bindings.clone(),
            for_each: blueprint.for_each.clone(),
            availability: if gaps.is_empty() {
                SourceAvailability::Configured
            } else {
                SourceAvailability::ConfigurationMissing {
                    requirements: gaps.clone(),
                }
            },
            reuse: ReusePolicy::FreshOrAcquire,
        });
        let has_quantities = capability
            .semantics
            .as_ref()
            .is_some_and(|s| !s.quantities.is_empty());
        if mappings_resolved {
            result.projections.push(AcquisitionRouteProof {
                batch: batches.get(&blueprint.id).cloned(),
                blueprint_id: blueprint.id.clone(),
                capability: capability.clone(),
                operator_arguments: binding.map(|binding| binding.arguments.clone()),
                input_bindings: binding
                    .map(|b| b.input_bindings.clone())
                    .unwrap_or_default(),
                projection_id: has_quantities.then(|| {
                    format!(
                        "engarg_projection_{}",
                        &canonical_identity(&blueprint.id).expect("string serialises")[..24]
                    )
                }),
            });
        }
        result.routes.push(AcquisitionRoute {
            acquisition_id: blueprint.id.clone(),
            selected: Some(capability.id.clone()),
            candidates: assessments,
            ranked,
            configuration_gaps: gaps,
        });
    }
    project_execution_graph(
        original_elaboration,
        &result.projections,
        &mut result.execution,
    )?;
    result.execution.validate().map_err(Error::InvalidInput)?;
    Ok(result)
}
