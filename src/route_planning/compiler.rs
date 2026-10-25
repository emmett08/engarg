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

pub fn compile(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    catalogue: &CapabilityCatalogue,
    config: &PlanningConfig,
    discovery: &DiscoverySnapshot,
) -> Result<RoutePlan, Error> {
    let errors = crate::support_requirements::validate_elaboration(case, elaboration);
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    catalogue.validate().map_err(Error::InvalidInput)?;
    config.validate()?;
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
    };
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
    for blueprint in &elaboration.acquisitions {
        let ranked = rank_adequate(
            &blueprint.contract,
            &catalogue.capabilities,
            &config.ranking,
        )
        .map_err(Error::InvalidInput)?;
        let assessments: Vec<_> = catalogue
            .capabilities
            .iter()
            .map(|cap| {
                let compatibility=compatible(&blueprint.contract,cap);
                let binding=config.bindings.iter().find(|binding|binding.capability_id==cap.id);
                let mut configuration_gaps=if compatibility.adequate {arguments::materialise(blueprint,cap,binding)?.1}else{vec![]};
                if compatibility.adequate&&!discovery.admitted_capabilities.contains(&cap.id) {configuration_gaps.push("Live source admission or its connection/credential references remain unresolved".into());}
                let executable=compatibility.adequate&&configuration_gaps.is_empty();
                Ok(CandidateAssessment {capability_id:cap.id.clone(),compatibility,configuration_gaps,executable})
            })
            .collect::<Result<_,Error>>()?;
        let selected_rank = ranked
            .iter()
            .find(|rank| {
                assessments.iter().any(|candidate| {
                    candidate.capability_id == rank.capability_id && candidate.executable
                })
            })
            .or_else(|| ranked.first());
        let selected = selected_rank.and_then(|rank| {
            catalogue
                .capabilities
                .iter()
                .find(|cap| cap.id == rank.capability_id)
        });
        let Some(capability) = selected else {
            let (kind, description) = candidates::missing(blueprint, catalogue, discovery);
            let implementation = if matches!(
                kind,
                GapKind::OperationImplementation | GapKind::TransformationImplementation
            ) {
                Some(implementation::request(blueprint)?)
            } else {
                None
            };
            result.gaps.push(PlanningGap {
                acquisition_id: Some(blueprint.id.clone()),
                requirement_id: None,
                kind,
                description: description.into(),
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
                    historical_interval: None,
                },
                arguments: blueprint.arguments.clone(),
                bindings: blueprint.bindings.clone(),
                for_each: blueprint.for_each.clone(),
                availability: SourceAvailability::Unavailable {
                    reason: description.into(),
                },
                reuse: ReusePolicy::FreshOrAcquire,
            });
            result.routes.push(AcquisitionRoute {
                acquisition_id: blueprint.id.clone(),
                selected: None,
                candidates: assessments,
                ranked,
                configuration_gaps: vec![description.into()],
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
                blueprint_id: blueprint.id.clone(),
                capability: capability.clone(),
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
    project_execution_graph(elaboration, &result.projections, &mut result.execution)?;
    result.execution.validate().map_err(Error::InvalidInput)?;
    Ok(result)
}
