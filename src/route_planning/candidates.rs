//! Interface relevance and missing-route classification never rely on tool prose.
use super::{DiscoverySnapshot, GapKind};
use crate::support_requirements::AcquisitionBlueprint;
use eal_collector_contracts::capability::{CapabilityCatalogue, SupportType};

pub fn missing(
    blueprint: &AcquisitionBlueprint,
    catalogue: &CapabilityCatalogue,
    discovery: &DiscoverySnapshot,
) -> (GapKind, &'static str) {
    if discovery.incomplete {
        return (
            GapKind::DiscoveryIncomplete,
            "The configured discovery boundary was not completed; implementation is deferred until candidate comparison is complete.",
        );
    }
    let relevant = |capability: &eal_collector_contracts::capability::CapabilityDescriptor| {
        capability.semantics.as_ref().is_some_and(|semantics| {
            semantics
                .subject_kinds
                .contains(&blueprint.contract.scope.kind)
                && semantics
                    .support_types
                    .contains(&blueprint.contract.support_type)
                && semantics
                    .methods
                    .iter()
                    .any(|method| blueprint.contract.acceptable_methods.contains(method))
        })
    };
    let unqualified = catalogue.capabilities.iter().any(|capability| {
        capability.semantics.is_none()
            && jsonschema::validator_for(&capability.input_schema)
                .is_ok_and(|validator| validator.is_valid(&blueprint.arguments))
    });
    if unqualified {
        return (
            GapKind::Qualification,
            "An interface accepts these inputs but lacks an independently qualified measurement contract. Fixture tests cannot establish empirical adequacy.",
        );
    }
    if catalogue.capabilities.iter().any(relevant) {
        return (
            GapKind::CapabilityMismatch,
            "Related qualified capabilities do not preserve the required method, scope, units, coverage, time or provenance; their incompatibilities remain explicit.",
        );
    }
    (
        if matches!(
            blueprint.contract.support_type,
            SupportType::Calculation | SupportType::ProofCheck
        ) {
            GapKind::TransformationImplementation
        } else {
            GapKind::OperationImplementation
        },
        "No related qualified capability supplies this operation within the completed discovery boundary. A parameterised implementation requires separate qualification before evidence use.",
    )
}
