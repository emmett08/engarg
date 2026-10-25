//! Actual extra MCP -> fixture custom operation -> EAL/3 collection and reasoning.
//!
//! The custom operation in this test is independently authored test code. Live
//! model authoring and Docker package generation are separate acceptance stages.
#![cfg(unix)]

use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires matching EAL_COLLECTOR, ENGARG_EAL_HOST and ENGARG_CATALOGUE_MCP binaries"]
fn additional_mcp_and_custom_operation_support_only_the_measured_catalogue_claim() {
    let executable = |name| {
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("Set {name}")))
            .canonicalize()
            .unwrap()
    };
    let collector = executable("EAL_COLLECTOR");
    let host = executable("ENGARG_EAL_HOST");
    let server = executable("ENGARG_CATALOGUE_MCP");
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("ENGARG_CATALOGUE_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_path_buf());
    std::fs::create_dir_all(&directory).unwrap();
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/catalogue_workflow_native.py"
        ))
        .args([
            directory.as_os_str(),
            collector.as_os_str(),
            host.as_os_str(),
            server.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Native catalogue acceptance failed.\nstdout: {}\nstderr: {}\nArtifacts: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        directory.display()
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["positive"]["claim_status"], "supported");
    assert_eq!(
        summary["duplicate_identifiers"]["claim_status"],
        "unsupported"
    );
    assert_eq!(summary["mcp_unavailable"]["claim_status"], "unsupported");
}
