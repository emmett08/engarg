//! Bind prospective evidence collection to EAL support without accepting its findings.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::{
    Error,
    acquisition::{self, AcquisitionPlan},
    collector_catalogue::{CollectorCatalogue, RegistryCommand, ToolsRegistry},
    domain::{ArgumentCase, EvidenceNeed, Request},
    eal_export::{self, EalExport, name, quoted},
};

const SCHEMA: &str = "engarg.eal-acquisition/1";

/// The existing JSON bundle and its separate native EAL collector registry.
pub struct ExportArtifacts {
    pub bundle: EalExport,
    pub tools_toml: String,
    pub registry: ToolsRegistry,
    pub review_requests: Vec<eal_export::ReviewRequest>,
}

/// Export every evidence need, with file stubs when an adapter or input is unresolved.
///
/// An empty plan supports historical cases by making each need a custom stub.
/// A supplied plan must bind every need exactly once. Collection availability
/// and the existing independent inference review remain separate conditions.
pub fn export_with_plan(
    request: &Request,
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
) -> Result<ExportArtifacts, Error> {
    export_with_catalogue(request, case, plans, None)
}

pub fn export_with_catalogue(
    request: &Request,
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
    catalogue: Option<&CollectorCatalogue>,
) -> Result<ExportArtifacts, Error> {
    export_with_policy(
        request,
        case,
        plans,
        catalogue,
        &crate::export_policy::ExportPolicy::default(),
    )
}

pub fn export_with_policy(
    request: &Request,
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
    catalogue: Option<&CollectorCatalogue>,
    policy: &crate::export_policy::ExportPolicy,
) -> Result<ExportArtifacts, Error> {
    let policy = policy.resolve(case)?;
    let projection = AcquisitionProjection::new(case, plans, catalogue)?;
    let projected = eal_export::export_projection_with_policy_identity(
        request,
        case,
        Some(&projection),
        eal_export::ExportIdentity::current(),
        &policy,
    )?;
    let registry = projection.registry()?;
    Ok(ExportArtifacts {
        bundle: projected.bundle,
        tools_toml: registry.to_toml().map_err(Error::InvalidInput)?,
        registry,
        review_requests: projected.review_requests,
    })
}

/// Verify persisted source under its declared projection semantics and original producer metadata.
pub fn reconstruct_bundle(bundle: &EalExport) -> Result<EalExport, Error> {
    if bundle.schema_version != "engarg.eal-export/3" {
        let old = serde_json::from_value(serde_json::to_value(bundle)?)?;
        return Ok(serde_json::from_value(serde_json::to_value(
            crate::legacy_acquisition::reconstruct_bundle(&old)?,
        )?)?);
    }
    eal_export::ExportIdentity::for_bundle(bundle)?;
    let catalogue = bundle
        .collector_catalogue
        .as_ref()
        .map(|value| CollectorCatalogue::from_json(&serde_json::to_vec(value)?))
        .transpose()?;
    let plans = plan_from_bundle(bundle)?;
    let projection = plans
        .as_deref()
        .map(|plans| AcquisitionProjection::new(&bundle.original_case, plans, catalogue.as_ref()))
        .transpose()?;
    let rebuilt = eal_export::reconstruct_projection(bundle, projection.as_ref())?.bundle;
    if serde_json::to_value(&rebuilt)? != serde_json::to_value(bundle)? {
        return Err(Error::InvalidInput(
            "EXPORT_IDENTITY_MISMATCH: EAL bundle differs from the current deterministic export"
                .into(),
        ));
    }
    Ok(rebuilt)
}

/// Recover the exact exporter-owned plan without enlarging EAL observation context.
///
/// Historical bundles have no marker. Duplicate or malformed markers are errors
/// so a verifier cannot silently select one of several conflicting plans.
pub fn plan_from_bundle(bundle: &EalExport) -> Result<Option<Vec<AcquisitionPlan>>, Error> {
    if bundle.schema_version != "engarg.eal-export/3" {
        let old = serde_json::from_value(serde_json::to_value(bundle)?)?;
        return crate::legacy_acquisition::plan_from_bundle(&old);
    }
    eal_export::ExportIdentity::for_bundle(bundle)?;
    let plan = bundle
        .acquisition_plan
        .as_ref()
        .expect("validated structural plan");
    Ok(plan.enabled.then(|| plan.entries.clone()))
}

struct Entry {
    need: EvidenceNeed,
    plan: AcquisitionPlan,
    tool_name: String,
    evidence_name: String,
}

pub(crate) struct AcquisitionProjection {
    pub(crate) plans: Vec<AcquisitionPlan>,
    entries: Vec<Entry>,
    catalogue: Option<CollectorCatalogue>,
}

impl AcquisitionProjection {
    fn new(
        case: &ArgumentCase,
        plans: &[AcquisitionPlan],
        selected: Option<&CollectorCatalogue>,
    ) -> Result<Self, Error> {
        // New projections consume only operator-selected contracts.
        // Historical readers own their frozen infrastructure defaults.
        let catalogue = selected
            .map(|selected| {
                selected.validate()?;
                Ok::<_, Error>(selected.clone())
            })
            .transpose()?;
        let plans = if plans.is_empty() {
            case.evidence_plan
                .iter()
                .map(|need| AcquisitionPlan {
                    input_preparation: None,
                    need_id: need.id.clone(),
                    tool_id: "custom".into(),
                    arguments: Vec::new(),
                    unresolved_arguments: vec!["adapter".into()],
                })
                .collect()
        } else {
            plans.to_vec()
        };
        let errors = acquisition::validate_with_catalogue(case, &plans, catalogue.as_ref());
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
        let mut by_need: BTreeMap<_, _> = plans
            .into_iter()
            .map(|mut plan| {
                plan.arguments
                    .sort_by(|left, right| left.name.cmp(&right.name));
                plan.unresolved_arguments.sort();
                (plan.need_id.clone(), plan)
            })
            .collect();
        let entries: Vec<_> = case
            .evidence_plan
            .iter()
            .map(|need| Entry {
                need: need.clone(),
                plan: by_need.remove(&need.id).expect("validated need binding"),
                tool_name: name("acquisition_tool", &need.id),
                evidence_name: name("acquisition", &need.id),
            })
            .collect();
        Ok(Self {
            plans: entries.iter().map(|entry| entry.plan.clone()).collect(),
            entries,
            catalogue,
        })
    }

    pub(crate) fn catalogue_value(&self) -> Option<Value> {
        self.catalogue
            .as_ref()
            .map(|catalogue| serde_json::to_value(catalogue).expect("catalogue serialises"))
    }

    pub(crate) fn declarations(
        &self,
        revision: &str,
        environment: &str,
        policy: &crate::export_policy::ExportPolicy,
    ) -> Result<String, Error> {
        let mut source = String::new();
        for entry in &self.entries {
            let arguments: BTreeMap<_, _> = entry
                .plan
                .arguments
                .iter()
                .map(|argument| (&argument.name, &argument.value))
                .collect();
            let mut input = json!({
                "schema": SCHEMA,
                "case_digest": revision,
                "need": entry.need,
                "tool_id": entry.plan.tool_id,
                "arguments": arguments,
                "unresolved_arguments": entry.plan.unresolved_arguments,
            });
            let descriptor = self
                .catalogue
                .as_ref()
                .and_then(|catalogue| catalogue.get(&entry.plan.tool_id));
            let predicate = if let Some(tool) = descriptor {
                input = tool.input(&entry.plan)?;
                source.push_str(&format!(
                    "// engarg_evidence_need {}\n",
                    serde_json::to_string(&entry.need)?
                ));
                tool.0.acquisition_predicate.as_str()
            } else {
                eal_adapters::profiles::get(&entry.plan.tool_id)
                    .map_or("collected == true", |profile| profile.acquisition_predicate)
            };
            source.push_str(&format!(
                "tool {} {{\n  version {}\n}}\n\nevidence {} {{\n  tool {}\n  kind acquisition\n  environment {environment}\n  max_age {observation_age}\n  input {input}\n  require {predicate}\n}}\n\n",
                entry.tool_name, quoted(descriptor.map_or_else(|| eal_adapters::profiles::get(&entry.plan.tool_id).map_or("1",|profile|profile.version), |tool| tool.0.version.as_str())), entry.evidence_name, entry.tool_name, observation_age=policy.acquisition_age(&entry.need.id)
            ));
        }
        Ok(source)
    }

    pub(crate) fn mappings(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.entries.iter().map(|entry| {
            (
                entry.need.id.as_str(),
                entry.tool_name.as_str(),
                entry.evidence_name.as_str(),
            )
        })
    }

    pub(crate) fn attach(&self, targets: &BTreeSet<&str>, evidence: &mut BTreeSet<String>) {
        evidence.extend(
            self.entries
                .iter()
                .filter(|entry| targets.contains(entry.need.target_id.as_str()))
                .map(|entry| entry.evidence_name.clone()),
        );
    }

    pub(crate) fn review_inputs(&self, target_id: &str) -> Vec<Value> {
        self.entries
            .iter()
            .filter(|entry| entry.need.target_id == target_id)
            .map(|entry| {
                json!({
                    "need_id": entry.need.id,
                    "need": entry.need,
                    "evidence_id": entry.evidence_name,
                    "interpretation": "Independently assess the observation against the evidence need and its comparator and acceptance criterion. Successful acquisition establishes the recorded measurement only."
                })
            })
            .collect()
    }

    fn registry(&self) -> Result<ToolsRegistry, Error> {
        let mut registry = ToolsRegistry::default();
        registry.tools.insert(
            "engarg_review_contract".into(),
            RegistryCommand {
                kind: eal_collector_contracts::ToolKind::Command,
                version: "1".into(),
                argv: vec!["/REPLACE_WITH_INDEPENDENT_REVIEW_COLLECTOR".into()],
                timeout_seconds: 65,
                max_output_bytes: 4_194_304,
                inherit_env: Vec::new(),
                parallel_safe: false,
            },
        );
        for entry in &self.entries {
            let binding = if let Some(descriptor) = self
                .catalogue
                .as_ref()
                .and_then(|catalogue| catalogue.get(&entry.plan.tool_id))
            {
                descriptor.binding(&entry.plan)?
            } else {
                crate::collector_registry::binding(&entry.plan, &entry.need.id)?
            };
            registry.tools.insert(entry.tool_name.clone(), binding);
        }
        Ok(registry)
    }
}
