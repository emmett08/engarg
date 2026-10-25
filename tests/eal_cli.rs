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

fn planned_report(path: &Path, mismatch: bool) {
    report(path);
    let mut report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut argument = report["argument"].clone();
    let plans: Vec<_> = argument["evidence_plan"].as_array().unwrap().iter().enumerate().map(|(index, need)| {
        if index == 0 {
            json!({"need_id":need["id"],"tool_id":"http_json_get","arguments":[{"name":"url","value":"https://example.test/observations"}],"unresolved_arguments":[]})
        } else {
            json!({"need_id":need["id"],"tool_id":"custom","arguments":[],"unresolved_arguments":["adapter"]})
        }
    }).collect();
    if mismatch {
        argument["adjudication"] = json!("A different retained argument.");
    }
    let content = json!({"argument":argument,"acquisition_plan":plans}).to_string();
    report["transcript"] = json!([{
        "role":"adjudicator", "prompt":{"role":"adjudicator","system":"fixture","user":"{}","schema":{}},
        "response":{"content":content,"model":"fixture","input_tokens":null,"output_tokens":null,"request_id":null,"service_tier":null,"reasoning_effort":null},
        "response_content_bytes":content.len(),"response_content_sha256":"fixture","response_content_omitted":false,"elapsed_ms":0
    }]);
    std::fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
}

#[test]
fn export_uses_the_final_retained_plan_and_rejects_a_different_argument() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("planned.json");
    let output = directory.path().join("planned.eal");
    planned_report(&input, true);
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(!directory.path().join("planned.tools.toml").exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("different canonical argument"));
    planned_report(&input, false);
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(&output).unwrap();
    let tools = std::fs::read_to_string(directory.path().join("planned.tools.toml")).unwrap();
    assert!(source.contains("https://example.test/observations"));
    assert!(tools.contains("http_json_get"));
    assert!(tools.contains("kind = \"command\""));
}

#[test]
fn null_optional_graph_metadata_preserves_flat_collectors_in_export_and_prepare() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("planned.json");
    planned_report(&input, false);
    let mut report: Value = serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
    report["evidence_planning"] = Value::Null;
    std::fs::write(&input, serde_json::to_vec(&report).unwrap()).unwrap();
    let bundle = directory.path().join("argument.json");
    let result = cli()
        .args(["eal", "export", "--format", "json", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let exported: Value = serde_json::from_slice(&std::fs::read(bundle).unwrap()).unwrap();
    assert_eq!(
        exported["acquisition_plan"]["entries"][0]["tool_id"],
        "http_json_get"
    );
    let prepared = directory.path().join("prepared");
    let result = cli()
        .args(["eal", "prepare", "--input"])
        .arg(&input)
        .arg("--output-dir")
        .arg(&prepared)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let prepared: Value =
        serde_json::from_slice(&std::fs::read(prepared.join("argument.bundle.json")).unwrap())
            .unwrap();
    assert_eq!(
        prepared["acquisition_plan"]["entries"][0]["tool_id"],
        "http_json_get"
    );
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
    let tools = directory.path().join("case.tools.toml");
    let registry = std::fs::read_to_string(&tools).unwrap();
    assert!(registry.contains("engarg_review_contract"));
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
        assert_eq!(
            std::fs::metadata(&tools).unwrap().permissions().mode() & 0o777,
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
    assert_eq!(
        std::fs::read_to_string(directory.path().join("report.eal")).unwrap(),
        exported["source"]
    );
    assert!(directory.path().join("report.tools.toml").is_file());
}

#[test]
fn eal_stdout_still_creates_the_tools_companion_beside_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("report.json");
    report(&input);
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.starts_with(b"language \"EAL/3\""));
    assert_eq!(
        std::fs::read(directory.path().join("report.eal")).unwrap(),
        result.stdout
    );
    assert!(directory.path().join("report.tools.toml").is_file());
}

#[test]
fn companions_are_preflighted_together_and_allow_explicit_destinations() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("report.json");
    let output = directory.path().join("bundle.json");
    let source = directory.path().join("custom.eal");
    let tools = directory.path().join("custom.toml");
    report(&input);
    std::fs::write(&tools, "keep").unwrap();
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .args(["--format", "json", "--output"])
        .arg(&output)
        .arg("--source-output")
        .arg(&source)
        .arg("--tools-output")
        .arg(&tools)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(!source.exists());
    assert_eq!(std::fs::read_to_string(&tools).unwrap(), "keep");
    std::fs::remove_file(&tools).unwrap();
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .args(["--format", "json", "--output"])
        .arg(&output)
        .arg("--source-output")
        .arg(&source)
        .arg("--tools-output")
        .arg(&tools)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output.is_file());
    assert!(source.is_file());
    assert!(tools.is_file());
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
    fn collection_requires_a_registry_and_preserves_the_exact_context() {
        let directory = tempfile::tempdir().unwrap();
        let (input, bundle) = bundle(directory.path());
        let (executable, capture) = host(directory.path(), "collect", success("collect_claim"), 0);
        let result = cli()
            .args(["eal", "collect", "--input"])
            .arg(&input)
            .arg("--eal-host")
            .arg(&executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!capture.exists());
        let result = cli()
            .args(["eal", "collect", "--input"])
            .arg(&input)
            .arg("--tools")
            .arg(directory.path().join("bundle.tools.toml"))
            .arg("--eal-host")
            .arg(&executable)
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
            json!({"operation":"collect_claim","claim":bundle["claim_names"]["current_claim"],"source":bundle["source"],"context":bundle["context"]})
        );
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
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains("EAL validate failed"));
        assert!(diagnostic.contains(output.canonicalize().unwrap().to_str().unwrap()));
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
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains("EAL describe failed"));
        assert!(diagnostic.contains("written to stdout"));
        assert!(!diagnostic.contains("incompatible host/server"));
    }

    #[test]
    fn legacy_method_loader_error_preserves_response_and_identifies_the_rust_pair() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("legacy-response.json");
        let response = json!({
            "is_error": true,
            "error": "methods must have the form package.module:function"
        });
        let (executable, _) = host(directory.path(), "legacy", response.clone(), 1);
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
        assert!(result.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&output).unwrap()).unwrap(),
            response
        );
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains("EAL describe failed"));
        assert!(diagnostic.contains("incompatible host/server method loader"));
        assert!(diagnostic.contains("--methods builtin"));
        assert!(diagnostic.contains(output.canonicalize().unwrap().to_str().unwrap()));
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
    fn collector_credentials_require_explicit_supported_environment_names() {
        let directory = tempfile::tempdir().unwrap();
        let tools = directory.path().join("tools.toml");
        let marker = directory.path().join("credential-received");
        std::fs::write(&tools, "[tools.source]\nkind = \"command\"\nversion = \"1\"\nargv = [\"/operator/collector\"]\ninherit_env = [\"GH_TOKEN\"]\n").unwrap();
        let (executable, _) = host(directory.path(), "credential", success("describe"), 0);
        let script = format!(
            "#!/bin/sh\nif [ \"$GH_TOKEN\" = 'synthetic-collector-credential' ]; then touch {}; fi\ncat >/dev/null\nprintf '%s\\n' {}\n",
            quote(marker.to_str().unwrap()),
            quote(&success("describe").to_string())
        );
        std::fs::write(&executable, script).unwrap();
        let result = cli()
            .args(["eal", "describe", "--tools"])
            .arg(&tools)
            .arg("--eal-host")
            .arg(&executable)
            .arg("--workspace")
            .arg(directory.path())
            .env("GH_TOKEN", "synthetic-collector-credential")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(!marker.exists());
        let result = cli()
            .args(["eal", "describe", "--tools"])
            .arg(&tools)
            .args(["--collector-env", "GH_TOKEN"])
            .env_remove("GH_TOKEN")
            .arg("--eal-host")
            .arg(&executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!marker.exists());
        for unsupported in ["EAL_MCP_REGISTRY", "LD_PRELOAD", "PATH", "ENGARG_API_KEY"] {
            let result = cli()
                .args(["eal", "describe", "--tools"])
                .arg(&tools)
                .args(["--collector-env", unsupported])
                .arg("--eal-host")
                .arg(&executable)
                .arg("--workspace")
                .arg(directory.path())
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(!marker.exists());
        }
        let result = cli()
            .args(["eal", "describe", "--tools"])
            .arg(&tools)
            .args(["--collector-env", "GH_TOKEN"])
            .env("GH_TOKEN", "synthetic-collector-credential")
            .arg("--eal-host")
            .arg(&executable)
            .arg("--workspace")
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(marker.exists());
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains("synthetic-collector-credential")
        );
        assert!(
            !String::from_utf8_lossy(&result.stderr).contains("synthetic-collector-credential")
        );
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

#[test]
fn operator_native_override_reuses_saved_argument_without_a_model_call() {
    let directory = tempfile::tempdir().unwrap();
    let report_path = directory.path().join("report.json");
    planned_report(&report_path, false);
    let before = std::fs::read(&report_path).unwrap();
    let report: Value = serde_json::from_slice(&before).unwrap();
    let need = report["argument"]["evidence_plan"][0]["id"].clone();
    let plan = directory.path().join("bindings.json");
    std::fs::write(&plan, json!([{"need_id":need,"tool_id":"kubernetes","arguments":[{"name":"kind","value":"Application"},{"name":"name","value":"pilot-sync"}],"unresolved_arguments":["inventory","resource","namespace"]}]).to_string()).unwrap();
    let catalogue = directory.path().join("native.json");
    std::fs::write(
        &catalogue,
        include_bytes!("../contracts/collectors/native.json"),
    )
    .unwrap();
    let source = directory.path().join("case.eal");
    let manifest = directory.path().join("case.json");
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&report_path)
        .arg("--collector-catalogue")
        .arg(&catalogue)
        .arg("--acquisition-plan")
        .arg(&plan)
        .arg("--output")
        .arg(&source)
        .arg("--manifest")
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle: Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    assert_eq!(bundle["original_case"], report["argument"]);
    assert_eq!(std::fs::read(report_path).unwrap(), before);
    let tools = std::fs::read_to_string(directory.path().join("case.tools.toml")).unwrap();
    let parsed: engarg::collector_catalogue::ToolsRegistry = toml::from_str(&tools).unwrap();
    assert!(parsed.tools.values().any(|binding| {
        binding
            .argv
            .starts_with(&["eal-collector".to_owned(), "kubernetes".to_owned()])
    }));
    assert!(!tools.contains("kind = \"json_file\""));
    assert!(
        std::fs::read_to_string(source)
            .unwrap()
            .contains("\"namespace\":null")
    );
}
