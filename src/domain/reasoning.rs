//! Authored, typed correspondence between a warrant and a bounded EAL method.
//!
//! Queries are authored programme data, not observations. EAL owns method
//! schemas; empirical premise values arrive through explicit acquisitions.
//! Correspondence with prose remains explicitly authored.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::InferenceKind;

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

impl<'de> Deserialize<'de> for ReasoningMethodBinding {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let mut value = eal_collector_contracts::deserialize_value_exact(decoder)?;
        let object = value.as_object_mut().ok_or_else(|| {
            serde::de::Error::custom("reasoning method binding must be an object")
        })?;
        let status = object
            .remove("status")
            .ok_or_else(|| serde::de::Error::custom("reasoning method status is required"))?;
        match status.as_str() {
            Some("bound") => serde_json::from_value(value)
                .map(|bound| Self::Bound(Box::new(bound)))
                .map_err(serde::de::Error::custom),
            Some("unresolved") => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Unresolved {
                    method: ReasoningMethodKind,
                    reasons: Vec<String>,
                    evidence_need_ids: Vec<String>,
                }
                let binding: Unresolved =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Unresolved {
                    method: binding.method,
                    reasons: binding.reasons,
                    evidence_need_ids: binding.evidence_need_ids,
                })
            }
            _ => Err(serde::de::Error::custom(
                "unknown reasoning method binding status",
            )),
        }
    }
}

impl ReasoningMethodBinding {
    pub fn bound(&self) -> Option<&BoundReasoningMethod> {
        match self {
            Self::Bound(bound) => Some(bound),
            Self::Unresolved { .. } => None,
        }
    }

    pub fn kind(&self) -> Option<ReasoningMethodKind> {
        match self {
            Self::Bound(bound) => bound.method.kind(),
            Self::Unresolved { method, .. } => Some(*method),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundReasoningMethod {
    /// Omitted historical values keep the empirical source obligations. A
    /// mathematical consequence concerns only the literal authored definitions.
    #[serde(default, skip_serializing_if = "MethodClaimMode::is_empirical")]
    pub claim_mode: MethodClaimMode,
    pub method: MethodSpecification,
    /// Every relied-upon warrant premise has an explicit input correspondence.
    #[schemars(length(max = 128))]
    pub formal_premises: Vec<FormalPremiseBinding>,
    #[schemars(length(max = 128))]
    pub evidence_requirements: Vec<MethodEvidenceRequirement>,
    /// Named applicability assumptions are dependencies, not verified metadata.
    #[schemars(length(max = 128))]
    pub assumption_ids: Vec<String>,
    pub conclusion: MethodConclusionBinding,
}

/// Mathematical validity does not establish an assertion about a physical
/// system. Such an assertion retains the empirical and applicability duties.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MethodClaimMode {
    #[default]
    Empirical,
    MathematicalConsequence,
}

impl MethodClaimMode {
    pub const fn is_empirical(&self) -> bool {
        matches!(self, Self::Empirical)
    }
}

impl BoundReasoningMethod {
    pub const fn is_mathematical(&self) -> bool {
        matches!(self.claim_mode, MethodClaimMode::MathematicalConsequence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningMethodKind {
    Deductive,
    /// Exact consequence of an explicitly supplied finite probability model.
    FiniteProbability,
    Inductive,
    Abductive,
    Causal,
    Counterfactual,
    Analogical,
}

impl ReasoningMethodKind {
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Deductive => "deductive/1",
            Self::FiniteProbability => "finite_probability/1",
            Self::Inductive => "inductive/1",
            Self::Abductive => "abductive/1",
            Self::Causal => "causal/1",
            Self::Counterfactual => "counterfactual/1",
            Self::Analogical => "analogical/1",
        }
    }

    pub const fn inference(self) -> InferenceKind {
        match self {
            Self::Deductive => InferenceKind::Deductive,
            Self::FiniteProbability => InferenceKind::Deductive,
            Self::Inductive => InferenceKind::Inductive,
            Self::Abductive => InferenceKind::Abductive,
            Self::Causal => InferenceKind::Causal,
            Self::Counterfactual => InferenceKind::Counterfactual,
            Self::Analogical => InferenceKind::Analogical,
        }
    }

    pub const fn required_evidence_role(self) -> MethodEvidenceRole {
        match self {
            Self::Deductive => MethodEvidenceRole::PremiseObservation,
            Self::FiniteProbability => MethodEvidenceRole::PremiseObservation,
            Self::Inductive => MethodEvidenceRole::SamplingDesign,
            Self::Abductive => MethodEvidenceRole::HypothesisModel,
            Self::Causal => MethodEvidenceRole::InterventionDesign,
            Self::Counterfactual => MethodEvidenceRole::StructuralModel,
            Self::Analogical => MethodEvidenceRole::Applicability,
        }
    }
}

/// EAL owns the exact query shape and method version. This record adds no
/// parallel formula AST or numerical contract in EngArg.
#[derive(Debug, Clone, Serialize)]
pub struct MethodSpecification {
    pub identifier: String,
    pub query: Value,
}

impl MethodSpecification {
    pub fn kind(&self) -> Option<ReasoningMethodKind> {
        match self.identifier.as_str() {
            "deductive/1" => Some(ReasoningMethodKind::Deductive),
            "finite_probability/1" => Some(ReasoningMethodKind::FiniteProbability),
            "inductive/1" => Some(ReasoningMethodKind::Inductive),
            "abductive/1" => Some(ReasoningMethodKind::Abductive),
            "causal/1" => Some(ReasoningMethodKind::Causal),
            "counterfactual/1" => Some(ReasoningMethodKind::Counterfactual),
            "analogical/1" => Some(ReasoningMethodKind::Analogical),
            _ => None,
        }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn evidence_kind(&self) -> Option<&str> {
        self.contract()
            .and_then(|contract| contract.evidence_kind.as_deref())
    }

    pub fn query(&self) -> Value {
        self.query.clone()
    }

    pub fn contract(&self) -> Option<&'static eal_core::methods::MethodContract> {
        eal_core::methods::default_registry().get(&self.identifier)
    }
}

/// Text-model output carries exact JSON text because a provider's strict
/// object schema cannot express every EAL method's open mathematical values.
/// This transport form is decoded once; persisted canonical cases retain Value.
impl JsonSchema for MethodSpecification {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MethodSpecification".into()
    }
    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::try_from(method_specification_schema()).expect("method authoring schema")
    }
}

impl<'de> Deserialize<'de> for MethodSpecification {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = eal_collector_contracts::deserialize_value_exact(decoder)?;
        let object = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("method must be an object"))?;
        let identifier = object
            .get("identifier")
            .and_then(Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("method identifier must be a string"))?
            .to_owned();
        if object.len() != 2 {
            return Err(serde::de::Error::custom(
                "method requires identifier and exactly one of query or query_json",
            ));
        }
        let query = if let Some(query) = object.get("query") {
            query.clone()
        } else if let Some(text) = object.get("query_json").and_then(Value::as_str) {
            eal_collector_contracts::strict_json_exact(
                text.as_bytes(),
                eal_collector_contracts::JsonLimits {
                    max_bytes: 1_048_576,
                    ..Default::default()
                },
            )
            .map_err(serde::de::Error::custom)?
        } else {
            return Err(serde::de::Error::custom(
                "method requires query or query_json",
            ));
        };
        Ok(Self { identifier, query })
    }
}

/// Authoring chooses among descriptors exposed by the pinned EAL implementation.
/// Query and output contracts travel in the prompt, not a second formula schema.
pub(crate) fn method_specification_schema() -> Value {
    let descriptors = eal_core::methods::default_registry().describe();
    let identifiers: Vec<_> = descriptors
        .as_object()
        .expect("method descriptors")
        .keys()
        .filter(|identifier| {
            MethodSpecification {
                identifier: (*identifier).clone(),
                query: Value::Null,
            }
            .kind()
            .is_some()
        })
        .collect();
    json!({
        "type":"object", "additionalProperties":false,
        "properties":{
            "identifier":{"type":"string","enum":identifiers},
            "query_json":{"type":"string","minLength":2,"maxLength":1048576}
        }, "required":["identifier","query_json"]
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FormalPremiseBinding {
    pub premise_id: String,
    /// JSON pointer within the method payload, not within prose.
    pub input_pointer: String,
    pub evidence_need_id: String,
    pub interpretation: String,
    pub correspondence: FormalInputCorrespondence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FormalInputCorrespondence {
    /// A stipulated definition in the immutable authored query. This does not
    /// assert that a physical system obeys the model; that needs separate support.
    AuthoredDefinition,
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
    pub scope: MethodScope,
    pub valid_from: String,
    pub valid_until: String,
    pub result: MethodResultPredicate,
}

/// The method's target identity is checked against each acquired premise scope.
/// Extra source selectors, such as an actual sample identifier, do not change
/// the target population. Transport to a different identity remains unresolved.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodScope {
    #[schemars(length(min = 1, max = 64))]
    #[schemars(regex(pattern = "^[A-Za-z_][A-Za-z0-9_.-]*$"))]
    pub kind: String,
    #[schemars(length(min = 1, max = 32))]
    pub selectors: Vec<MethodScopeSelector>,
    #[schemars(length(min = 1, max = 32))]
    pub identity_dimensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MethodScopeSelector {
    #[schemars(length(min = 1, max = 128))]
    pub name: String,
    pub value: MethodScopeScalar,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MethodScopeScalar {
    String(#[schemars(length(min = 1, max = 4096))] String),
    Boolean(bool),
    Number(f64),
}

impl MethodScope {
    pub fn validate(&self) -> Result<(), String> {
        let text = |value: &str, maximum| {
            !value.trim().is_empty()
                && value.len() <= maximum
                && !value.chars().any(char::is_control)
        };
        if !text(&self.kind, 64)
            || !eal_collector_contracts::identifier(&self.kind)
            || !(1..=32).contains(&self.selectors.len())
            || !(1..=32).contains(&self.identity_dimensions.len())
        {
            return Err(
                "method scope requires a bounded kind and 1–32 selectors and identity dimensions"
                    .into(),
            );
        }
        let mut selectors = BTreeSet::new();
        for selector in &self.selectors {
            if !text(&selector.name, 128) || !selectors.insert(selector.name.as_str()) {
                return Err(
                    "method scope selector names must be unique bounded nonblank text".into(),
                );
            }
            let valid = match &selector.value {
                MethodScopeScalar::String(value) => text(value, 4096),
                MethodScopeScalar::Boolean(_) => true,
                MethodScopeScalar::Number(value) => value.is_finite(),
            };
            if !valid {
                return Err("method scope selectors require nonblank bounded strings, booleans or finite numbers".into());
            }
        }
        let mut dimensions = BTreeSet::new();
        if self.identity_dimensions.iter().any(|dimension| {
            !text(dimension, 128)
                || !dimensions.insert(dimension.as_str())
                || !selectors.contains(dimension.as_str())
        }) {
            return Err(
                "method scope identity dimensions must be unique and name declared selectors"
                    .into(),
            );
        }
        Ok(())
    }

    /// Stable across selector and identity-dimension ordering. The EAL field is
    /// an identity of this record, never a free-text scope interpretation.
    pub fn identity(&self) -> Result<String, String> {
        self.validate()?;
        let selectors: BTreeMap<_, _> = self
            .selectors
            .iter()
            .map(|selector| (selector.name.as_str(), &selector.value))
            .collect();
        let dimensions: BTreeSet<_> = self.identity_dimensions.iter().collect();
        eal_collector_contracts::capability::canonical_identity(&json!({
            "schema":"engarg.method-scope/1", "kind":self.kind,
            "selectors":selectors, "identity_dimensions":dimensions
        }))
        .map(|digest| format!("engarg.scope:{digest}"))
    }
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
    pub fn proposition(&self, method: &MethodSpecification) -> Result<Value, String> {
        Ok(json!({
            "subject": self.subject, "quantity": self.quantity, "unit": self.unit,
            "scope": self.scope.identity()?, "valid_from": self.valid_from,
            "valid_until": self.valid_until, "query": method.query(),
            "result": {
                "path": self.result.path, "operator": self.result.operator.as_str(),
                "expected": self.result.expected, "expression": null
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_validation_uses_the_eal_descriptor_without_a_parallel_formula_ast() {
        let method = MethodSpecification {
            identifier: "deductive/1".into(),
            query: json!({"premises":["p"],"conclusion":{"xor":["p","q"]}}),
        };
        assert!(
            !eal_core::methods::validate_query(
                method.identifier(),
                &method.query,
                eal_core::methods::default_registry()
            )
            .is_empty()
        );
        assert!(
            MethodSpecification {
                identifier: "invented/1".into(),
                query: json!({})
            }
            .contract()
            .is_none()
        );
    }

    #[test]
    fn analogical_authoring_uses_the_installed_eal_contract() {
        let method = MethodSpecification {
            identifier: "analogical/1".into(),
            query: json!({"relevant_features":["interface"],"source":{"interface":"v1"},"target":{"interface":"v1"}}),
        };
        assert_eq!(method.kind(), Some(ReasoningMethodKind::Analogical));
        assert_eq!(method.evidence_kind(), Some("analogy"));
        assert!(
            eal_core::methods::validate_query(
                method.identifier(),
                &method.query,
                eal_core::methods::default_registry()
            )
            .is_empty()
        );
        assert!(
            method_specification_schema()["properties"]["identifier"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!("analogical/1"))
        );
    }

    #[test]
    fn authoring_query_text_is_decoded_once_without_numeric_loss() {
        let authored = json!({"identifier":"deductive/1",
            "query_json":r#"{"premises":[],"conclusion":9007199254740993123456789}"#});
        let method: MethodSpecification = serde_json::from_value(authored).unwrap();
        assert_eq!(
            method.query["conclusion"].to_string(),
            "9007199254740993123456789"
        );
        let canonical = serde_json::to_value(method).unwrap();
        assert!(canonical.get("query_json").is_none());
        assert!(canonical["query"].is_object());
        assert_eq!(
            serde_json::from_value::<MethodSpecification>(canonical)
                .unwrap()
                .query["conclusion"]
                .to_string(),
            "9007199254740993123456789"
        );
        for malformed in [
            json!({"identifier":"deductive/1","query_json":r#"{"premises":[],"premises":["p"],"conclusion":"p"}"#}),
            json!({"identifier":"deductive/1","query_json":"{} {}"}),
            json!({"identifier":"deductive/1","query":{},"query_json":"{}"}),
        ] {
            assert!(serde_json::from_value::<MethodSpecification>(malformed).is_err());
        }
    }

    #[test]
    fn method_scope_identity_is_canonical_and_rejects_ambiguous_identities() {
        let mut scope: MethodScope = serde_json::from_value(json!({
            "kind":"population", "selectors":[
                {"name":"population","value":"production"},
                {"name":"enabled","value":true},
                {"name":"revision","value":1.0}
            ], "identity_dimensions":["population","revision"]
        }))
        .unwrap();
        let identity = scope.identity().unwrap();
        scope.selectors.reverse();
        scope.identity_dimensions.reverse();
        assert_eq!(scope.identity().unwrap(), identity);
        assert!(identity.starts_with("engarg.scope:"));

        let mut duplicate = scope.clone();
        duplicate.selectors.push(duplicate.selectors[0].clone());
        assert!(duplicate.identity().is_err());
        let mut duplicate = scope.clone();
        duplicate.identity_dimensions.push("population".into());
        assert!(duplicate.identity().is_err());
        let mut missing = scope.clone();
        missing
            .identity_dimensions
            .push("unobserved_environment".into());
        assert!(missing.identity().is_err());
        let mut invalid = scope;
        invalid.selectors[0].value = MethodScopeScalar::Number(f64::NAN);
        assert!(invalid.identity().is_err());
        assert!(serde_json::from_value::<MethodScope>(json!("production")).is_err());
    }

    #[test]
    fn catalogue_scope_identity_matches_the_independent_python_host_policy() {
        let scope: MethodScope = serde_json::from_value(json!({
            "kind":"catalogue_snapshot",
            "selectors":[{"name":"snapshot","value":"demonstration-v1"}],
            "identity_dimensions":["snapshot"]
        }))
        .unwrap();
        assert_eq!(
            scope.identity().unwrap(),
            "engarg.scope:e4573ab176d50fdfebebee9913448452898c9ad953abd8b22ae3ba2e9efb02a8"
        );
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
