//! A model-free preflight retains Docker failures before authoring admission.
#![cfg(unix)]

use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn check(run_code: i32, admission_code: i32) -> (std::process::Output, Value) {
    let directory = tempfile::tempdir().unwrap();
    let docker = directory.path().join("docker");
    fs::write(&docker, format!(r#"#!/bin/sh
case "$1" in
  image)
    if [ {admission_code} -ne 0 ]; then printf 'synthetic image admission denial\n' >&2; exit {admission_code}; fi
    printf 'sha256:%064d\n' 0
    ;;
  run)
    # Prefix checks assert the same hardened invocation as generated collectors.
    case " $* " in *' --read-only '*) :;; *) exit 1;; esac
    case " $* " in *' --user 65534:65534 '*) :;; *) exit 1;; esac
    case " $* " in *' --network none '*) :;; *) exit 1;; esac
    case " $* " in *'target=/src,readonly'*) :;; *) exit 1;; esac
    case " $* " in *'target=/fixtures,readonly'*) :;; *) exit 1;; esac
    if [ {run_code} -ne 0 ]; then printf 'synthetic bind mount denial\n' >&2; exit {run_code}; fi
    cat >/dev/null
    printf '{{"schema":"engarg.toolchain-probe/1","passed":true}}\n'
    ;;
  container) exit 0;;
  *) exit 1;;
esac
"#)).unwrap();
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
    let receipt: Value = serde_json::from_slice(&fs::read(receipt_file).unwrap()).unwrap();
    (output, receipt)
}

#[test]
fn complete_restricted_preflight_requires_no_model_credentials() {
    let (output, receipt) = check(0, 0);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(receipt["passed"], true);
    assert_eq!(receipt["stage"], "execution");
    assert_eq!(receipt["returncode"], 0);
    assert!(
        receipt["runner"]["image_id"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
}

#[test]
fn docker_launch_failure_is_retained_with_status_and_stderr() {
    let (output, receipt) = check(125, 0);
    assert!(!output.status.success());
    assert_eq!(receipt["passed"], false);
    assert_eq!(receipt["returncode"], 125);
    assert_eq!(receipt["execution_error"], "container_execution_error");
    assert!(
        receipt["stderr"]
            .as_str()
            .unwrap()
            .contains("synthetic bind mount denial")
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("tool execution error"));
}

#[test]
fn admission_failure_still_writes_a_failed_receipt() {
    let (output, receipt) = check(0, 1);
    assert!(!output.status.success());
    assert_eq!(receipt["stage"], "admission");
    assert_eq!(receipt["passed"], false);
    assert!(
        receipt["diagnostic"]
            .as_str()
            .unwrap()
            .contains("synthetic image admission denial")
    );
}
