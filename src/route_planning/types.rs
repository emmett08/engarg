use crate::{
    support_graph::AcquisitionRouteProof, support_requirements::OperationSpecification,
    tooling::capability::CapabilityImplementationRequest,
};
use eal_collector_contracts::{capability::*, execution::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
pub const PLANNING_CONFIG_SCHEMA: &str = "engarg.capability-planning-config/1";
pub const ROUTE_PLAN_SCHEMA: &str = "engarg.evidence-routes/1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningConfig {
    pub schema: String,
    pub collector: PathBuf,
    pub connections: PathBuf,
    pub store: PathBuf,
    #[serde(default)]
    pub environment: Vec<String>,
    pub ranking: RankingPolicy,
    /// Explicit feasibility preference. Cost ranking is the default.
    #[serde(default)]
    pub prefer_configured: bool,
    pub scheduling: SchedulingPolicy,
    pub bindings: Vec<CapabilityInputBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub batch_correspondences: Vec<crate::support_graph::QualifiedBatchCorrespondence>,
    /// Operator-owned function definitions keyed by their exact versioned method.
    /// These govern implementation without rewriting the authored source graph.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub operation_specifications: BTreeMap<String, OperationSpecification>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityInputBinding {
    pub capability_id: String,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub arguments: Value,
    /// Physical argument JSON pointer -> logical input JSON pointer.
    pub input_bindings: BTreeMap<String, String>,
    /// IDs of operator-confirmed prerequisites; no credential values.
    pub configured_prerequisites: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySnapshot {
    #[serde(default)]
    pub limits: Value,
    #[serde(default)]
    pub budget_usage: Value,
    pub admitted_capabilities: Vec<String>,
    pub incomplete: bool,
    pub gaps: Vec<Value>,
    pub metrics: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePlan {
    pub schema: String,
    pub execution: EvidenceExecutionPlan,
    pub routes: Vec<AcquisitionRoute>,
    pub projections: Vec<AcquisitionRouteProof>,
    pub gaps: Vec<PlanningGap>,
    pub discovery: DiscoverySnapshot,
    pub configuration: PlanningConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<PlanSelection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSelection {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphs: Option<AcquisitionGraphComparison>,
    pub search_complete: bool,
    pub candidate_states: usize,
    /// Exact static request-group count; dynamic arguments or fan-out remain unknown.
    pub estimated_distinct_acquisitions: Option<usize>,
    /// Source-operation invocation bound without reuse discount; excludes provider-internal requests.
    /// Fan-out contributes its configured maximum.
    #[serde(default)]
    pub estimated_distinct_acquisitions_upper_bound: Option<usize>,
    #[serde(default)]
    pub dynamic_acquisition_groups: usize,
    /// Unknown estimates remain counted separately from known values.
    pub total_costs: Vec<(usize, u128)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRoute {
    pub acquisition_id: String,
    pub selected: Option<String>,
    pub candidates: Vec<CandidateAssessment>,
    pub ranked: Vec<RankedCapability>,
    pub configuration_gaps: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateAssessment {
    pub capability_id: String,
    pub compatibility: CompatibilityResult,
    pub configuration_gaps: Vec<String>,
    pub executable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    OperationImplementation,
    TransformationImplementation,
    Configuration,
    SourceUnavailable,
    HistoricalUnavailable,
    Qualification,
    CapabilityMismatch,
    Clarification,
    Judgement,
    DiscoveryIncomplete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningGap {
    pub acquisition_id: Option<String>,
    pub requirement_id: Option<String>,
    pub kind: GapKind,
    pub description: String,
    pub implementation: Option<CapabilityImplementationRequest>,
}

/// Bounded topology search and the precise finite family that was compared.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionGraphComparison {
    pub selected: String,
    pub limit: usize,
    pub correspondence_check_limit: usize,
    pub correspondence_checks: usize,
    pub search_complete: bool,
    pub alternatives: Vec<AcquisitionGraphAlternative>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionGraphAlternative {
    pub id: String,
    pub transformations: Vec<String>,
    pub rejection: Option<String>,
    pub total_costs: Vec<(usize, u128)>,
    pub unresolved_acquisitions: usize,
    pub configuration_gaps: usize,
    pub route_search_complete: bool,
}
