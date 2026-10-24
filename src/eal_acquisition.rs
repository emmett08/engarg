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
const OBSERVATION_MAX_AGE_SECONDS: u64 = 86_400;
const PLAN_MARKER: &str = "// engarg_acquisition_plan";

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
    let projection = AcquisitionProjection::new(case, plans, catalogue, false)?;
    let projected = eal_export::export_projection_with_acquisitions(request, case, &projection)?;
    let registry = projection.registry()?;
    Ok(ExportArtifacts {
        bundle: projected.bundle,
        tools_toml: registry.to_toml()?,
        registry,
        review_requests: projected.review_requests,
    })
}

/// Verify persisted source under its declared projection semantics and original producer metadata.
pub fn reconstruct_bundle(bundle: &EalExport) -> Result<EalExport, Error> {
    let legacy = eal_export::ExportIdentity::for_bundle(bundle)?.is_legacy();
    let catalogue = bundle
        .collector_catalogue
        .as_ref()
        .map(|value| CollectorCatalogue::from_json(&serde_json::to_vec(value)?))
        .transpose()?;
    let plans = plan_from_bundle(bundle)?;
    let projection = plans
        .as_deref()
        .map(|plans| {
            AcquisitionProjection::new(&bundle.original_case, plans, catalogue.as_ref(), legacy)
        })
        .transpose()?;
    Ok(eal_export::reconstruct_projection(bundle, projection.as_ref())?.bundle)
}

/// Recover the exact exporter-owned plan without enlarging EAL observation context.
///
/// Historical bundles have no marker. Duplicate or malformed markers are errors
/// so a verifier cannot silently select one of several conflicting plans.
pub fn plan_from_bundle(bundle: &EalExport) -> Result<Option<Vec<AcquisitionPlan>>, Error> {
    let mut markers = bundle
        .source
        .lines()
        .filter(|line| line.starts_with(PLAN_MARKER));
    let Some(marker) = markers.next() else {
        return Ok(None);
    };
    if markers.next().is_some() {
        return Err(Error::InvalidInput(
            "EAL bundle repeats its acquisition plan marker".into(),
        ));
    }
    let content = marker
        .strip_prefix(PLAN_MARKER)
        .and_then(|suffix| suffix.strip_prefix(' '))
        .ok_or_else(|| Error::InvalidInput("malformed EAL acquisition plan marker".into()))?;
    let plans: Vec<AcquisitionPlan> = serde_json::from_str(content)
        .map_err(|_| Error::InvalidInput("malformed EAL acquisition plan".into()))?;
    let catalogue = bundle
        .collector_catalogue
        .as_ref()
        .map(|value| CollectorCatalogue::from_json(&serde_json::to_vec(value)?))
        .transpose()?;
    let errors =
        acquisition::validate_with_catalogue(&bundle.original_case, &plans, catalogue.as_ref());
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    Ok(Some(plans))
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
    legacy: bool,
}

impl AcquisitionProjection {
    pub(crate) fn plan_comment(&self) -> String {
        format!(
            "{PLAN_MARKER} {}\n",
            serde_json::to_string(&self.plans).expect("acquisition plans serialise")
        )
    }

    fn new(
        case: &ArgumentCase,
        plans: &[AcquisitionPlan],
        selected: Option<&CollectorCatalogue>,
        legacy: bool,
    ) -> Result<Self, Error> {
        // Historical reports explicitly naming bundled native collectors remain exportable.
        // Current planning only sees a catalogue selected by the operator.
        let catalogue = if let Some(selected) = selected {
            selected.validate()?;
            Some(selected.clone())
        } else if !legacy
            && plans
                .iter()
                .any(|plan| matches!(plan.tool_id.as_str(), "kubernetes" | "github" | "tempo"))
        {
            Some(CollectorCatalogue::from_json(include_bytes!(
                "../contracts/collectors/native.json"
            ))?)
        } else {
            None
        };
        let plans = if plans.is_empty() {
            case.evidence_plan
                .iter()
                .map(|need| AcquisitionPlan {
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
            legacy,
        })
    }

    pub(crate) fn catalogue_value(&self) -> Option<Value> {
        self.catalogue
            .as_ref()
            .map(|catalogue| serde_json::to_value(catalogue).expect("catalogue serialises"))
    }

    pub(crate) fn identity(&self) -> Result<Vec<u8>, Error> {
        Ok(serde_json::to_vec(&(&self.plans, self.catalogue_value()))?)
    }

    pub(crate) fn declarations(&self, revision: &str, environment: &str) -> Result<String, Error> {
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
                tool.acquisition_predicate.as_str()
            } else if self.legacy
                && let Some(tool) = crate::legacy_native_collectors::spec(&entry.plan.tool_id)
            {
                input = tool.input(&entry.plan)?;
                source.push_str(&format!(
                    "// engarg_evidence_need {}\n",
                    serde_json::to_string(&entry.need)?
                ));
                tool.predicate()
            } else {
                "collected == true"
            };
            source.push_str(&format!(
                "tool {} {{\n  version {}\n}}\n\nevidence {} {{\n  tool {}\n  kind acquisition\n  environment {environment}\n  max_age {OBSERVATION_MAX_AGE_SECONDS}\n  input {input}\n  require {predicate}\n}}\n\n",
                entry.tool_name, quoted(descriptor.map_or("1", |tool| tool.version.as_str())), entry.evidence_name, entry.tool_name
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
                kind: "command".into(),
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
