//! Offline retained observations preserve provenance, age and independent decisions.
use engarg::retained_collectors::observe;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use tempfile::TempDir;

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EARLIER: &str = "2025-01-01T12:00:00Z";
const LATER: &str = "2025-01-02T12:00:00.100Z";

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn request() -> Value {
    json!({"evidence_id":"acquisition_4531","environment":"engarg_case",
        "tool":"acquisition_tool_4531","tool_version":"1",
        "input":{"schema":"engarg.eal-acquisition/1","case_digest":DIGEST,
            "need":{"id":"E1","acceptance_criterion":"independent interpretation required"},
            "tool_id":"custom","arguments":{},"unresolved_arguments":["adapter"]},
        "context":{"engarg_export_digest":DIGEST}})
}

fn correspondence(request: &Value) -> Value {
    json!({"tool":request["tool"],"tool_version":request["tool_version"],
        "input":request["input"],"context":request["context"]})
}

fn save_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn artifact(directory: &Path, name: &str, bytes: &[u8], observed_at: &str) -> Value {
    fs::write(directory.join(name), bytes).unwrap();
    json!({"path":name,"sha256":hash(bytes),
        "observed_at":observed_at,"source":"independently retained test measurement"})
}

fn artifacts(entries: Vec<Value>) -> Value {
    json!({"schema":"engarg.artifact-inputs/1","case_digest":DIGEST,
        "needs":{"E1":{"artifacts":entries}}})
}

fn review_request() -> Value {
    let mut request = request();
    request["tool"] = json!("engarg_review_contract");
    request["input"] = json!({"schema":"engarg.eal-export/1","case_digest":DIGEST,
        "obligation":{"id":"evidence_need:E1","requirement":"review actual source"},
        "authored_detail":{"text":"bounded sample"},"required_acquisitions":[{"need_id":"E1"}],
        "criterion":"review actual source","target":"evidence_need:E1","scope":{"case_digest":DIGEST,"target_id":"evidence_need:E1"}});
    request
}

fn reviews(request: &Value, value: Value) -> Value {
    json!({"schema":"eal.review-inputs/2","reviews":[{
        "request":correspondence(request),"value":value}]})
}

fn review_value(verdict: &str) -> Value {
    json!({"criterion":"review actual source","target":"evidence_need:E1",
        "scope":{"case_digest":DIGEST,"target_id":"evidence_need:E1"},"reviewer":"identified-test-reviewer",
        "verdict":verdict,"reasons":["The retained decision is not inferred by this importer."],"reviewed_at":EARLIER})
}

#[test]
fn artifacts_retain_original_age_pins_source_and_measured_negative() {
    let directory = TempDir::new().unwrap();
    let first = artifact(
        directory.path(),
        "first.json",
        br#"{"present":false,"large":123456789012345678901234567890}"#,
        LATER,
    );
    let second = artifact(
        directory.path(),
        "second.json",
        br#"{"observed":3}"#,
        EARLIER,
    );
    let path = directory.path().join("inputs.json");
    save_json(&path, &artifacts(vec![first.clone(), second]));
    let request = request();
    let result = observe("artifact_json", request.clone(), &path).unwrap();
    assert_eq!(result["request"], correspondence(&request));
    assert_eq!(result["context"], request["context"]);
    assert_eq!(result["observed_at"], EARLIER);
    assert_eq!(result["value"]["collected"], true);
    assert_eq!(
        result["value"]["artifacts"][0]["observation"]["present"],
        false
    );
    assert_eq!(result["value"]["artifacts"][0]["sha256"], first["sha256"]);
    assert_eq!(result["value"]["artifacts"][0]["source"], first["source"]);
    assert_eq!(result["value"]["artifacts"][0]["observed_at"], LATER);
    assert_eq!(
        result["value"]["artifacts"][0]["observation"]["large"].to_string(),
        "123456789012345678901234567890"
    );
    assert!(result["value"].get("accepted").is_none());
    assert_eq!(
        result["details"]["inputs_sha256"],
        hash(&fs::read(path).unwrap())
    );
}

#[test]
fn missing_empty_unpinned_or_transplanted_artifacts_fail() {
    let directory = TempDir::new().unwrap();
    let first = artifact(directory.path(), "first.json", b"{}", EARLIER);
    let path = directory.path().join("inputs.json");
    let mut variants = vec![artifacts(vec![]), artifacts(vec![first.clone()])];
    variants[1]["case_digest"] =
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let mut missing = artifacts(vec![first.clone()]);
    missing["needs"] = json!({});
    variants.push(missing);
    let mut unpinned = artifacts(vec![first.clone()]);
    unpinned["needs"]["E1"]["artifacts"][0]["sha256"] = json!("0".repeat(64));
    variants.push(unpinned);
    let mut provenance = artifacts(vec![first.clone()]);
    provenance["needs"]["E1"]["artifacts"][0]["source"] = json!("");
    variants.push(provenance);
    let mut timestamp = artifacts(vec![first.clone()]);
    timestamp["needs"]["E1"]["artifacts"][0]["observed_at"] = json!("2025-02-29T12:00:00Z");
    variants.push(timestamp);
    variants.push(artifacts(vec![first.clone(), first]));
    for variant in variants {
        save_json(&path, &variant);
        assert!(
            observe("artifact_json", request(), &path).is_err(),
            "{variant}"
        );
    }
}

#[test]
fn modified_artifact_bytes_and_ambiguous_json_fail() {
    let directory = TempDir::new().unwrap();
    let first = artifact(directory.path(), "first.json", b"{}", EARLIER);
    let path = directory.path().join("inputs.json");
    save_json(&path, &artifacts(vec![first]));
    fs::write(directory.path().join("first.json"), b"{\"changed\":true}").unwrap();
    assert!(observe("artifact_json", request(), &path).is_err());
    for bytes in [
        br#"{"same":true,"same":false}"#.as_slice(),
        b"[]",
        b"{} trailing",
        b"{\"a\":NaN}",
    ] {
        let artifact = artifact(directory.path(), "first.json", bytes, EARLIER);
        save_json(&path, &artifacts(vec![artifact]));
        assert!(observe("artifact_json", request(), &path).is_err());
    }
}

#[test]
fn parent_traversal_and_symlink_escape_fail_but_absolute_grants_work() {
    let directory = TempDir::new().unwrap();
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let first = artifact(directory.path(), "outside.json", b"{}", EARLIER);
    let path = nested.join("inputs.json");
    let mut selected = first.clone();
    selected["path"] = json!("../outside.json");
    save_json(&path, &artifacts(vec![selected]));
    assert!(observe("artifact_json", request(), &path).is_err());
    let mut selected = first;
    selected["path"] = json!(directory.path().join("outside.json"));
    save_json(&path, &artifacts(vec![selected]));
    assert!(observe("artifact_json", request(), &path).is_ok());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            directory.path().join("outside.json"),
            nested.join("linked.json"),
        )
        .unwrap();
        let mut selected = artifacts(vec![]);
        selected["needs"]["E1"]["artifacts"] = json!([{
            "path":"linked.json","sha256":hash(b"{}"),
            "observed_at":EARLIER,"source":"retained fixture"}]);
        save_json(&path, &selected);
        assert!(observe("artifact_json", request(), &path).is_err());
    }
}

#[test]
fn retained_reviews_preserve_scoped_verdicts_and_original_clock() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("reviews.json");
    let request = review_request();
    for verdict in ["supported", "rejected", "inconclusive"] {
        let value = review_value(verdict);
        save_json(&path, &reviews(&request, value.clone()));
        let result = observe("review_json", request.clone(), &path).unwrap();
        assert_eq!(result["value"], value);
        assert_eq!(result["observed_at"], EARLIER);
        assert_eq!(result["request"], correspondence(&request));
        assert_eq!(result["context"], request["context"]);
    }
}

#[test]
fn reviews_do_not_accept_changed_requests_contexts_or_unbound_values() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("reviews.json");
    let request = review_request();
    save_json(&path, &reviews(&request, review_value("supported")));
    for (pointer, replacement) in [
        ("/input/authored_detail/text", json!("different")),
        ("/input/required_acquisitions", json!([])),
        ("/context/engarg_export_digest", json!("b".repeat(64))),
        ("/input/case_digest", json!("b".repeat(64))),
        ("/tool_version", json!("2")),
        ("/tool", json!("a_different_review_tool")),
    ] {
        let mut changed = request.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(observe("review_json", changed, &path).is_err(), "{pointer}");
    }
    for value in [
        json!({}),
        json!({"accepted":"true"}),
        json!(true),
        json!({"checked_by_user":true}),
    ] {
        save_json(&path, &reviews(&request, value));
        assert!(observe("review_json", request.clone(), &path).is_err());
    }
    for (field, replacement) in [
        ("reviewer", json!("")),
        ("criterion", json!("different")),
        ("target", json!("different")),
        ("scope", json!({"case_digest":DIGEST})),
        ("reasons", json!([])),
        ("reviewed_at", json!("2025-02-29T12:00:00Z")),
    ] {
        let mut value = review_value("supported");
        value[field] = replacement;
        save_json(&path, &reviews(&request, value));
        assert!(
            observe("review_json", request.clone(), &path).is_err(),
            "{field}"
        );
    }
    let mut duplicate = reviews(&request, review_value("supported"));
    let repeated = duplicate["reviews"][0].clone();
    duplicate["reviews"].as_array_mut().unwrap().push(repeated);
    save_json(&path, &duplicate);
    assert!(observe("review_json", request, &path).is_err());
}

#[test]
fn file_bounds_request_fields_and_duplicate_input_keys_are_enforced() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("inputs.json");
    let first = artifact(directory.path(), "first.json", b"{}", EARLIER);
    save_json(&path, &artifacts(vec![first]));
    let mut changed = request();
    changed["extra"] = json!(true);
    assert!(observe("artifact_json", changed, &path).is_err());
    let mut changed = request();
    changed["context"] = json!(null);
    assert!(observe("artifact_json", changed, &path).is_err());
    assert!(observe("unknown", request(), &path).is_err());
    fs::write(
        &path,
        br#"{"schema":"engarg.artifact-inputs/1","schema":"engarg.artifact-inputs/1"}"#,
    )
    .unwrap();
    assert!(observe("artifact_json", request(), &path).is_err());
    fs::write(&path, vec![b' '; 1_048_577]).unwrap();
    assert!(observe("artifact_json", request(), &path).is_err());
}

#[test]
fn oldest_fractional_instant_is_preserved_without_refreshing_it() {
    let directory = TempDir::new().unwrap();
    let first = artifact(
        directory.path(),
        "first.json",
        b"{}",
        "2024-02-29T12:00:00.01Z",
    );
    let second = artifact(
        directory.path(),
        "second.json",
        b"{}",
        "2024-02-29T12:00:00.001Z",
    );
    let path = directory.path().join("inputs.json");
    save_json(&path, &artifacts(vec![first, second]));
    let result = observe("artifact_json", request(), &path).unwrap();
    assert_eq!(result["observed_at"], "2024-02-29T12:00:00.001Z");
}
