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

pub fn case_schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ArgumentCase))
        .expect("a statically derived schema is serialisable");
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
    }
    supported_schema(&mut schema);
    // New generation must make the computational/unresolved choice explicit.
    // Deserialising historical reports continues to admit the absent field.
    let warrant = schema["$defs"]["Warrant"].take();
    let mut computational = warrant.clone();
    computational["properties"]["inference"] = serde_json::json!({"type":"string",
        "enum":["deductive","inductive","abductive","causal","counterfactual"]});
    computational["properties"]["reasoning_method"] =
        serde_json::json!({"$ref":"#/$defs/ReasoningMethodBinding"});
    let mut authored = warrant;
    authored["properties"]["inference"] = serde_json::json!({"type":"string",
        "enum":["analogical","practical"]});
    authored["properties"]["reasoning_method"] = serde_json::json!({"type":"null"});
    schema["$defs"]["Warrant"] = serde_json::json!({"anyOf":[computational,authored]});
    schema
}

/// Schemars emits disjoint tagged unions as oneOf. The provider's strict
/// structured-output dialect uses anyOf, requires every object property and
/// represents optional values through an explicit nullable alternative.
fn supported_schema(value: &mut serde_json::Value) {
    use serde_json::{Value, json};
    match value {
        Value::Object(object) => {
            object.remove("default");
            object.remove("format");
            if let Some(variants) = object.remove("oneOf") {
                object.insert("anyOf".into(), variants);
            }
            if let Some(constant) = object.remove("const") {
                object.insert("enum".into(), json!([constant]));
            }
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                let keys: Vec<_> = properties.keys().cloned().map(Value::String).collect();
                object.insert("required".into(), Value::Array(keys));
                object.insert("additionalProperties".into(), Value::Bool(false));
            }
            for child in object.values_mut() {
                supported_schema(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                supported_schema(child);
            }
        }
        _ => {}
    }
}
