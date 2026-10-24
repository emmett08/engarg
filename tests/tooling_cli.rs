//! Admission and registration regressions that require neither Docker nor credentials.
use serde_json::{Value, json};
use std::{fs, path::Path, process::{Command, Output}};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command.env_remove("UNSET_TOOL_TEST_KEY");
    command
}

fn failed(output: Output, expected: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stderr: {}", String::from_utf8_lossy(&output.stderr),
    );
}

fn demo(path: &Path) {
    let output = command()
        .args(["demo", "--format", "json", "--output"])
        .arg(path)
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn inactive_tooling_flags_are_rejected_before_provider_credentials() {
    failed(command().args([
        "analyse", "--claim", "The bounded engineering claim is supported.",
        "--model", "fixture", "--api-key-env", "UNSET_TOOL_TEST_KEY",
        "--tool-language", "python",
    ]).output().unwrap(), "require --implement-tools");
}

#[test]
fn implementation_requires_a_retained_json_output_before_provider_admission() {
    failed(command().args([
        "analyse", "--claim", "The bounded engineering claim is supported.",
        "--model", "fixture", "--api-key-env", "UNSET_TOOL_TEST_KEY",
        "--implement-tools",
    ]).output().unwrap(), "requires --format json and --output");
}

#[test]
fn implementation_requires_language_before_provider_credentials() {
    let directory = tempfile::tempdir().unwrap();
    failed(command().args([
        "analyse", "--claim", "The bounded engineering claim is supported.",
        "--model", "fixture", "--api-key-env", "UNSET_TOOL_TEST_KEY",
        "--implement-tools", "--format", "json", "--output",
    ]).arg(directory.path().join("analysis.json")).output().unwrap(), "requires --tool-language");
    assert!(!directory.path().join("analysis.json").exists());
}

#[test]
fn saved_implementation_requires_an_operator_toolchain() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    demo(&input);
    failed(command().args(["eal", "implement", "--input"]).arg(input).args([
        "--model", "fixture", "--api-key-env", "UNSET_TOOL_TEST_KEY",
        "--tool-language", "python", "--tool-output-dir",
    ]).arg(directory.path().join("attempt")).output().unwrap(), "requires --toolchain");
    assert!(!directory.path().join("attempt").exists());
}

#[test]
fn unverified_tooling_metadata_cannot_create_an_export_registry() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    demo(&input);
    let mut report: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    report["tooling"] = json!({"schema": "engarg.tooling/1", "packages": []});
    fs::write(&input, serde_json::to_vec(&report).unwrap()).unwrap();
    let source = directory.path().join("argument.eal");
    let tools = directory.path().join("argument.tools.toml");
    let output = command().args(["eal", "export", "--input"]).arg(input)
        .arg("--output").arg(&source).arg("--tools-output").arg(&tools).output().unwrap();
    assert!(!output.status.success());
    assert!(!source.exists());
    assert!(!tools.exists());
}

#[test]
fn hidden_launcher_rejects_an_unverified_manifest_without_observations() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("manifest.json");
    fs::write(&package, b"{}").unwrap();
    let output = command().args(["eal", "tool-run", "--package"]).arg(package)
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
