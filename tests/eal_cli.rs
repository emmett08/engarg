//! Deterministic CLI contracts. Host calls use an executable test double, not MCP
//! or language-model execution. Real-host integration is tested separately.

use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
}

fn report(path: &Path) {
    let value = json!({
        "request":serde_json::from_str::<Value>(include_str!("../examples/request.json")).unwrap(),
        "argument":serde_json::from_str::<Value>(include_str!("../examples/case.json")).unwrap()
    });
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn export_keeps_exact_source_and_loss_preserving_manifest() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("report.json");
    let source = directory.path().join("case.eal");
    let bundle = directory.path().join("case.eal.json");
    report(&input);
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&source)
        .arg("--manifest")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    let exported: Value = serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&source).unwrap(),
        exported["source"]
    );
    assert_eq!(exported["target_language"], "EAL/3");
    assert_eq!(
        exported["original_request"]["claim"],
        serde_json::from_str::<Value>(include_str!("../examples/request.json")).unwrap()["claim"]
    );
    assert!(exported["claim_names"]["current_claim"].is_string());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&source).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&bundle).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn json_export_is_available_on_stdout_without_model_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("report.json");
    report(&input);
    let result = cli()
        .env_remove("ENGARG_API_KEY")
        .env_remove("ENGARG_MODEL")
        .args(["eal", "export", "--format", "json", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let exported: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        exported["obligations"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    );
    assert!(exported["context"].is_object());
}

#[test]
fn export_preflights_existing_and_aliased_destinations() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("report.json");
    let output = directory.path().join("case.eal");
    let manifest = directory.path().join("case.eal.json");
    report(&input);
    std::fs::write(&manifest, b"keep").unwrap();
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--manifest")
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert_eq!(std::fs::read(&manifest).unwrap(), b"keep");
    std::fs::remove_file(&manifest).unwrap();
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--manifest")
        .arg(directory.path().join(".").join("case.eal"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("different paths"));
}

#[test]
fn timeout_bounds_are_checked_before_host_execution() {
    let result = cli()
        .args([
            "eal",
            "describe",
            "--timeout-secs",
            "0",
            "--eal-host",
            "/must-not-execute",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("timeout-secs"));
}

#[cfg(unix)]
mod unix_host {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn quote(text: &str) -> String {
        format!("'{}'", text.replace('\'', "'\\''"))
    }

    fn host(
        directory: &Path,
        name: &str,
        response: Value,
        status: u8,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let executable = directory.join(format!("{name} host"));
        let capture = directory.join(format!("{name}.request.json"));
        let script = format!(
            "#!/bin/sh\ncat > {}\nprintf '%s\\n' {}\nexit {status}\n",
            quote(capture.to_str().unwrap()),
            quote(&response.to_string())
        );
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        (executable, capture)
    }

    fn success(operation: &str) -> Value {
        json!({"operation":operation,"protocol_version":"2025-11-25","is_error":false,"result":{"valid":true},"content":[]})
    }

    fn bundle(directory: &Path) -> (std::path::PathBuf, Value) {
        let input = directory.join("canonical.json");
        let output = directory.join("bundle.json");
        report(&input);
        let result = cli()
            .args(["eal", "export", "--input"])
            .arg(&input)
            .args(["--format", "json", "--output"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        (output, value)
    }

    #[test]
    fn raw_source_bytes_and_shell_metacharacters_remain_data() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.eal");
        let marker = directory.path().join("must-not-exist");
        let text = format!(
            "  module X {{\n  // $(touch {}) ; 'quoted' | data\n}}\n\n",
            marker.display()
        );
        std::fs::write(&input, &text).unwrap();
        let (executable, capture) = host(directory.path(), "raw", success("validate"), 0);
        let result = cli()
            .args(["eal", "validate", "--input"])
            .arg(&input)
            .arg("--eal-host")
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert_eq!(request["operation"], "validate");
        assert_eq!(request["source"], text);
        assert!(!marker.exists());
    }

    #[test]
    fn bundle_plan_maps_canonical_claim_and_assess_uses_bound_context() {
        let directory = tempfile::tempdir().unwrap();
        let (input, bundle) = bundle(directory.path());
        let (executable, capture) = host(directory.path(), "plan", success("plan"), 0);
        let result = cli()
            .args(["eal", "plan", "--input"])
            .arg(&input)
            .arg("--eal-host")
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert_eq!(request["claim"], bundle["claim_names"]["current_claim"]);
        assert_eq!(request["source"], bundle["source"]);
        let (executable, capture) = host(directory.path(), "reason", success("reason"), 0);
        let result = cli()
            .args(["eal", "assess", "--input"])
            .arg(&input)
            .args([
                "--now",
                "2026-10-23T18:00:00Z",
                "--collection-id",
                "existing-collection",
            ])
            .arg("--eal-host")
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert_eq!(request["operation"], "reason");
        assert_eq!(request["context"], bundle["context"]);
        assert_eq!(request["collection_id"], "existing-collection");
        assert_eq!(request["now"], "2026-10-23T18:00:00Z");
    }

    #[test]
    fn edited_bundle_is_rejected_before_host_but_raw_text_can_be_selected() {
        let directory = tempfile::tempdir().unwrap();
        let (input, mut bundle) = bundle(directory.path());
        bundle["source"] = json!("altered projection");
        std::fs::write(&input, serde_json::to_vec(&bundle).unwrap()).unwrap();
        let (executable, capture) = host(directory.path(), "tamper", success("validate"), 0);
        let result = cli()
            .args(["eal", "validate", "--input"])
            .arg(&input)
            .arg("--eal-host")
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!capture.exists());
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("differs from the current deterministic export")
        );
    }

    #[test]
    fn invalid_semantic_result_is_emitted_before_failure() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("invalid.eal");
        let output = directory.path().join("response.json");
        std::fs::write(&source, "unrecognised source").unwrap();
        let response = json!({"operation":"validate","protocol_version":"2025-11-25","is_error":false,"result":{"valid":false,"errors":["synthetic invalid source"]},"content":[]});
        let (executable, _) = host(directory.path(), "invalid", response.clone(), 0);
        let result = cli()
            .args(["eal", "validate", "--input"])
            .arg(&source)
            .arg("--eal-host")
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&output).unwrap()).unwrap(),
            response
        );
    }

    #[test]
    fn tool_error_envelope_is_printed_on_stdout_before_failure() {
        let directory = tempfile::tempdir().unwrap();
        let response = json!({"is_error":true,"error":"synthetic tool failure"});
        let (executable, _) = host(directory.path(), "failure", response.clone(), 1);
        let result = cli()
            .args(["eal", "describe", "--eal-host"])
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stdout).unwrap(),
            response
        );
    }

    #[test]
    fn existing_output_prevents_host_execution() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("response.json");
        std::fs::write(&output, b"keep").unwrap();
        let (executable, capture) = host(directory.path(), "preserve", success("describe"), 0);
        let result = cli()
            .args(["eal", "describe", "--eal-host"])
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!capture.exists());
        assert_eq!(std::fs::read(output).unwrap(), b"keep");
    }

    #[test]
    fn packet_and_explain_send_only_explicit_retrieval_fields() {
        let directory = tempfile::tempdir().unwrap();
        let (executable, capture) = host(directory.path(), "packet", success("packet"), 0);
        let result = cli()
            .args([
                "eal",
                "packet",
                "--assessment-id",
                "assessment-1",
                "--claim",
                "c_exact",
                "--max-bytes",
                "4096",
                "--max-statement-bytes",
                "128",
                "--eal-host",
            ])
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert_eq!(
            request,
            json!({"operation":"packet","assessment_id":"assessment-1","claim":"c_exact","packet_limits":{"max_bytes":4096,"max_statement_bytes":128}})
        );
        let (executable, capture) = host(directory.path(), "explain", success("explain"), 0);
        let result = cli()
            .args([
                "eal",
                "explain",
                "--assessment-id",
                "assessment-1",
                "--eal-host",
            ])
            .arg(executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert_eq!(
            request,
            json!({"operation":"explain","assessment_id":"assessment-1"})
        );
    }
}
