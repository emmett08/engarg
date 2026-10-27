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

fn revised_selected_runtime_report(path: &Path) -> Value {
    authored(path);
    let mut report = engarg::report::read_direct(
        &serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
    )
    .unwrap();
    let mut target = engarg::runtime_compatibility::embedded().unwrap();
    target.kind = engarg::runtime_compatibility::TargetKind::SelectedHost;
    target.implementation_version = Some("synthetic-authoring-host".into());
    target.executable_sha256 = Some("a".repeat(64));
    report.target_runtime = Some(target);
    let mut argument = report.argument.clone();
    argument
        .adjudication
        .push_str(" The explicit revision retains the finite fixture scope.");
    let revised = engarg::engine::support::revision::revise_with_support(
        &report,
        argument,
        report.support_elaboration.clone(),
        None,
    )
    .unwrap();
    let value = serde_json::to_value(revised).unwrap();
    engarg::report::read_direct(&value).unwrap();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    value
}

fn implementation_command(root: &Path, endpoint: &str) -> Command {
    let toolchain = root.join("toolchain.json");
    fs::write(
        &toolchain,
        serde_json::to_vec(&json!({
            "schema":"engarg.toolchain/1", "language":"python",
            "docker":"must-not-start", "image":"must-not-start",
            "build_argv":[], "run_argv":["python3"], "inherit_env":[],
            "runtime_network":"none", "timeout_seconds":30,
            "memory_mb":128, "output_bytes":1048576
        }))
        .unwrap(),
    )
    .unwrap();
    let mut invocation = command();
    invocation
        .env_remove("EAL_HOST")
        .args(["eal", "implement", "--input"])
        .arg(root.join("analysis.json"))
        .arg("--tools")
        .arg(tools(root))
        .args(["--model", "fixture", "--endpoint", endpoint])
        .args([
            "--api-key-env",
            "UNSET_TOOL_TEST_KEY",
            "--http-retries",
            "0",
            "--request-timeout-secs",
            "1",
            "--tool-language",
            "python",
            "--toolchain",
        ])
        .arg(toolchain)
        .arg("--tool-store")
        .arg(root.join("store"))
        .arg("--tool-output-dir")
        .arg(root.join("attempt"))
        .args(["--tool-timeout-secs", "5"]);
    invocation
}

#[test]
fn revised_report_keeps_its_authoring_runtime_when_implementation_uses_embedded_contracts() {
    use std::{io, net::TcpListener};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let original = revised_selected_runtime_report(&root.join("analysis.json"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    let output = implementation_command(root, &endpoint).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let implemented: Value =
        serde_json::from_slice(&fs::read(root.join("attempt/analysis.with-tools.json")).unwrap())
            .unwrap();
    engarg::report::read_direct(&implemented).unwrap();
    assert_eq!(implemented["target_runtime"], original["target_runtime"]);
    assert_eq!(
        implemented["argument_revisions"],
        original["argument_revisions"]
    );
    assert_eq!(implemented["tooling"]["model_calls"], 0);
    assert_eq!(
        implemented["tooling"]["deferred"].as_array().unwrap().len(),
        2
    );
    assert!(!root.join("store").exists());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[cfg(unix)]
#[test]
fn revised_implementation_admits_the_selected_host_before_provider_calls() {
    use std::{io, net::TcpListener, os::unix::fs::PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    revised_selected_runtime_report(&root.join("analysis.json"));
    let host = root.join("incompatible-host");
    fs::write(
        &host,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"operation\":\"describe\",\"protocol_version\":\"2025-11-25\",\"is_error\":false,\"result\":{\"source_syntax\":\"EAL/3\"},\"content\":[]}'\n",
    )
    .unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    let output = implementation_command(root, &endpoint)
        .arg("--eal-host")
        .arg(host)
        .output()
        .unwrap();
    failed(output, "EAL_TARGET_INCOMPATIBLE");
    assert!(!root.join("attempt").exists());
    assert!(!root.join("store").exists());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}
