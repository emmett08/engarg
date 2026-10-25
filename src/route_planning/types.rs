use crate::{
    support_graph::AcquisitionRouteProof, tooling::capability::CapabilityImplementationRequest,
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
    pub scheduling: SchedulingPolicy,
    pub bindings: Vec<CapabilityInputBinding>,
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
