//! Model-free checks upload bounded snapshots instead of binding client paths.
#![cfg(unix)]

use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

struct Check {
    output: std::process::Output,
    receipt: Value,
    events: Vec<Value>,
}

fn check(fault: Option<&str>) -> Check {
    let directory = tempfile::tempdir().unwrap();
    let docker = directory.path().join("docker");
    let log = directory.path().join("docker.jsonl");
    let config = json!({"fault":fault,"log":log,"state":directory.path().join("state.json")});
    let encoded = serde_json::to_string(&serde_json::to_string(&config).unwrap()).unwrap();
    fs::write(
        &docker,
        format!(
            "#!/usr/bin/env python3\nimport json\nCONFIG = json.loads({encoded})\n{}",
            include_str!("fixtures/daemon_snapshot_docker.py")
        ),
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let mut profile: Value =
        serde_json::from_str(include_str!("../examples/toolchains/rust.json")).unwrap();
    profile["docker"] = json!(docker);
    let profile_file = directory.path().join("toolchain.json");
    fs::write(&profile_file, serde_json::to_vec(&profile).unwrap()).unwrap();
    let receipt_file = directory.path().join("receipt.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "toolchain-check", "--toolchain"])
        .arg(profile_file)
        .arg("--output")
        .arg(&receipt_file)
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    let receipt = serde_json::from_slice(&fs::read(receipt_file).unwrap()).unwrap();
    let events = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    Check {
        output,
        receipt,
        events,
    }
}

fn events<'a>(check: &'a Check, operation: &str) -> Vec<&'a Value> {
    check
        .events
        .iter()
        .filter(|event| event["operation"] == operation)
        .collect()
}

fn failure_text(check: &Check) -> String {
    format!(
        "{} {}",
        check.receipt["stderr"], check.receipt["diagnostic"]
    )
}

fn assert_owned_resources_cleaned(check: &Check) {
    let created = events(check, "create");
    assert_eq!(created.len(), 1, "{:?}", check.events);
    let stager = created[0]["name"].as_str().unwrap();
    let worker = stager.strip_suffix("-inputs").unwrap();
    let removed = events(check, "container rm");
    let stager_cleanup = removed
        .iter()
        .find(|event| event["name"] == stager)
        .unwrap();
    assert_eq!(stager_cleanup["volumes"], true, "{:?}", check.events);
    let stager_index = check
        .events
        .iter()
        .position(|event| event["operation"] == "container rm" && event["name"] == stager)
        .unwrap();
    let worker_index = check
        .events
        .iter()
        .position(|event| event["operation"] == "container rm" && event["name"] == worker);
    if events(check, "run").is_empty() {
        assert!(worker_index.is_none(), "{:?}", check.events);
    } else {
        assert!(worker_index.unwrap() < stager_index, "{:?}", check.events);
    }
    assert!(events(check, "start").is_empty());
}

#[test]
fn complete_restricted_preflight_uploads_snapshots_without_host_paths_or_model_credentials() {
    let check = check(None);
    assert!(
        check.output.status.success(),
        "{}",
        String::from_utf8_lossy(&check.output.stderr)
    );
    assert_eq!(check.receipt["passed"], true);
    assert_eq!(check.receipt["stage"], "execution");
    assert_eq!(check.receipt["returncode"], 0);
    assert_eq!(events(&check, "run").len(), 1);
    let uploads = events(&check, "cp");
    assert_eq!(uploads.len(), 2);
    assert!(
        uploads
            .iter()
            .any(|event| event["destination"] == "/src" && event["files"] == json!(["main.rs"]))
    );
    assert!(
        uploads
            .iter()
            .any(|event| event["destination"] == "/fixtures"
                && event["files"] == json!(["engarg-preflight.txt"]))
    );
    assert!(check.events.iter().all(|event| {
        !event["argv"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.as_str().unwrap().contains("type=bind"))
    }));
    assert_owned_resources_cleaned(&check);
}

#[test]
fn docker_launch_failure_is_retained_with_status_and_stderr_and_cleanup() {
    let check = check(Some("run"));
    assert!(!check.output.status.success());
    assert_eq!(check.receipt["passed"], false);
    assert_eq!(check.receipt["returncode"], 125);
    assert_eq!(
        check.receipt["execution_error"],
        "container_execution_error"
    );
    assert!(failure_text(&check).contains("synthetic worker launch denial"));
    assert!(String::from_utf8_lossy(&check.output.stderr).contains("tool execution error"));
    assert_owned_resources_cleaned(&check);
}

#[test]
fn admission_failure_still_writes_a_failed_receipt_without_allocating_resources() {
    let check = check(Some("image"));
    assert!(!check.output.status.success());
    assert_eq!(check.receipt["stage"], "admission");
    assert_eq!(check.receipt["passed"], false);
    assert!(failure_text(&check).contains("synthetic image admission denial"));
    assert!(events(&check, "create").is_empty());
    assert!(events(&check, "container rm").is_empty());
}

#[test]
fn failed_stager_creation_cannot_delete_a_container_it_did_not_create() {
    for fault in ["create", "create-collision"] {
        let check = check(Some(fault));
        assert!(!check.output.status.success());
        assert_eq!(check.receipt["passed"], false);
        assert!(failure_text(&check).contains("synthetic staging creation denial"));
        assert!(events(&check, "cp").is_empty());
        assert!(events(&check, "run").is_empty());
        assert!(
            !events(&check, "container rm")
                .iter()
                .any(|event| event["name"].as_str().unwrap().ends_with("-inputs")),
            "{fault}: {:?}",
            check.events
        );
    }
}

#[test]
fn ambiguous_stager_creation_failure_still_releases_its_owned_volumes() {
    let check = check(Some("create-ambiguous"));
    assert!(!check.output.status.success());
    assert_eq!(check.receipt["passed"], false);
    assert!(events(&check, "cp").is_empty());
    assert!(events(&check, "run").is_empty());
    assert_owned_resources_cleaned(&check);
}

#[test]
fn failed_inspection_or_upload_stops_before_worker_execution_and_releases_owned_resources() {
    for fault in ["inspect", "inspect-malformed", "cp-source", "cp-fixtures"] {
        let check = check(Some(fault));
        assert!(!check.output.status.success(), "{fault}");
        assert_eq!(check.receipt["passed"], false, "{fault}");
        assert!(events(&check, "run").is_empty(), "{fault}");
        assert_owned_resources_cleaned(&check);
    }
}

#[test]
fn cleanup_failure_still_attempts_every_owned_cleanup_and_blocks_success() {
    let check = check(Some("cleanup-worker"));
    assert!(!check.output.status.success());
    assert_eq!(check.receipt["passed"], false);
    assert!(failure_text(&check).contains("synthetic worker cleanup denial"));
    assert_owned_resources_cleaned(&check);
}
