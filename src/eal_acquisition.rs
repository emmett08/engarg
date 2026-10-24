//! Bind prospective evidence collection to EAL support without accepting its findings.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::{
    Error,
    acquisition::{self, AcquisitionPlan},
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
    let projection = AcquisitionProjection::new(case, plans)?;
    let bundle = eal_export::export_with_acquisitions(request, case, &projection)?;
    Ok(ExportArtifacts {
        bundle,
        tools_toml: projection.registry(),
    })
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
    let errors = acquisition::validate(&bundle.original_case, &plans);
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
}

impl AcquisitionProjection {
    pub(crate) fn plan_comment(&self) -> String {
        format!(
            "{PLAN_MARKER} {}\n",
            serde_json::to_string(&self.plans).expect("acquisition plans serialise")
        )
    }

    fn new(case: &ArgumentCase, plans: &[AcquisitionPlan]) -> Result<Self, Error> {
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
        let errors = acquisition::validate(case, &plans);
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
        })
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
            let predicate = if let Some(tool) = crate::native_collectors::spec(&entry.plan.tool_id)
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
                "tool {} {{\n  version \"1\"\n}}\n\nevidence {} {{\n  tool {}\n  kind acquisition\n  environment {environment}\n  max_age {OBSERVATION_MAX_AGE_SECONDS}\n  input {input}\n  require {predicate}\n}}\n\n",
                entry.tool_name, entry.evidence_name, entry.tool_name
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

    fn registry(&self) -> String {
        let mut registry = String::from(
            "# Native eal-mcp tool registry for this exact exported case.\n# Exporting this file performs no collection.\n# Commands are explicit operator configuration.\n# Replace review stubs with a trusted reviewer that handles each exact request.\n# Each command receives an EAL acquisition JSON request on stdin.\n# Successful collectors return the native EAL observation envelope.\n# REPLACE selectors and executable paths require operator configuration.\n# Acquisition results require independent interpretation against their acceptance criteria.\n\n[tools.engarg_review_contract]\nkind = \"command\"\nversion = \"1\"\nargv = [\"/REPLACE_WITH_INDEPENDENT_REVIEW_COLLECTOR\"]\ntimeout_seconds = 65\nmax_output_bytes = 4194304\n# Stub for independent review. Each review has a different acquisition request.\n# The reviewer must assess the exact request and return its own acceptance decision.\n\n",
        );
        for entry in &self.entries {
            registry.push_str(&format!(
                "# Evidence need {}\n# Target {}\n# Proposed collector {}\n",
                quoted(&entry.need.id),
                quoted(&entry.need.target_id),
                quoted(&entry.plan.tool_id)
            ));
            for argument in &entry.plan.arguments {
                registry.push_str(&format!(
                    "# Argument {} = {}\n",
                    quoted(&argument.name),
                    quoted(&argument.value)
                ));
            }
            for argument in &entry.plan.unresolved_arguments {
                registry.push_str(&format!("# Unresolved input {}\n", quoted(argument)));
            }
            registry.push_str(&format!("[tools.{}]\n", entry.tool_name));
            registry.push_str(&crate::collector_registry::binding(
                &entry.plan,
                &entry.need.id,
            ));
        }
        registry
    }
}
