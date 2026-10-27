//! Direct operation admission requires neither Docker nor provider credentials.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command.env_remove("UNSET_TOOL_TEST_KEY");
    command
}

fn failed(output: Output, expected: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn authored(path: &Path) {
    let (request, argument, support) = executable_case::fixture();
    let report = json!({
        "schema_version": engarg::report::RUN_REPORT_SCHEMA,
        "execution_mode": "offline_synthetic_fixture",
        "input_sha256": eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        "mode": "single", "request": request, "argument": argument,
        "transcript": [], "support_elaboration": support,
        "limitations": ["Synthetic admission fixture; no provider calls."]
    });
    engarg::report::read_direct(&report).unwrap();
    fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
}

fn tools(root: &Path) -> std::path::PathBuf {
    let path = root.join("tools.toml");
    fs::write(&path, "[tools]\n").unwrap();
    path
}

#[test]
fn inactive_tooling_flags_are_rejected_before_provider_credentials() {
    failed(
        command()
            .args([
                "analyse",
                "--claim",
                "The bounded engineering claim is supported.",
                "--model",
                "fixture",
                "--api-key-env",
                "UNSET_TOOL_TEST_KEY",
                "--tool-language",
                "python",
            ])
            .output()
            .unwrap(),
        "require --implement-tools",
    );
}

#[test]
fn implementation_requires_a_retained_json_output_before_provider_admission() {
    failed(
        command()
            .args([
                "analyse",
                "--claim",
                "The bounded engineering claim is supported.",
                "--model",
                "fixture",
                "--api-key-env",
                "UNSET_TOOL_TEST_KEY",
                "--implement-tools",
            ])
            .output()
            .unwrap(),
        "requires --format json and --output",
    );
}

#[test]
fn direct_authoring_requires_existing_operator_bindings_before_provider_credentials() {
    let directory = tempfile::tempdir().unwrap();
    failed(
        command()
            .args([
                "analyse",
                "--claim",
                "The bounded engineering claim is supported.",
                "--model",
                "fixture",
                "--api-key-env",
                "UNSET_TOOL_TEST_KEY",
                "--implement-tools",
                "--format",
                "json",
                "--output",
            ])
            .arg(directory.path().join("analysis.json"))
            .output()
            .unwrap(),
        "direct tool generation requires --tools",
    );
    assert!(!directory.path().join("analysis.json").exists());
}

#[test]
fn saved_implementation_requires_language_before_provider_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    authored(&input);
    failed(
        command()
            .args(["eal", "implement", "--input"])
            .arg(input)
            .arg("--tools")
            .arg(tools(directory.path()))
            .args([
                "--model",
                "fixture",
                "--api-key-env",
                "UNSET_TOOL_TEST_KEY",
                "--tool-output-dir",
            ])
            .arg(directory.path().join("attempt"))
            .output()
            .unwrap(),
        "requires --tool-language",
    );
    assert!(!directory.path().join("attempt").exists());
}

#[test]
fn saved_implementation_requires_an_operator_toolchain() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    authored(&input);
    failed(
        command()
            .args(["eal", "implement", "--input"])
            .arg(input)
            .arg("--tools")
            .arg(tools(directory.path()))
            .args([
                "--model",
                "fixture",
                "--api-key-env",
                "UNSET_TOOL_TEST_KEY",
                "--tool-language",
                "python",
                "--tool-output-dir",
            ])
            .arg(directory.path().join("attempt"))
            .output()
            .unwrap(),
        "requires --toolchain",
    );
    assert!(!directory.path().join("attempt").exists());
}

#[test]
fn obsolete_tooling_metadata_cannot_create_an_executable_package() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    authored(&input);
    let mut report: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    report["tooling"] = json!({"schema": "engarg.tooling/1", "packages": []});
    fs::write(&input, serde_json::to_vec(&report).unwrap()).unwrap();
    let package = directory.path().join("package");
    let bindings = tools(directory.path());
    let output = command()
        .args(["eal", "export", "--input"])
        .arg(input)
        .arg("--output")
        .arg(&package)
        .arg("--tools")
        .arg(&bindings)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!package.exists());
}

#[test]
fn hidden_launcher_rejects_an_unverified_manifest_without_observations() {
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("manifest.json");
    fs::write(&package, b"{}").unwrap();
    let output = command()
        .args(["eal", "tool-run", "--package"])
        .arg(package)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
