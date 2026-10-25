//! EAL/3 support gates over an unchanged, canonical authored projection.
//!
//! The outer version binds the source graph, collector locations and generated
//! gates. The inner v3 projection retains its historical identity and semantics.
mod projection;
mod types;

pub use crate::support_graph::{AcquisitionRouteProof, project_execution_graph};
pub use types::*;

use crate::{
    Error,
    domain::{ArgumentCase, Request},
    eal_export::{self, ExportIdentity},
    export_policy::ExportPolicy,
    support_requirements::{SupportElaboration, validate_elaboration},
};
use eal_collector_contracts::execution::{EvidenceExecutionPlan, canonical_digest};

pub const SUPPORT_EXPORT_SCHEMA: &str = "engarg.support-export/1";
pub const SUPPORT_PROJECTION_SEMANTICS: &str = "engarg.support-projection/1";
pub const SUPPORT_ACQUISITION_SCHEMA: &str = "engarg.support-acquisition/1";

/// Paths are operator-selected final locations, before any bundle identity is made.
pub fn export(
    request: &Request,
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    execution: &EvidenceExecutionPlan,
    routes: &[AcquisitionRouteProof],
    locations: &SupportExportLocations,
    policy: &ExportPolicy,
) -> Result<SupportExportArtifacts, Error> {
    let resolved = policy.resolve(case)?;
    let authored = eal_export::export_projection_with_policy_identity(
        request,
        case,
        None,
        ExportIdentity::current(),
        &resolved,
    )?;
    projection::build(
        authored,
        elaboration,
        execution,
        routes,
        locations,
        env!("CARGO_PKG_VERSION"),
    )
}

/// Reconstruct all generated source and registry fields before trusting a bundle.
pub fn reconstruct(bundle: &SupportExport) -> Result<SupportExportArtifacts, Error> {
    if bundle.schema_version != SUPPORT_EXPORT_SCHEMA
        || bundle.semantics_version != SUPPORT_PROJECTION_SEMANTICS
        || bundle.exporter_version.trim().is_empty()
        || bundle.exporter_version.len() > 128
        || bundle.exporter_version.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput(
            "SUPPORT_EXPORT_SCHEMA: unsupported support export identity".into(),
        ));
    }
    crate::eal_acquisition::reconstruct_bundle(&bundle.authored)?;
    let authored = eal_export::reconstruct_projection(&bundle.authored, None)?;
    let rebuilt = projection::build(
        authored,
        &bundle.support_elaboration,
        &bundle.execution_plan,
        &bundle.route_proofs,
        &bundle.locations,
        &bundle.exporter_version,
    )?;
    if canonical_digest(&rebuilt.bundle).map_err(Error::InvalidInput)?
        != canonical_digest(bundle).map_err(Error::InvalidInput)?
    {
        return Err(Error::InvalidInput("SUPPORT_EXPORT_IDENTITY_MISMATCH: source, graph, bindings or metadata differ from the deterministic support projection".into()));
    }
    Ok(rebuilt)
}

pub fn reconstruct_bundle(bundle: &SupportExport) -> Result<SupportExport, Error> {
    Ok(reconstruct(bundle)?.bundle)
}

fn validate_support(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    execution: &EvidenceExecutionPlan,
    routes: &[AcquisitionRouteProof],
) -> Result<(), Error> {
    let errors = validate_elaboration(case, elaboration);
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    execution.validate().map_err(Error::InvalidInput)?;
    crate::support_graph::validate_execution_graph(elaboration, routes, execution)
}
