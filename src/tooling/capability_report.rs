//! Saved capability registrations bind consumer gaps without entering package keys.
use super::capability::{ImplementedCapability, revalidate_implemented_capability};
use super::contracts::digest_json;
use crate::{
    Error,
    route_planning::{GapKind, ROUTE_PLAN_SCHEMA, RoutePlan},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const CAPABILITY_TOOLING_SCHEMA: &str = "engarg.capability-tooling/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityGapBinding {
    pub acquisition_id: Option<String>,
    pub requirement_id: Option<String>,
    pub capability_sha256: String,
    pub collector_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityToolingReport {
    pub schema: String,
    pub original_planning_sha256: String,
    pub language: String,
    pub capabilities: Vec<ImplementedCapability>,
    pub bindings: Vec<CapabilityGapBinding>,
    pub model_calls: u32,
    pub skipped_non_implementation_gaps: usize,
    pub deferred_implementation_gaps: usize,
    pub audit_path: PathBuf,
    pub note: String,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

/// Validate saved graph tooling separately from legacy per-need overrides.
pub fn revalidate_report(raw: &Value, executable: &Path) -> Result<(), Error> {
    let Some(value) = raw.get("tooling").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let report: CapabilityToolingReport = serde_json::from_value(value.clone())?;
    let planning = raw
        .get("evidence_planning")
        .ok_or_else(|| invalid("capability tooling requires its original evidence planning"))?;
    if report.schema != CAPABILITY_TOOLING_SCHEMA
        || report.original_planning_sha256 != digest_json(planning)?
    {
        return Err(invalid(
            "capability tooling differs from its original evidence planning",
        ));
    }
    let routes: RoutePlan = serde_json::from_value(planning["routes"].clone())?;
    if routes.schema != ROUTE_PLAN_SCHEMA {
        return Err(invalid("unsupported capability route plan schema"));
    }
    routes.execution.validate().map_err(Error::InvalidInput)?;
    routes.configuration.validate()?;
    let mut records = BTreeMap::new();
    let mut model_calls = 0u32;
    for capability in &report.capabilities {
        if records
            .insert(capability.capability_sha256.clone(), capability)
            .is_some()
            || capability.qualification.limitations.is_empty()
            || capability.reused && capability.model_calls != 0
            || !capability.reused && !matches!(capability.model_calls, 1 | 2)
        {
            return Err(invalid(
                "capability tooling repeats a package or changes implementation accounting",
            ));
        }
        let verified = revalidate_implemented_capability(capability, executable)?;
        let package = super::package::load_verified(&verified.package_manifest)?;
        if package.contract().language != report.language {
            return Err(invalid(
                "capability tooling language differs from its verified package",
            ));
        }
        model_calls = model_calls
            .checked_add(capability.model_calls)
            .ok_or_else(|| invalid("capability tooling model-call count overflow"))?;
    }
    let incomplete = routes.discovery.incomplete
        || routes
            .gaps
            .iter()
            .any(|gap| gap.kind == GapKind::DiscoveryIncomplete);
    let mut used = BTreeSet::new();
    let mut expected_bindings = Vec::new();
    let (mut skipped, mut deferred) = (0, 0);
    for gap in &routes.gaps {
        if !matches!(
            gap.kind,
            GapKind::OperationImplementation | GapKind::TransformationImplementation
        ) {
            skipped += 1;
            continue;
        }
        let Some(request) = gap.implementation.as_ref().filter(|_| !incomplete) else {
            deferred += 1;
            continue;
        };
        let key = request.identity()?;
        let capability = records
            .get(&key)
            .ok_or_else(|| invalid("capability tooling omits an eligible operation gap"))?;
        used.insert(key.clone());
        expected_bindings.push(CapabilityGapBinding {
            acquisition_id: gap.acquisition_id.clone(),
            requirement_id: gap.requirement_id.clone(),
            capability_sha256: key,
            collector_id: capability.collector.id.clone(),
        });
    }
    if used != records.keys().cloned().collect()
        || digest_json(&expected_bindings)? != digest_json(&report.bindings)?
        || skipped != report.skipped_non_implementation_gaps
        || deferred != report.deferred_implementation_gaps
        || model_calls != report.model_calls
    {
        return Err(invalid(
            "capability tooling registration or consumer bindings differ from the original gaps",
        ));
    }
    Ok(())
}
