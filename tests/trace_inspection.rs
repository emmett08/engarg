//! Full-span candidate inspection never substitutes for independent provenance.
use engarg::trace_inspection::inspect_trace;
use serde_json::{Value, json};

const KIND: &str = "Application";
const NAME: &str = "catalog-sync-operator";

fn attribute(key: &str, value: Value) -> Value {
    json!({"key":key,"value":value})
}
fn string(key: &str, value: &str) -> Value {
    attribute(key, json!({"stringValue":value}))
}
fn selectors() -> Vec<Value> {
    vec![
        string("dpe.rollout_kind", KIND),
        string("dpe.rollout_name", NAME),
    ]
}
fn span(id: &str, attributes: Vec<Value>) -> Value {
    json!({"traceId":"1234567890abcdef1234567890abcdef", "spanId":id,
        "name":"rollout", "startTimeUnixNano":"1234567890000000000",
        "endTimeUnixNano":"1234567890000001000", "attributes":attributes})
}
fn batch(spans: Vec<Value>) -> Value {
    json!({"scopeSpans":[{"spans":spans}]})
}
fn inspect(trace: &Value) -> Value {
    inspect_trace(trace, KIND, NAME).unwrap()
}

#[test]
fn matching_span_retains_typed_identity_resource_and_time_material() {
    let mut attributes = selectors();
    attributes.push(string("dpe.rollout_uid", "object-123"));
    attributes.push(attribute("dpe.rollout_generation", json!({"intValue":"7"})));
    let mut batch = batch(vec![span("1234567890abcdef", attributes)]);
    batch["resource"] = json!({"attributes":[string("k8s.cluster.name","dev-measured-label"),string("service.name","operator")]});
    let result = inspect(&json!({"resourceSpans":[batch]}));
    assert_eq!(result["candidate_matches"], 1);
    assert_eq!(result["structurally_valid"], true);
    let matched = &result["matching_spans"][0];
    assert_eq!(matched["trace_id"], "1234567890abcdef1234567890abcdef");
    assert_eq!(matched["span_id"], "1234567890abcdef");
    assert_eq!(matched["start_time_unix_nano"], "1234567890000000000");
    assert_eq!(
        matched["cluster_attributes"]["resource"]["k8s.cluster.name"]["stringValue"],
        "dev-measured-label"
    );
    assert_eq!(
        matched["rollout_identity_attributes"]["span"]["dpe.rollout_generation"]["intValue"],
        "7"
    );
    for field in [
        "provenance_verified",
        "current_state_verified",
        "fleet_coverage_verified",
        "completeness_verified",
    ] {
        assert_eq!(result[field], false);
    }
    assert!(result.get("accepted").is_none());
}

#[test]
fn split_span_and_resource_only_matches_are_not_candidates() {
    let first = span("1234567890abcdef", vec![string("dpe.rollout_kind", KIND)]);
    let second = span("2234567890abcdef", vec![string("dpe.rollout_name", NAME)]);
    let mut batch = batch(vec![first, second]);
    batch["resource"] = json!({"attributes":selectors()});
    let result = inspect(&json!({"batches":[batch]}));
    assert_eq!(result["candidate_matches"], 0);
    assert_eq!(result["span_count"], 2);
    assert_eq!(result["structurally_valid"], true);
}

#[test]
fn typed_values_are_not_coerced_to_strings() {
    for wrong in [
        json!({"intValue":"1"}),
        json!({"boolValue":true}),
        json!({"arrayValue":{"values":[{"stringValue":KIND}]}}),
    ] {
        let result = inspect(
            &json!({"resourceSpans":[batch(vec![span("1234567890abcdef",vec![attribute("dpe.rollout_kind",wrong),string("dpe.rollout_name",NAME)])])]}),
        );
        assert_eq!(result["candidate_matches"], 0);
        assert_eq!(result["structurally_valid"], true);
    }
    let result = inspect(
        &json!({"resourceSpans":[batch(vec![span("1234567890abcdef",vec![attribute("dpe.rollout_kind",json!({"stringValue":17})),string("dpe.rollout_name",NAME)])])]}),
    );
    assert_eq!(result["candidate_matches"], 0);
    assert_eq!(result["structurally_valid"], false);
}

#[test]
fn duplicates_conflicts_and_multiple_value_types_invalidate_all_matches() {
    for extra in [
        string("dpe.rollout_kind", KIND),
        string("dpe.rollout_kind", "Other"),
        attribute("other", json!({"stringValue":"x","intValue":"1"})),
    ] {
        let mut attributes = selectors();
        attributes.push(extra);
        let result = inspect(
            &json!({"batches":[batch(vec![span("1234567890abcdef",selectors()),span("2234567890abcdef",attributes)])]}),
        );
        assert_eq!(result["candidate_matches"], 0);
        assert_eq!(result["matching_spans"], json!([]));
        assert_eq!(result["structurally_valid"], false);
        assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    }
}

#[test]
fn malformed_or_incomplete_payloads_cannot_match() {
    let good = span("1234567890abcdef", selectors());
    for field in ["traceId", "spanId", "startTimeUnixNano", "endTimeUnixNano"] {
        let mut incomplete = good.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        let result = inspect(&json!({"resourceSpans":[batch(vec![incomplete])]}));
        assert_eq!(result["candidate_matches"], 0, "{field}");
        assert_eq!(result["structurally_valid"], false, "{field}");
    }
    for malformed in [
        json!({"traces":[{"traceID":"search-summary"}]}),
        json!({"resourceSpans":{}}),
        json!({"resourceSpans":[{"scopeSpans":[{}]}]}),
        json!({"partial":true,"batches":[batch(vec![good.clone()])]}),
        json!({"resourceSpans":[batch(vec![good.clone()])],"batches":[]}),
    ] {
        assert_eq!(inspect(&malformed)["candidate_matches"], 0);
        assert_eq!(inspect(&malformed)["structurally_valid"], false);
    }
    let mut dropped = good.clone();
    dropped["droppedAttributesCount"] = json!(1);
    let mut reversed = good.clone();
    reversed["endTimeUnixNano"] = json!(1);
    let mut zero = good;
    zero["traceId"] = json!("00000000000000000000000000000000");
    for malformed in [dropped, reversed, zero] {
        assert_eq!(
            inspect(&json!({"batches":[batch(vec![malformed])]}))["candidate_matches"],
            0
        );
    }
}

#[test]
fn multiple_batches_and_snake_case_or_legacy_scopes_are_inspected() {
    let mut snake = span("2234567890abcdef", selectors());
    for (camel, snake_key) in [
        ("traceId", "trace_id"),
        ("spanId", "span_id"),
        ("startTimeUnixNano", "start_time_unix_nano"),
        ("endTimeUnixNano", "end_time_unix_nano"),
    ] {
        snake[snake_key] = snake.as_object_mut().unwrap().remove(camel).unwrap();
    }
    snake["attributes"][0]["value"] = json!({"string_value":KIND});
    let result = inspect(
        &json!({"resource_spans":[batch(vec![span("1234567890abcdef",selectors())]),{"scope_spans":[{"spans":[snake]}]},{"instrumentationLibrarySpans":[{"spans":[span("3234567890abcdef",selectors())]}]}]}),
    );
    assert_eq!(result["candidate_matches"], 3);
    assert_eq!(result["span_count"], 3);
    assert_eq!(result["resource_span_count"], 3);
}

#[test]
fn duplicate_resource_attributes_cannot_validate_an_otherwise_matching_span() {
    let mut batch = batch(vec![span("1234567890abcdef", selectors())]);
    batch["resource"] =
        json!({"attributes":[string("k8s.cluster.name","dev"),string("k8s.cluster.name","other")]});
    let result = inspect(&json!({"batches":[batch]}));
    assert_eq!(result["candidate_matches"], 0);
    assert_eq!(result["structurally_valid"], false);
}

#[test]
fn empty_collections_and_nonmatching_strings_remain_measurements_only() {
    assert_eq!(inspect(&json!({"batches":[]}))["candidate_matches"], 0);
    assert_eq!(inspect(&json!({"batches":[]}))["structurally_valid"], true);
    let result = inspect(
        &json!({"batches":[batch(vec![span("1234567890abcdef",vec![string("dpe.rollout_kind",KIND),string("dpe.rollout_name","other")])])]}),
    );
    assert_eq!(result["candidate_matches"], 0);
    assert_eq!(result["structurally_valid"], true);
    assert!(inspect_trace(&json!({"batches":[]}), "", NAME).is_err());
}
