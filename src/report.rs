//! Typed report extension admission shared by export, replanning and tooling.
use crate::{
    Error,
    engine::SupportPlanning,
    route_planning::{ROUTE_PLAN_SCHEMA, RoutePlan},
    support_requirements::supported_schema,
};
use eal_collector_contracts::execution::canonical_digest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub const REPLANNING_SCHEMA: &str = "engarg.evidence-replan/1";
pub const MAX_REPLANNING_HISTORY: usize = 16;

#[derive(Debug, Clone)]
pub enum EvidencePlanningReport {
    Flat,
    Graph(Box<SupportPlanning<RoutePlan>>),
}

/// A null optional extension has the same meaning as an omitted extension.
pub fn classify_evidence_planning(raw: &Value) -> Result<EvidencePlanningReport, Error> {
    if !raw.is_object() {
        return Err(invalid("saved analysis must be a JSON object"));
    }
    let Some(value) = raw
        .get("evidence_planning")
        .filter(|value| !value.is_null())
    else {
        return Ok(EvidencePlanningReport::Flat);
    };
    let planning: SupportPlanning<RoutePlan> = serde_json::from_value(value.clone())?;
    if planning.schema != "engarg.support-planning/1"
        || !supported_schema(&planning.elaboration.schema)
        || planning.routes.schema != ROUTE_PLAN_SCHEMA
    {
        return Err(invalid("unsupported saved evidence planning schema"));
    }
    Ok(EvidencePlanningReport::Graph(Box::new(planning)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplanningRecord {
    pub schema: String,
    pub input_report_sha256: String,
    pub previous_planning_sha256: String,
    pub current_planning_sha256: String,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub previous_evidence_planning: Value,
    /// Preserve presence, nulls and exact values of the previous extensions.
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_map_exact")]
    pub previous_extensions: BTreeMap<String, Value>,
    pub model_calls: u32,
}

/// Retain the original generation metadata without rebinding it to new routes.
pub fn retain_replanning(previous: &Value, current: &mut Value) -> Result<(), Error> {
    let old = previous
        .get("evidence_planning")
        .filter(|value| !value.is_null())
        .ok_or_else(|| invalid("replanning requires a graph report"))?;
    let new = current
        .get("evidence_planning")
        .filter(|value| !value.is_null())
        .ok_or_else(|| invalid("replanning requires a graph report"))?;
    let record = ReplanningRecord {
        schema: REPLANNING_SCHEMA.into(),
        input_report_sha256: digest(previous)?,
        previous_planning_sha256: digest(old)?,
        current_planning_sha256: digest(new)?,
        previous_evidence_planning: old.clone(),
        previous_extensions: ["tooling", "replanning"]
            .into_iter()
            .filter_map(|key| previous.get(key).map(|value| (key.into(), value.clone())))
            .collect(),
        model_calls: 0,
    };
    let object = current
        .as_object_mut()
        .ok_or_else(|| invalid("saved analysis must be a JSON object"))?;
    object.remove("tooling");
    object.insert("replanning".into(), serde_json::to_value(record)?);
    Ok(())
}

/// Verify a bounded chain and revalidate every archived package against its
/// original planning metadata; no new model-call accounting is invented.
pub fn validate_replanning(raw: &Value, executable: &Path) -> Result<(), Error> {
    validate_replanning_limit(raw, executable, MAX_REPLANNING_HISTORY)
}

/// Reserve one retained-history slot before discovery can issue external reads.
/// Existing maximum-depth reports remain valid for export and assessment.
pub fn validate_replanning_for_update(raw: &Value, executable: &Path) -> Result<(), Error> {
    validate_replanning_limit(raw, executable, MAX_REPLANNING_HISTORY - 1)
}

fn validate_replanning_limit(raw: &Value, executable: &Path, maximum: usize) -> Result<(), Error> {
    let mut current = raw.clone();
    for depth in 0..=maximum {
        let Some(value) = current.get("replanning").filter(|value| !value.is_null()) else {
            return Ok(());
        };
        if depth == maximum {
            return Err(invalid(if maximum == MAX_REPLANNING_HISTORY {
                "replanning history exceeds 16 revisions"
            } else {
                "replanning history already contains 16 revisions; no additional transition can be retained"
            }));
        }
        let record: ReplanningRecord = serde_json::from_value(value.clone())?;
        if record.schema != REPLANNING_SCHEMA
            || record.model_calls != 0
            || record
                .previous_extensions
                .keys()
                .any(|key| !matches!(key.as_str(), "tooling" | "replanning"))
        {
            return Err(invalid(
                "invalid replanning provenance schema or extension fields",
            ));
        }
        let EvidencePlanningReport::Graph(now) = classify_evidence_planning(&current)? else {
            return Err(invalid("replanning provenance requires graph planning"));
        };
        let prior_shell = json!({"evidence_planning":record.previous_evidence_planning});
        let EvidencePlanningReport::Graph(prior) = classify_evidence_planning(&prior_shell)? else {
            return Err(invalid(
                "previous replanning provenance requires graph planning",
            ));
        };
        if digest(&current["evidence_planning"])? != record.current_planning_sha256
            || digest(&record.previous_evidence_planning)? != record.previous_planning_sha256
            || digest(&now.elaboration)? != digest(&prior.elaboration)?
        {
            return Err(invalid(
                "replanning changed its original requirements or planning identities",
            ));
        }
        let object = current.as_object_mut().expect("classified report object");
        object.insert(
            "evidence_planning".into(),
            record.previous_evidence_planning,
        );
        object.remove("tooling");
        object.remove("replanning");
        object.extend(record.previous_extensions);
        if digest(&current)? != record.input_report_sha256 {
            return Err(invalid("replanning changed the retained original report"));
        }
        crate::tooling::capability::revalidate_report(&current, executable)?;
    }
    unreachable!("bounded replanning loop returns at its limit")
}

fn digest(value: &impl Serialize) -> Result<String, Error> {
    canonical_digest(value).map_err(Error::InvalidInput)
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_and_null_extensions_are_flat_and_malformed_graphs_are_rejected() {
        for value in [json!({}), json!({"evidence_planning":null})] {
            assert!(matches!(
                classify_evidence_planning(&value).unwrap(),
                EvidencePlanningReport::Flat
            ));
        }
        for value in [
            json!(null),
            json!({"evidence_planning":false}),
            json!({"evidence_planning":{"schema":"engarg.support-planning/999"}}),
        ] {
            assert!(classify_evidence_planning(&value).is_err());
        }
    }
}
