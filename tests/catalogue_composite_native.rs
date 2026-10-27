//! Real local MCP + audited calculation + EAL/3, with a pending review.
#![cfg(unix)]
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "requires matching EAL_COLLECTOR, ENGARG_EAL_HOST and ENGARG_CATALOGUE_MCP binaries"]
fn composite_evidence_is_observed_but_an_independent_review_stays_unresolved() {
    let executable = |name| {
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("Set {name}")))
            .canonicalize()
            .unwrap()
    };
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("ENGARG_CATALOGUE_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_path_buf());
    std::fs::create_dir_all(&directory).unwrap();
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/catalogue_composite_native.py"
        ))
        .args([
            directory.as_path(),
            &executable("EAL_COLLECTOR"),
            &executable("ENGARG_EAL_HOST"),
            &executable("ENGARG_CATALOGUE_MCP"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}\nArtifacts: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        directory.display()
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for scenario in ["positive", "incomplete"] {
        assert_eq!(result[scenario]["claim_status"], "unsupported");
        assert_eq!(result[scenario]["review_status"], "unavailable");
    }
    assert_eq!(result["positive"]["source_calls"], 1);
    assert_eq!(result["incomplete"]["source_calls"], 0);
}
