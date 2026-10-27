//! Runtime mismatch is rejected before authoring can contact its provider.
use engarg::runtime_compatibility::{TargetKind, admit_description, embedded, validate_receipt};
use serde_json::{Value, json};

fn description() -> Value {
    let registry = eal_core::methods::default_registry();
    json!({"source_syntax":eal_core::LANGUAGE,
        "execution_features":["evidence_bindings","pure_derivations","direct_tool_bindings","execute","interface_preparation"],
        "method_registry_fingerprint":registry.fingerprint(),
        "typed_bindings":eal_core::propositions::describe_bindings(registry),
        "implementation_version":"fixture"})
}

#[test]
fn requires_language_features_and_exact_method_contracts_but_allows_new_methods() {
    let mut value = description();
    value["typed_bindings"]["methods"]["extra/1"] = json!({"identifier":"extra/1"});
    assert_eq!(
        admit_description(&value).unwrap().kind,
        TargetKind::SelectedHost
    );
    for changed in [
        {
            let mut v = value.clone();
            v["source_syntax"] = json!("EAL/3");
            v
        },
        {
            let mut v = value.clone();
            v["execution_features"] = json!(["execute"]);
            v
        },
        {
            let mut v = value.clone();
            v["typed_bindings"]["methods"]["deductive/1"]["implementation_version"] =
                json!("different");
            v
        },
    ] {
        assert!(
            admit_description(&changed)
                .unwrap_err()
                .to_string()
                .contains("EAL_TARGET_INCOMPATIBLE")
        );
    }
    let portable = embedded().unwrap();
    assert_eq!(portable.kind, TargetKind::EmbeddedContract);
    assert!(portable.executable_sha256.is_none());
}

#[test]
fn retained_receipts_distinguish_embedded_contracts_from_observed_executables() {
    let embedded = embedded().unwrap();
    validate_receipt(&embedded).unwrap();
    let mut forged = embedded.clone();
    forged.executable_sha256 = Some("a".repeat(64));
    assert!(validate_receipt(&forged).is_err());
    let mut selected = admit_description(&description()).unwrap();
    assert!(validate_receipt(&selected).is_err());
    selected.executable_sha256 = Some("a".repeat(64));
    validate_receipt(&selected).unwrap();
    for changed in [
        {
            let mut value = selected.clone();
            value.features.push(value.features[0].clone());
            value
        },
        {
            let mut value = selected.clone();
            value.method_contracts_sha256 = "b".repeat(64);
            value
        },
        {
            let mut value = selected.clone();
            value.implementation_version = Some(" ".into());
            value
        },
    ] {
        assert!(validate_receipt(&changed).is_err());
    }
}

#[test]
#[cfg(unix)]
fn incompatible_selected_host_stops_before_any_model_http_request() {
    use std::{fs, net::TcpListener, os::unix::fs::PermissionsExt, process::Command};
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("old-host");
    let mut old = description();
    old["source_syntax"] = json!("EAL/3");
    let response = json!({"operation":"describe","protocol_version":"2025-11-25","is_error":false,"result":old,"content":[]});
    let quoted = format!("'{}'", response.to_string().replace('\'', "'\\''"));
    fs::write(
        &host,
        format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' {quoted}\n"),
    )
    .unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args([
            "analyse",
            "--claim",
            "The retained fixture has three records.",
            "--model",
            "fixture",
            "--eal-host",
        ])
        .arg(&host)
        .arg("--endpoint")
        .arg(format!(
            "http://{}/responses",
            listener.local_addr().unwrap()
        ))
        .args(["--request-timeout-secs", "1", "--http-retries", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("EAL_TARGET_INCOMPATIBLE"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
