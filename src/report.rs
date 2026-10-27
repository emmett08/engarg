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

mod authorship;
mod current;
mod encoding;
pub use encoding::FieldEncoding as RetainedFieldEncoding;
pub mod freshness;
pub use current::{
    AuthoringRecovery, RUN_REPORT_SCHEMA, SupportRecovery, read, read_direct, validate, with_direct,
};

pub const REPLANNING_SCHEMA: &str = "engarg.evidence-replan/1";
pub const MAX_REPLANNING_HISTORY: usize = 16;
pub(crate) const MAX_SUPPORT_RECOVERY_HISTORY: usize = 3;

#[derive(Debug, Clone)]
pub enum EvidencePlanningReport {
    Unplanned,
    Direct(Box<crate::engine::RunReport>),
    Graph(Box<SupportPlanning<RoutePlan>>),
}

/// A null optional extension has the same meaning as an omitted extension.
pub fn classify_evidence_planning(raw: &Value) -> Result<EvidencePlanningReport, Error> {
    crate::authoring_checkpoint::validate_recovery_report(raw)?;
    if !raw.is_object() {
        return Err(invalid("saved analysis must be a JSON object"));
    }
    let Some(value) = raw
        .get("evidence_planning")
        .filter(|value| !value.is_null())
    else {
        return if raw
            .get("support_elaboration")
            .is_some_and(|value| !value.is_null())
        {
            Ok(EvidencePlanningReport::Direct(Box::new(read_direct(raw)?)))
        } else {
            Ok(EvidencePlanningReport::Unplanned)
        };
    };
    let planning: SupportPlanning<RoutePlan> = serde_json::from_value(value.clone())?;
    if planning.schema != "engarg.support-planning/1"
        || !supported_schema(&planning.elaboration.schema)
        || planning.routes.schema != ROUTE_PLAN_SCHEMA
    {
        return Err(invalid("unsupported saved evidence planning schema"));
    }
    if let Some(review) = &planning.design_review {
        let transcript: Vec<crate::engine::CallRecord> =
            serde_json::from_value(raw.get("transcript").cloned().ok_or_else(|| {
                invalid("acquisition design review requires its retained transcript")
            })?)?;
        let expected = crate::support_requirements::review_acquisition_design(
            &planning.elaboration,
            &transcript,
            16 * 1_048_576,
        )?;
        if digest(review)? != digest(&expected)? {
            return Err(invalid(
                "saved acquisition design review differs from retained authoring history",
            ));
        }
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
    /// Sparse raw encodings lost when a consumer normalises typed defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub previous_encoding: BTreeMap<String, RetainedFieldEncoding>,
    pub model_calls: u32,
    /// Explicit operator-authorised age additions. Older route-only records omit this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness_clarification: Option<freshness::FreshnessClarification>,
}

/// Retain the original generation metadata without rebinding it to new routes.
pub fn retain_replanning(previous: &Value, current: &mut Value) -> Result<(), Error> {
    retain_replanning_with_freshness(previous, current, None)
}

pub fn retain_replanning_with_freshness(
    previous: &Value,
    current: &mut Value,
    clarification: Option<freshness::FreshnessClarification>,
) -> Result<(), Error> {
    let old = previous
        .get("evidence_planning")
        .filter(|value| !value.is_null())
        .ok_or_else(|| invalid("replanning requires a graph report"))?;
    let new = current
        .get("evidence_planning")
        .filter(|value| !value.is_null())
        .ok_or_else(|| invalid("replanning requires a graph report"))?;
    // A new route plan has one current typed encoding. The parent keeps its
    // exact original encoding and digest through the sparse restoration below.
    let canonical: SupportPlanning<RoutePlan> = serde_json::from_value(new.clone())?;
    let canonical = serde_json::to_value(canonical)?;
    let previous_extensions: BTreeMap<String, Value> = [
        "tooling",
        "replanning",
        "support_recovery",
        "support_elaboration",
        "direct_bindings",
        "design_review",
    ]
    .into_iter()
    .filter(|key| *key != "support_recovery" || clarification.is_some())
    .filter_map(|key| previous.get(key).map(|value| (key.into(), value.clone())))
    .collect();
    let excluded_roots: Vec<_> = previous_extensions
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("evidence_planning"))
        .collect();
    let previous_encoding = encoding::capture(previous, &excluded_roots)?;
    let record = ReplanningRecord {
        schema: REPLANNING_SCHEMA.into(),
        input_report_sha256: digest(previous)?,
        previous_planning_sha256: digest(old)?,
        current_planning_sha256: digest(&canonical)?,
        previous_evidence_planning: old.clone(),
        previous_extensions,
        previous_encoding,
        model_calls: 0,
        freshness_clarification: clarification,
    };
    let encoded_record = serde_json::to_value(&record)?;
    let object = current
        .as_object_mut()
        .ok_or_else(|| invalid("saved analysis must be a JSON object"))?;
    object.insert("evidence_planning".into(), canonical);
    object.remove("tooling");
    if record.freshness_clarification.is_some() {
        object.remove("support_recovery");
    }
    object.insert("replanning".into(), encoded_record);
    Ok(())
}

/// Verify a bounded chain and revalidate every archived package against its
/// original planning metadata; no new model-call accounting is invented.
pub fn validate_replanning(raw: &Value, executable: &Path) -> Result<(), Error> {
    validate_support_recovery(raw)?;
    validate_replanning_limit(raw, Some(executable), MAX_REPLANNING_HISTORY)
}

/// Check imported provenance without reading or executing referenced tool files.
/// Tool-package qualification remains a separate admission at its use boundary.
fn validate_history(raw: &Value) -> Result<(), Error> {
    validate_support_recovery(raw)?;
    validate_replanning_limit(raw, None, MAX_REPLANNING_HISTORY)
}

/// Reserve one retained-history slot before discovery can issue external reads.
/// Existing maximum-depth reports remain valid for export and assessment.
pub fn validate_replanning_for_update(raw: &Value, executable: &Path) -> Result<(), Error> {
    validate_support_recovery(raw)?;
    validate_replanning_limit(raw, Some(executable), MAX_REPLANNING_HISTORY - 1)
}

/// Recovery retains complete prior reports, with a small independent depth bound.
pub fn support_recovery_depth(raw: &Value) -> Result<usize, Error> {
    let mut current = raw;
    for depth in 0..=MAX_SUPPORT_RECOVERY_HISTORY {
        let Some(record) = current
            .get("support_recovery")
            .filter(|record| !record.is_null())
        else {
            return Ok(depth);
        };
        if depth == MAX_SUPPORT_RECOVERY_HISTORY {
            return Err(invalid("support recovery history exceeds three revisions"));
        }
        current = record
            .get("previous_report")
            .ok_or_else(|| invalid("support recovery omits previous report"))?;
    }
    unreachable!()
}

/// Validate recovery separately from route-only replanning. Support may change,
/// while the argument, request and retained authoring prefix stay identical.
pub fn validate_support_recovery(raw: &Value) -> Result<(), Error> {
    support_recovery_depth(raw)?;
    let mut current = raw;
    while let Some(record) = current
        .get("support_recovery")
        .filter(|record| !record.is_null())
    {
        let previous = record
            .get("previous_report")
            .ok_or_else(|| invalid("support recovery omits previous report"))?;
        if record["schema"] != "engarg.support-recovery/1"
            || record["input_report_sha256"].as_str() != Some(digest(previous)?.as_str())
            || current["argument"] != previous["argument"]
            || current["request"] != previous["request"]
            || current["input_sha256"] != previous["input_sha256"]
            || current["run_config"] != previous["run_config"]
            || current["provider_config"] != previous["provider_config"]
        {
            return Err(invalid(
                "support recovery changed its retained source identity or frozen argument",
            ));
        }
        let before = previous["transcript"]
            .as_array()
            .ok_or_else(|| invalid("support recovery requires original transcript"))?;
        let after = current["transcript"]
            .as_array()
            .ok_or_else(|| invalid("support recovery requires transcript"))?;
        let calls = record["model_calls"]
            .as_u64()
            .ok_or_else(|| invalid("support recovery needs model-call accounting"))?;
        let limit = record["repair_limit"]
            .as_u64()
            .ok_or_else(|| invalid("support recovery needs repair allowance"))?;
        if !(1..=3).contains(&limit)
            || calls == 0
            || calls > limit
            || record["recorded_calls"].as_u64() != Some(before.len() as u64)
            || after.len() != before.len() + calls as usize
            || after.get(..before.len()) != Some(before.as_slice())
        {
            return Err(invalid(
                "support recovery changed recorded calls or exceeded its separate allowance",
            ));
        }
        let added: Vec<crate::engine::CallRecord> = after[before.len()..]
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()?;
        if record["provider_requests"]
            != serde_json::to_value(added.iter().map(|call| &call.prompt).collect::<Vec<_>>())?
        {
            return Err(invalid(
                "support recovery provider requests differ from retained completions",
            ));
        }
        for call in &added {
            if call.role != crate::provider::Role::Adjudicator
                || call.prompt.role != call.role
                || call.response_content_omitted
                || call.response_content_bytes != call.response.content.len()
                || call.response_content_sha256
                    != crate::engine::sha256_hex(call.response.content.as_bytes())
            {
                return Err(invalid(
                    "support recovery contains an invalid completion identity",
                ));
            }
        }
        let argument: crate::domain::ArgumentCase =
            serde_json::from_value(current["argument"].clone())?;
        let EvidencePlanningReport::Graph(now) = classify_evidence_planning(current)? else {
            return Err(invalid("support recovery requires graph planning"));
        };
        let EvidencePlanningReport::Graph(prior) = classify_evidence_planning(previous)? else {
            return Err(invalid("support recovery requires previous graph planning"));
        };
        let errors = crate::support_requirements::validate_revision(
            &argument,
            &prior.elaboration,
            &argument,
            &now.elaboration,
        );
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
        let final_call = added.last().expect("positive call count");
        let mut parsed = crate::support_requirements::parse_supported_argument(
            &final_call.response.content,
            final_call.response_content_bytes,
        )?;
        crate::output_validation::restore_host_fields(&mut parsed.argument, &argument)?;
        if serde_json::to_value(parsed.argument)? != current["argument"]
            || digest(&parsed.support_elaboration)? != digest(&now.elaboration)?
        {
            return Err(invalid(
                "support recovery result differs from its retained final completion",
            ));
        }
        current = previous;
    }
    Ok(())
}

fn validate_replanning_limit(
    raw: &Value,
    executable: Option<&Path>,
    maximum: usize,
) -> Result<(), Error> {
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
            || record.previous_extensions.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "tooling"
                        | "replanning"
                        | "support_recovery"
                        | "support_elaboration"
                        | "direct_bindings"
                        | "design_review"
                )
            })
        {
            return Err(invalid(
                "invalid replanning provenance schema or extension fields",
            ));
        }
        let EvidencePlanningReport::Graph(now) = classify_evidence_planning(&current)? else {
            return Err(invalid("replanning provenance requires graph planning"));
        };
        let prior_shell = json!({"evidence_planning":record.previous_evidence_planning,"transcript":current.get("transcript").cloned().unwrap_or_else(||json!([]))});
        let EvidencePlanningReport::Graph(prior) = classify_evidence_planning(&prior_shell)? else {
            return Err(invalid(
                "previous replanning provenance requires graph planning",
            ));
        };
        let mut expected_elaboration = prior.elaboration.clone();
        if let Some(clarification) = &record.freshness_clarification {
            freshness::apply(&mut expected_elaboration, clarification)?;
        }
        if digest(&current["evidence_planning"])? != record.current_planning_sha256
            || digest(&record.previous_evidence_planning)? != record.previous_planning_sha256
            || digest(&now.elaboration)? != digest(&expected_elaboration)?
        {
            return Err(invalid(
                "replanning changed its original requirements or planning identities",
            ));
        }
        restore_replanning_parent(&mut current, record)?;
        if let Some(executable) = executable {
            crate::tooling::direct::revalidate_report(&current, executable)?;
        }
        validate_support_recovery(&current)?;
    }
    unreachable!("bounded replanning loop returns at its limit")
}

/// Restore the exact parent bytes represented by a checked route-only transition.
/// Nullable extension presence is part of the retained identity.
fn restore_replanning_parent(current: &mut Value, record: ReplanningRecord) -> Result<(), Error> {
    let object = current
        .as_object_mut()
        .ok_or_else(|| invalid("replanning requires a report object"))?;
    object.insert(
        "evidence_planning".into(),
        record.previous_evidence_planning,
    );
    for key in [
        "tooling",
        "replanning",
        "support_elaboration",
        "direct_bindings",
        "design_review",
    ] {
        object.remove(key);
    }
    object.extend(record.previous_extensions);
    encoding::restore(current, &record.previous_encoding)?;
    if digest(current)? != record.input_report_sha256 {
        return Err(invalid("replanning changed the retained original report"));
    }
    Ok(())
}

/// Select the original authoring cut after its recovery/replanning history has
/// passed validate_history. Repairs own their later support changes; initial
/// authored output is checked against the retained source before those changes.
pub(super) fn authoring_cut(
    report: &crate::engine::RunReport,
) -> Result<crate::engine::RunReport, Error> {
    let mut current = serde_json::to_value(report)?;
    let maximum = (MAX_REPLANNING_HISTORY + 1) * (MAX_SUPPORT_RECOVERY_HISTORY + 1);
    for _ in 0..=maximum {
        if let Some(record) = current.get("replanning").filter(|value| !value.is_null()) {
            let record: ReplanningRecord = serde_json::from_value(record.clone())?;
            restore_replanning_parent(&mut current, record)?;
        } else if let Some(record) = current
            .get("support_recovery")
            .filter(|value| !value.is_null())
        {
            current = record
                .get("previous_report")
                .cloned()
                .ok_or_else(|| invalid("support recovery omits previous report"))?;
        } else {
            return serde_json::from_value(current).map_err(Error::from);
        }
    }
    Err(invalid(
        "authoring origin exceeds its retained history bounds",
    ))
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
    fn absent_and_null_extensions_are_unplanned_and_malformed_graphs_are_rejected() {
        for value in [json!({}), json!({"evidence_planning":null})] {
            assert!(matches!(
                classify_evidence_planning(&value).unwrap(),
                EvidencePlanningReport::Unplanned
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
