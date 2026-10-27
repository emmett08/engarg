//! Versioned obligations retain canonical argument identifiers without choosing providers.
use crate::domain::ArgumentCase;
use eal_collector_contracts::{
    capability::SupportContract,
    execution::{
        DependencyBinding, DerivationStep, EvaluationCriterion, ForEachBinding,
        RequirementEvaluation,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportElaboration {
    pub schema: String,
    pub requirements: Vec<SupportRequirement>,
    pub acquisitions: Vec<AcquisitionBlueprint>,
    pub derivations: Vec<DerivationStep>,
    pub evaluations: Vec<RequirementEvaluation>,
    pub revisions: Vec<RequirementRevision>,
    /// Dispositions for historical source designs, anchored to exact prior step identities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acquisition_revisions: Vec<AcquisitionRevision>,
    /// Explicit discriminating obligations for the case's ordered alternatives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_support: Vec<AlternativeSupport>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlternativeSupport {
    pub alternative_id: String,
    pub distinguishing_test: String,
    pub claim_ids: Vec<String>,
    pub requirement_ids: Vec<String>,
}

/// A positive observation has a declared argumentative use. Challenge activity
/// and its resolution are distinct obligations even when they share a target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupportRequirementRole {
    #[default]
    Support,
    ChallengeActivation,
    ChallengeResolution,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportRequirement {
    pub id: String,
    #[serde(default)]
    pub role: SupportRequirementRole,
    pub target_id: String,
    pub evidence_need_ids: Vec<String>,
    /// Exact clauses from linked EvidenceNeed records, ordered by evidence need ID.
    pub original_acceptance_criteria: Vec<String>,
    pub contract: Option<SupportContract>,
    pub discriminating_evidence: Vec<String>,
    pub acceptance: EvaluationCriterion,
    pub acceptance_rationale: String,
    pub gaps: Vec<RequirementGap>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionBlueprint {
    pub id: String,
    pub contract: SupportContract,
    /// Logical input slots. Only operator-admitted bindings select execution authority.
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub arguments: Value,
    pub bindings: Vec<DependencyBinding>,
    pub for_each: Option<ForEachBinding>,
    /// A reusable operation definition, required only when an implementation is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_spec: Option<OperationSpecification>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationSpecification {
    pub schema: String,
    pub definition: String,
    pub input_conditions: Vec<String>,
    pub output_guarantees: Vec<String>,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub input_schema: Value,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub output_schema: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementGapKind {
    Clarification,
    SourceAvailabilityUnverified,
    SourceUnavailable,
    HistoricalUnavailable,
    Judgement,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequirementGap {
    pub kind: RequirementGapKind,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequirementRevision {
    pub requirement_id: String,
    pub previous_requirement_sha256: String,
    pub reason: String,
}

/// The argument-producing wire envelope is closed; arbitrary selector JSON stays encoded.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportedArgument {
    pub argument: ArgumentCase,
    pub support_elaboration: SupportElaboration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionDisposition {
    Repaired,
    Replaced,
    Deferred,
    Retired,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRevision {
    pub previous_step_id: String,
    pub previous_step_sha256: String,
    pub disposition: AcquisitionDisposition,
    pub replacement_ids: Vec<String>,
    pub reason: String,
}
