use crate::{
    eal_export::{EalExport, EalSourceMapping, ReviewRequest},
    support_graph::AcquisitionRouteProof,
    support_requirements::SupportElaboration,
};
use eal_collector_contracts::{
    CommandBinding, CommandRegistry,
    capability::SupportType,
    execution::{EvidenceExecutionPlan, OutputReference},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportExportLocations {
    pub collector: PathBuf,
    pub execution_plan: PathBuf,
    pub connections: PathBuf,
    pub store: PathBuf,
    #[serde(default)]
    pub environment: Vec<String>,
    /// A real, separately selected independent reviewer. Absence stays pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_binding: Option<CommandBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportExport {
    pub schema_version: String,
    pub semantics_version: String,
    pub exporter_version: String,
    pub revision: String,
    pub target_language: String,
    /// The exact authored case and canonical projection, not generated nodes.
    pub authored: EalExport,
    pub support_elaboration: SupportElaboration,
    pub execution_plan: EvidenceExecutionPlan,
    /// Explicit generated guards for incomplete legacy requirement extraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternative_projection: Option<AlternativeProjection>,
    pub route_proofs: Vec<AcquisitionRouteProof>,
    pub execution_plan_sha256: String,
    pub locations: SupportExportLocations,
    pub source: String,
    pub context: Value,
    /// Canonical claim IDs resolve to generated support gates around authored claims.
    pub claim_names: BTreeMap<String, String>,
    pub source_map: BTreeMap<String, EalSourceMapping>,
    pub requirements: Vec<SupportRequirementProjection>,
    /// Exact source selectors used by typed methods. Missing selectors stay pending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_inputs: Vec<ReasoningInputProjection>,
    pub registry: CommandRegistry,
    pub pending: Vec<PendingSupport>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeProjection {
    pub schema: String,
    pub original_execution_plan: EvidenceExecutionPlan,
    pub original_execution_plan_sha256: String,
    pub guards: Vec<AlternativeGuard>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeGuard {
    pub alternative_id: String,
    pub requirement_id: String,
    pub evaluation_id: String,
    pub evidence_name: String,
    pub claim_ids: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportRequirementProjection {
    pub requirement_id: String,
    pub target_id: String,
    pub evidence_need_ids: Vec<String>,
    pub evaluation_id: String,
    pub eal_evidence_name: String,
    pub support_type: Option<SupportType>,
    /// Logical operations are shared; these are not independent observations.
    pub source_step_ids: Vec<String>,
    pub derived_step_ids: Vec<String>,
    pub gated_claim_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningInputProjection {
    pub warrant_id: String,
    pub evidence_need_id: String,
    pub eal_evidence_name: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSupport {
    pub kind: String,
    pub requirement_id: Option<String>,
    pub target_id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct SupportExportArtifacts {
    pub bundle: SupportExport,
    pub tools_toml: String,
    pub registry: CommandRegistry,
    pub review_requests: Vec<ReviewRequest>,
    pub acquisition_requests: Vec<ReviewRequest>,
    pub execution_plan: EvidenceExecutionPlan,
}
