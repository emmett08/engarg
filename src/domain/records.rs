//! Serialised extended Toulmin records and their schema.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub claim: String,
    /// Optional information supplied by the user; absent context remains unknown.
    #[serde(default)]
    pub context: String,
    /// Optional supplied evidence, never observations invented from a claim.
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub title: String,
    pub source: String,
    pub excerpt: String,
    pub kind: String,
    /// A user assertion about checking, never an assertion made by this tool.
    pub checked_by_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArgumentCase {
    pub original_claim: String,
    pub current_claim: ClaimVersion,
    pub stronger_target: ClaimVersion,
    pub status: CaseStatus,
    pub scope: EngineeringScope,
    pub subclaims: Vec<ClaimNode>,
    pub grounds: Vec<Ground>,
    pub warrants: Vec<Warrant>,
    pub compositions: Vec<SupportComposition>,
    pub assumptions: Vec<Assumption>,
    pub defeaters: Vec<Defeater>,
    pub alternatives: Vec<Alternative>,
    pub evidence_plan: Vec<EvidenceNeed>,
    pub reading: Vec<NarrativeSection>,
    pub residual_risks: Vec<String>,
    pub decision_limits: Vec<String>,
    pub adjudication: String,
    /// Explicit application of retained objections after a target revision.
    /// Historical cases omit this field and retain the conservative exact policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dissent_revisions: Option<super::DissentRevisions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimVersion {
    pub text: String,
    pub qualifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimNode {
    pub id: String,
    pub text: String,
    pub qualifier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Unsupported,
    Conditional,
    ProvisionallySupported,
    Refuted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EngineeringScope {
    pub system_boundary: String,
    pub configuration: String,
    pub operating_conditions: String,
    pub units: String,
    pub time_horizon: String,
    pub consequences: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub id: String,
    pub statement: String,
    pub evidence_ids: Vec<String>,
    pub status: GroundStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroundStatus {
    Reported,
    Assumed,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Warrant {
    pub id: String,
    pub rule: String,
    pub conclusion_id: String,
    pub premise_ids: Vec<String>,
    pub backing_evidence_ids: Vec<String>,
    pub inference: InferenceKind,
    pub applicability: String,
    /// Typed computational correspondence, or an explicit unresolved method.
    /// Historical cases omit this field and retain authored structured support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_method: Option<super::ReasoningMethodBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SupportComposition {
    pub conclusion_id: String,
    pub relationship: SupportRelationship,
    /// An explicit model judgement about which declared routes are relied upon.
    /// A selected alternative is claimed individually adequate under the common
    /// scope; selection itself supplies no certificate of that adequacy.
    pub selected_warrant_ids: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupportRelationship {
    JointlyRequired,
    AlternativeRoutes,
    Cumulative,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NarrativeSection {
    pub claim_id: String,
    pub title: String,
    pub paragraphs: Vec<ReasoningParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasoningParagraph {
    pub purpose: ParagraphPurpose,
    pub text: String,
    pub reference_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphPurpose {
    Answer,
    Finding,
    Connection,
    Complication,
    Conclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InferenceKind {
    Deductive,
    Inductive,
    Abductive,
    Causal,
    Counterfactual,
    Analogical,
    Practical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assumption {
    pub id: String,
    pub statement: String,
    pub critical: bool,
    pub status: AssumptionStatus,
    pub evidence_ids: Vec<String>,
    pub applies_to: Vec<String>,
    /// A proposed check or an explicitly described existing check.
    pub validation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionStatus {
    Supported,
    Untested,
    Contradicted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Defeater {
    pub id: String,
    pub kind: DefeaterKind,
    pub target_id: String,
    pub challenge: String,
    pub status: DefeaterStatus,
    pub material: bool,
    pub basis: DefeaterBasis,
    pub response: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterKind {
    Undermining,
    Rebutting,
    Undercutting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterStatus {
    Open,
    Sustained,
    Refuted,
    InapplicableAfterRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DefeaterBasis {
    Observed,
    SupportedPossibility,
    Hypothetical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alternative {
    pub explanation: String,
    pub distinguishing_test: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceNeed {
    pub id: String,
    pub target_id: String,
    pub question: String,
    pub method: String,
    pub measurement: String,
    pub comparator: String,
    pub acceptance_criterion: String,
    pub if_failed: String,
    /// An existing claim ID whose support would be strengthened by this check.
    pub enables_claim: String,
    /// 1 is highest priority, 5 is lowest.
    pub priority: u8,
    pub cost: String,
}
