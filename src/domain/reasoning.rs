//! Authored, typed correspondence between a warrant and a bounded EAL method.
//!
//! These declarations are plans, not observations. Only a matching acquired
//! typed-input envelope can run a method, and its result must satisfy the
//! declared predicate. Correspondence with prose remains explicitly authored.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::InferenceKind;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReasoningMethodBinding {
    Bound(Box<BoundReasoningMethod>),
    Unresolved {
        method: ReasoningMethodKind,
        #[schemars(length(min = 1, max = 32))]
        reasons: Vec<String>,
        #[schemars(length(max = 128))]
        evidence_need_ids: Vec<String>,
    },
}

impl ReasoningMethodBinding {
    pub fn bound(&self) -> Option<&BoundReasoningMethod> {
        match self {
            Self::Bound(bound) => Some(bound),
            Self::Unresolved { .. } => None,
        }
    }

    pub fn kind(&self) -> ReasoningMethodKind {
        match self {
            Self::Bound(bound) => bound.method.kind(),
            Self::Unresolved { method, .. } => *method,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundReasoningMethod {
    pub method: MethodSpecification,
    /// One retained acquisition must return an exact EAL/typed-input/1 envelope.
    pub input_evidence_need_id: String,
    /// Every relied-upon warrant premise has an explicit input correspondence.
    #[schemars(length(min = 1, max = 128))]
    pub formal_premises: Vec<FormalPremiseBinding>,
    #[schemars(length(min = 2, max = 128))]
    pub evidence_requirements: Vec<MethodEvidenceRequirement>,
    /// Named applicability assumptions are dependencies, not verified metadata.
    #[schemars(length(max = 128))]
    pub assumption_ids: Vec<String>,
    pub conclusion: MethodConclusionBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningMethodKind {
    Deductive,
    Inductive,
    Abductive,
    Causal,
    Counterfactual,
}

impl ReasoningMethodKind {
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Deductive => "deductive/1",
            Self::Inductive => "inductive/1",
            Self::Abductive => "abductive/1",
            Self::Causal => "causal/1",
            Self::Counterfactual => "counterfactual/1",
        }
    }

    pub const fn inference(self) -> InferenceKind {
        match self {
            Self::Deductive => InferenceKind::Deductive,
            Self::Inductive => InferenceKind::Inductive,
            Self::Abductive => InferenceKind::Abductive,
            Self::Causal => InferenceKind::Causal,
            Self::Counterfactual => InferenceKind::Counterfactual,
        }
    }

    pub const fn evidence_kind(self) -> &'static str {
        match self {
            Self::Deductive => "logical_case",
            Self::Inductive => "sample",
            Self::Abductive => "hypotheses",
            Self::Causal => "experiment",
            Self::Counterfactual => "causal_model",
        }
    }

    pub const fn required_evidence_role(self) -> MethodEvidenceRole {
        match self {
            Self::Deductive => MethodEvidenceRole::PremiseObservation,
            Self::Inductive => MethodEvidenceRole::SamplingDesign,
            Self::Abductive => MethodEvidenceRole::HypothesisModel,
            Self::Causal => MethodEvidenceRole::InterventionDesign,
            Self::Counterfactual => MethodEvidenceRole::StructuralModel,
        }
    }
}

/// The formal query is mode-specific. Measurements arrive through acquisition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MethodSpecification {
    Deductive {
        #[schemars(length(max = 128))]
        premises: Vec<LogicalFormula>,
        conclusion: LogicalFormula,
    },
    Inductive {
        confidence: f64,
    },
    Abductive {
        observed: String,
        #[schemars(length(min = 2, max = 128))]
        candidates: Vec<HypothesisCandidate>,
    },
    Causal {
        assignment: ExperimentalAssignment,
    },
    Counterfactual {
        #[schemars(length(min = 1, max = 32))]
        variables: Vec<StructuralVariable>,
        intervention: StructuralIntervention,
        outcome: String,
    },
}

impl MethodSpecification {
    pub const fn kind(&self) -> ReasoningMethodKind {
        match self {
            Self::Deductive { .. } => ReasoningMethodKind::Deductive,
            Self::Inductive { .. } => ReasoningMethodKind::Inductive,
            Self::Abductive { .. } => ReasoningMethodKind::Abductive,
            Self::Causal { .. } => ReasoningMethodKind::Causal,
            Self::Counterfactual { .. } => ReasoningMethodKind::Counterfactual,
        }
    }

    pub const fn identifier(&self) -> &'static str {
        self.kind().identifier()
    }

    pub const fn evidence_kind(&self) -> &'static str {
        self.kind().evidence_kind()
    }

    /// Exactly the query fields declared by the pinned EAL method contract.
    pub fn query(&self) -> Value {
        if let Self::Counterfactual {
            variables,
            intervention,
            outcome,
        } = self
        {
            let variables: serde_json::Map<_, _> = variables
                .iter()
                .map(|variable| {
                    let coefficients: serde_json::Map<_, _> = variable
                        .coefficients
                        .iter()
                        .map(|coefficient| {
                            (coefficient.variable.clone(), json!(coefficient.coefficient))
                        })
                        .collect();
                    (
                        variable.name.clone(),
                        json!({"intercept":variable.intercept,
                    "coefficients":coefficients,"noise":variable.noise}),
                    )
                })
                .collect();
            return json!({"variables":variables,"intervention":intervention,"outcome":outcome});
        }
        let mut value = serde_json::to_value(self).expect("finite typed method query");
        value.as_object_mut().expect("tagged method").remove("kind");
        value
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum LogicalFormula {
    Atom(#[schemars(length(min = 1, max = 64))] String),
    Not {
        not: Box<LogicalFormula>,
    },
    And {
        #[schemars(length(min = 2, max = 128))]
        and: Vec<LogicalFormula>,
    },
    Or {
        #[schemars(length(min = 2, max = 128))]
        or: Vec<LogicalFormula>,
    },
    Implies {
        #[schemars(length(min = 2, max = 2))]
        implies: Vec<LogicalFormula>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HypothesisCandidate {
    pub name: String,
    pub prior: f64,
    pub likelihood: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum ExperimentalAssignment {
    #[serde(rename = "randomised")]
    Randomised,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StructuralVariable {
    pub name: String,
    pub intercept: f64,
    #[schemars(length(max = 32))]
    pub coefficients: Vec<StructuralCoefficient>,
    pub noise: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StructuralCoefficient {
    pub variable: String,
    pub coefficient: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StructuralIntervention {
    pub variable: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FormalPremiseBinding {
    pub premise_id: String,
    /// JSON pointer within the acquired envelope's payload, not within prose.
    pub input_pointer: String,
    pub evidence_need_id: String,
    pub interpretation: String,
    pub correspondence: FormalInputCorrespondence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FormalInputCorrespondence {
    IdentityValue {
        source_input_index: usize,
        source_pointer: String,
    },
    /// A separately satisfied empirical predicate licenses this formal premise.
    /// This correspondence is available only for deductive premise formulae.
    AcceptedPremise,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodEvidenceRequirement {
    pub evidence_need_id: String,
    pub role: MethodEvidenceRole,
    pub requirement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MethodEvidenceRole {
    FormalInput,
    PremiseObservation,
    SamplingDesign,
    HypothesisModel,
    InterventionDesign,
    StructuralModel,
    Applicability,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodConclusionBinding {
    pub claim_id: String,
    pub subject: String,
    pub quantity: String,
    pub unit: String,
    pub scope: String,
    pub valid_from: String,
    pub valid_until: String,
    pub result: MethodResultPredicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodResultPredicate {
    pub path: String,
    pub operator: MethodComparison,
    pub expected: MethodScalar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MethodComparison {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = ">")]
    Greater,
    #[serde(rename = ">=")]
    GreaterOrEqual,
    #[serde(rename = "<")]
    Less,
    #[serde(rename = "<=")]
    LessOrEqual,
}

impl MethodComparison {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MethodScalar {
    Boolean(bool),
    Number(f64),
}

impl MethodConclusionBinding {
    pub fn proposition(&self, method: &MethodSpecification) -> Value {
        json!({
            "subject": self.subject, "quantity": self.quantity, "unit": self.unit,
            "scope": self.scope, "valid_from": self.valid_from,
            "valid_until": self.valid_until, "query": method.query(),
            "result": {
                "path": self.result.path, "operator": self.result.operator.as_str(),
                "expected": self.result.expected, "expression": null
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_or_extended_formula_objects_are_rejected() {
        for malformed in [
            json!({"not":"p", "and":["q","r"]}),
            json!({"not":"p", "invented":true}),
            json!({"xor":["p","q"]}),
        ] {
            assert!(serde_json::from_value::<LogicalFormula>(malformed).is_err());
        }
    }

    #[test]
    fn provider_schema_uses_closed_objects_nullable_optional_fields_and_supported_unions() {
        fn inspect(schema: &Value) {
            match schema {
                Value::Object(object) => {
                    assert!(!object.contains_key("oneOf"));
                    assert!(!object.contains_key("allOf"));
                    assert!(!object.contains_key("prefixItems"));
                    if object.get("type").and_then(Value::as_str) == Some("object") {
                        assert_eq!(object.get("additionalProperties"), Some(&json!(false)));
                        let required = object.get("required").and_then(Value::as_array).unwrap();
                        let properties =
                            object.get("properties").and_then(Value::as_object).unwrap();
                        assert_eq!(required.len(), properties.len());
                    }
                    for value in object.values() {
                        inspect(value);
                    }
                }
                Value::Array(array) => {
                    for value in array {
                        inspect(value);
                    }
                }
                _ => {}
            }
        }
        let schema = crate::domain::case_schema();
        inspect(&schema);
        let warrant = &schema["$defs"]["Warrant"]["anyOf"][0];
        assert!(
            warrant["required"]
                .as_array()
                .unwrap()
                .contains(&json!("reasoning_method"))
        );
        assert_eq!(
            warrant["properties"]["reasoning_method"]["$ref"],
            "#/$defs/ReasoningMethodBinding"
        );
        assert_eq!(
            schema["$defs"]["Warrant"]["anyOf"][1]["properties"]["reasoning_method"]["type"],
            "null"
        );
    }
}
