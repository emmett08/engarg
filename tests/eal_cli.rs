//! Deterministic CLI contracts. Host calls use an executable test double, not MCP
//! or language-model execution. Real-host integration is tested separately.

use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
}

#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn report(path: &Path) {
    let (request, argument, support) = executable_case::fixture();
    let value = json!({
        "schema_version":engarg::report::RUN_REPORT_SCHEMA,
        "execution_mode":"offline_synthetic_fixture",
        "input_sha256":eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        "mode":"single", "request":request, "argument":argument,
        "transcript":[], "support_elaboration":support, "limitations":[],
    });
    engarg::report::read_direct(&value).unwrap();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn bindings(root: &Path) -> std::path::PathBuf {
    std::fs::write(root.join("measurement.json"), b"{}").unwrap();
    let tools = root.join("tools.toml");
    std::fs::write(&tools, "[tools.premise_source]\nkind='json_file'\nversion='1'\npath='measurement.json'\n[tools.applicability_source]\nkind='json_file'\nversion='1'\npath='measurement.json'\n").unwrap();
    tools
}

#[test]
fn export_builds_one_executable_package_without_provider_calls() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("report.json");
    report(&input);
    let before = std::fs::read(&input).unwrap();
    let tools = bindings(root.path());
    let package = root.path().join("package");
    let result = cli()
        .env_remove("ENGARG_API_KEY")
        .env_remove("ENGARG_MODEL")
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--tools")
        .arg(&tools)
        .arg("--output")
        .arg(&package)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["language"], "EAL/4");
    assert!(receipt.get("profile").is_none());
    assert_eq!(receipt["model_calls"], 0);
    assert_eq!(std::fs::read(&input).unwrap(), before);
    let source = std::fs::read_to_string(package.join("case.eal")).unwrap();
    assert!(source.starts_with("language \"EAL/4\""));
    assert!(!source.contains("engarg_support_graph"));
    assert!(package.join("tools.toml").is_file());
    assert!(package.join("measurement.json").is_file());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(package.join("build.json")).unwrap())
            .unwrap(),
        receipt
    );
}

#[test]
fn existing_output_and_unbound_operations_fail_without_partial_publication() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("report.json");
    report(&input);
    let tools = bindings(root.path());
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("keep"), b"original").unwrap();
    let run = || {
        cli()
            .args(["eal", "export", "--input"])
            .arg(&input)
            .arg("--tools")
            .arg(&tools)
            .arg("--output")
            .arg(&package)
            .output()
            .unwrap()
    };
    assert!(!run().status.success());
    assert_eq!(std::fs::read(package.join("keep")).unwrap(), b"original");
    std::fs::remove_file(package.join("keep")).unwrap();
    std::fs::remove_dir(&package).unwrap();
    std::fs::write(&tools, "[tools]\n").unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Missing direct tool binding"));
    assert!(!package.exists());
}

#[test]
fn source_export_requires_current_authored_support() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("report.json");
    report(&input);
    let mut value: Value = serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("support_elaboration");
    std::fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&input)
        .arg("--tools")
        .arg(bindings(root.path()))
        .arg("--output")
        .arg(root.path().join("package"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("support_elaboration"));
    assert!(!root.path().join("package").exists());
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

    fn source(directory: &Path) -> (std::path::PathBuf, Value) {
        let source = "language \"EAL/4\"\nclaim current_claim {\n  statement \"Synthetic transport fixture\"\n}\n";
        let input = directory.join("case.eal");
        std::fs::write(&input, source).unwrap();
        std::fs::write(directory.join("tools.toml"), "[tools]\n").unwrap();
        (
            input,
            json!({"source":source,"claim_names":{"current_claim":"current_claim"},"context":{}}),
        )
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
    fn source_plan_preserves_claim_and_assessment_uses_explicit_collection() {
        let directory = tempfile::tempdir().unwrap();
        let (input, bundle) = source(directory.path());
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
        let (input, bundle) = source(directory.path());
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
            .arg(directory.path().join("tools.toml"))
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
    fn package_manifest_context_is_used_by_collection_and_assessment() {
        let directory = tempfile::tempdir().unwrap();
        let (input, _) = source(directory.path());
        let tools = directory.path().join("tools.toml");
        std::fs::write(
            &tools,
            "[context]\ncase_digest='retained-case'\nrevision=7\n[tools]\n",
        )
        .unwrap();
        for (verb, operation) in [("collect", "collect_claim"), ("assess", "reason")] {
            let (executable, capture) = host(directory.path(), verb, success(operation), 0);
            let mut command = cli();
            command
                .args(["eal", verb, "--input"])
                .arg(&input)
                .arg("--tools")
                .arg(&tools)
                .arg("--eal-host")
                .arg(executable)
                .arg("--workspace")
                .arg(directory.path());
            if verb == "assess" {
                command.args([
                    "--now",
                    "2026-10-23T18:00:00Z",
                    "--collection-id",
                    "retained-collection",
                ]);
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let request: Value = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
            assert_eq!(
                request["context"],
                json!({"case_digest":"retained-case","revision":7})
            );
        }
    }

    #[test]
    fn retired_json_bundle_is_rejected_before_host_execution() {
        let directory = tempfile::tempdir().unwrap();
        let (input, mut bundle) = source(directory.path());
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
            String::from_utf8_lossy(&result.stderr).contains("JSON export bundles are retired")
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
