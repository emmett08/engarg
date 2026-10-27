//! Exercise the process boundary without requiring an installed EAL server.
#![cfg(unix)]

use engarg::{
    Error,
    eal_host::{HostConfig, invoke},
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn mock(directory: &TempDir, script: &str) -> HostConfig {
    let executable = directory.path().join("mock-host");
    fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    HostConfig {
        executable,
        workspace: directory.path().to_owned(),
        database: None,
        registry: None,
        collector_env: Vec::new(),
        public_collector_env: Vec::new(),
        url: None,
        token_env: None,
        timeout: Duration::from_secs(2),
    }
}

fn response(result: Value) -> Value {
    json!({
        "operation": "validate", "protocol_version": "2025-11-25",
        "is_error": false, "result": result, "content": []
    })
}

fn emit(value: &Value, status: u8) -> String {
    format!(
        "cat >/dev/null\nprintf '%s\\n' {}\nexit {status}",
        shell_quote(&value.to_string())
    )
}

fn quoted_path(path: &Path) -> String {
    shell_quote(path.to_str().unwrap())
}

#[tokio::test]
async fn preserves_exact_json_and_passes_explicit_server_configuration() {
    let directory = TempDir::new().unwrap();
    let args = directory.path().join("args");
    let input = directory.path().join("input");
    let integer = "12345678901234567890123456789012345678901234567890";
    let number: Value = serde_json::from_str(integer).unwrap();
    let expected = response(json!({"number": number}));
    let script = format!(
        "printf '%s\\n' \"$@\" > {}\ncat > {}\nprintf '%s\\n' {}",
        quoted_path(&args),
        quoted_path(&input),
        shell_quote(&expected.to_string())
    );
    let mut config = mock(&directory, &script);
    let database = directory.path().join("assessment.sqlite");
    config.database = Some(database.clone());
    let registry_file = directory.path().join("tools.toml");
    fs::write(&registry_file, "").unwrap();
    let registry = directory.path().join("tools-link.toml");
    std::os::unix::fs::symlink(&registry_file, &registry).unwrap();
    config.registry = Some(registry.clone());
    let request =
        json!({"operation":"validate", "source":"argument Test {}", "test_number": number});
    assert_eq!(invoke(&config, &request).await.unwrap(), expected);
    let received: Value = serde_json::from_slice(&fs::read(input).unwrap()).unwrap();
    assert_eq!(received, request);
    assert_eq!(received["test_number"].to_string(), integer);
    let actual_args = fs::read_to_string(args).unwrap();
    let mut actual_args: Vec<_> = actual_args.lines().collect();
    let remaining: f64 = actual_args[7].parse().unwrap();
    assert!(remaining > 0.0 && remaining <= 2.0);
    actual_args[7] = "2";
    let canonical_workspace = config.workspace.canonicalize().unwrap();
    let canonical_registry = registry.canonicalize().unwrap();
    assert_eq!(
        actual_args,
        vec![
            "--workspace",
            canonical_workspace.to_str().unwrap(),
            "--exposure",
            "operator",
            "--methods",
            "builtin",
            "--timeout",
            "2",
            "--database",
            database.to_str().unwrap(),
            "--registry",
            canonical_registry.to_str().unwrap(),
        ]
    );
}

#[tokio::test]
async fn a_local_registry_is_validated_before_starting_the_host() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("started");
    let script = format!("touch {}", quoted_path(&marker));
    let mut config = mock(&directory, &script);
    config.registry = Some(directory.path().join("missing.toml"));
    assert!(
        invoke(&config, &json!({"operation":"describe"}))
            .await
            .unwrap_err()
            .to_string()
            .contains("registry")
    );
    let registry = directory.path().join("tools.toml");
    fs::write(&registry, "").unwrap();
    config.registry = Some(registry);
    config.url = Some("https://example.test/mcp".into());
    assert!(
        invoke(&config, &json!({"operation":"describe"}))
            .await
            .unwrap_err()
            .to_string()
            .contains("remote")
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn retains_both_tool_and_host_error_envelopes() {
    for status in [1, 2] {
        let directory = TempDir::new().unwrap();
        let expected = json!({"is_error":true,"error":"bounded diagnostic"});
        let config = mock(&directory, &emit(&expected, status));
        assert_eq!(
            invoke(&config, &json!({"operation":"validate"}))
                .await
                .unwrap(),
            expected
        );
    }
    let directory = TempDir::new().unwrap();
    let mut expected = response(json!({"diagnostics":["tool diagnostic"]}));
    expected["is_error"] = json!(true);
    let config = mock(&directory, &emit(&expected, 1));
    assert_eq!(
        invoke(&config, &json!({"operation":"validate"}))
            .await
            .unwrap(),
        expected
    );
}

#[tokio::test]
async fn rejects_invalid_json_without_disclosing_child_output() {
    let directory = TempDir::new().unwrap();
    let secret = "fake-secret-must-not-appear";
    let script = format!(
        "cat >/dev/null\nprintf '%s' {0}\nprintf '%s' {0} >&2",
        shell_quote(secret)
    );
    let config = mock(&directory, &script);
    let error = invoke(&config, &json!({"operation":"validate"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid JSON"));
    assert!(!error.contains(secret));
}

#[tokio::test]
async fn rejects_inconsistent_exit_status_or_response_shape() {
    let cases = [
        (response(json!({})), 2),
        (json!({"is_error":true,"error":"failure"}), 0),
        (json!({"is_error":false}), 0),
        (json!({"is_error":"false"}), 0),
    ];
    for (envelope, status) in cases {
        let directory = TempDir::new().unwrap();
        let config = mock(&directory, &emit(&envelope, status));
        assert!(matches!(
            invoke(&config, &json!({"operation":"validate"})).await,
            Err(Error::Provider(_))
        ));
    }
}

#[tokio::test]
async fn times_out_and_reaps_the_host_process() {
    let directory = TempDir::new().unwrap();
    let pid_file = directory.path().join("pid");
    let script = format!(
        "printf '%s' \"$$\" > {}\nwhile :; do :; done",
        quoted_path(&pid_file)
    );
    let mut config = mock(&directory, &script);
    config.timeout = Duration::from_millis(30);
    let start = Instant::now();
    let error = invoke(&config, &json!({"operation":"validate"}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("deadline"));
    assert!(start.elapsed() < Duration::from_secs(4));
    let pid = fs::read_to_string(pid_file).unwrap();
    assert!(
        !Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}

#[tokio::test]
async fn bounds_input_before_starting_the_host() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("started");
    let config = mock(
        &directory,
        &format!("printf started > {}", quoted_path(&marker)),
    );
    let request = json!({"operation":"validate","source":"x".repeat(2 * 1024 * 1024)});
    assert!(matches!(
        invoke(&config, &request).await,
        Err(Error::Budget(_))
    ));
    assert!(!marker.exists());
}

#[tokio::test]
async fn bounds_both_output_streams() {
    for (stream, iterations) in [("stdout", 17_000), ("stderr", 100)] {
        let directory = TempDir::new().unwrap();
        let redirect = if stream == "stderr" { ">&2" } else { "" };
        let chunk = "x".repeat(1024);
        let script = format!(
            "cat >/dev/null\ni=0\nwhile [ \"$i\" -lt {iterations} ]; do printf '%s' '{chunk}' {redirect}; i=$((i + 1)); done"
        );
        let config = mock(&directory, &script);
        let error = invoke(&config, &json!({"operation":"validate"}))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Budget(_)), "{error}");
        assert!(error.to_string().contains(stream));
    }
}

#[tokio::test]
async fn rejects_unsafe_urls_and_invalid_deadlines_before_spawning() {
    let directory = TempDir::new().unwrap();
    let mut config = mock(&directory, "exit 0");
    for url in [
        "http://example.com/mcp",
        "https://user:secret@example.com/mcp",
        "https://example.com/mcp?key=secret",
        "file:///tmp/mcp",
    ] {
        config.url = Some(url.to_owned());
        let error = invoke(&config, &json!({"operation":"validate"}))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert!(!error.to_string().contains("secret"));
    }
    config.url = None;
    config.timeout = Duration::ZERO;
    assert!(matches!(
        invoke(&config, &json!({"operation":"validate"})).await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn refuses_the_named_bearer_token_in_any_host_response() {
    // Reuse a process-owned variable without modifying the global environment.
    let token = std::env::var("PATH").unwrap();
    if token.is_empty() || !token.bytes().all(|b| (33..=126).contains(&b)) {
        return;
    }
    let directory = TempDir::new().unwrap();
    let envelope = json!({"is_error":true,"error":format!("token was {token}")});
    let mut config = mock(&directory, &emit(&envelope, 2));
    config.url = Some("http://127.0.0.1:9123/mcp".to_owned());
    config.token_env = Some("PATH".to_owned());
    let error = invoke(&config, &json!({"operation":"validate"}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("EAL_HOST_CREDENTIAL_LEAK"));
    assert!(!error.to_string().contains(&token));
}

fn collector_cli(directory: &TempDir, host: &Path, registry: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .args(["eal", "describe", "--workspace"])
        .arg(directory.path())
        .arg("--eal-host")
        .arg(host)
        .arg("--tools")
        .arg(registry)
        .args(["--collector-env", "LAB_API_TOKEN"]);
    command
}

#[test]
fn operator_registry_forwards_credentials_but_refuses_them_in_semantic_output() {
    let directory = TempDir::new().unwrap();
    let secret = "synthetic-laboratory-api-credential";
    let mut envelope = response(json!({"diagnostic":format!("collector received {secret}")}));
    envelope["operation"] = json!("describe");
    let script = format!(
        "[ \"$LAB_API_TOKEN\" = {} ] || exit 42\n[ -z \"$ENGARG_UNREQUESTED_SECRET\" ] || exit 43\n{}",
        shell_quote(secret),
        emit(&envelope, 0)
    );
    let config = mock(&directory, &script);
    let registry = directory.path().join("tools.toml");
    fs::write(
        &registry,
        "[tools.lab]\nkind = 'command'\nversion = '1'\nargv = ['/operator/lab-collector']\ninherit_env = ['LAB_API_TOKEN']\n",
    )
    .unwrap();
    let output = collector_cli(&directory, &config.executable, &registry)
        .env("LAB_API_TOKEN", secret)
        .env(
            "ENGARG_UNREQUESTED_SECRET",
            "unrequested-ambient-credential",
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("EAL_HOST_CREDENTIAL_LEAK"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
}

#[test]
fn a_credential_matching_the_redaction_marker_cannot_reappear_in_output() {
    let directory = TempDir::new().unwrap();
    let mut envelope = response(json!({"diagnostic":"REDACTED"}));
    envelope["operation"] = json!("describe");
    let config = mock(&directory, &emit(&envelope, 0));
    let registry = directory.path().join("tools.toml");
    fs::write(
        &registry,
        "[tools.lab]\nkind = 'command'\nversion = '1'\nargv = ['/operator/lab-collector']\ninherit_env = ['LAB_API_TOKEN']\n",
    )
    .unwrap();
    let output = collector_cli(&directory, &config.executable, &registry)
        .env("LAB_API_TOKEN", "REDACTED")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("REDACTED"));
}

#[test]
fn direct_http_and_mcp_credentials_require_manifest_authority() {
    let directory = TempDir::new().unwrap();
    let mut envelope = response(json!({"ready":true}));
    envelope["operation"] = json!("describe");
    let config = mock(
        &directory,
        &format!(
            "[ -n \"$LAB_API_TOKEN\" ] || exit 42\n{}",
            emit(&envelope, 0)
        ),
    );
    let registry = directory.path().join("tools.toml");
    for mapping in [
        "kind='mcp_http'\nversion='1'\n[tools.lab.direct]\ntool='measure'\nurl='https://example.invalid/mcp'\ntoken_env='LAB_API_TOKEN'\n",
        "kind='http_get'\nversion='1'\n[tools.lab.direct]\nurl='https://example.invalid/result'\n[tools.lab.direct.headers_env]\nX-Api-Key='LAB_API_TOKEN'\n",
    ] {
        fs::write(&registry, format!("[tools.lab]\n{mapping}")).unwrap();
        let output = collector_cli(&directory, &config.executable, &registry)
            .env("LAB_API_TOKEN", "synthetic-direct-source-credential")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("synthetic-direct-source-credential")
        );
    }
    fs::write(&registry, "[tools.lab]\nkind='http_get'\nversion='1'\n[tools.lab.direct]\nurl='https://example.invalid/result'\n").unwrap();
    let output = collector_cli(&directory, &config.executable, &registry)
        .env("LAB_API_TOKEN", "synthetic-direct-source-credential")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn undeclared_or_missing_collector_credentials_fail_before_host_execution() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("started");
    let config = mock(&directory, &format!("touch {}", quoted_path(&marker)));
    let registry = directory.path().join("tools.toml");
    for (inherited, supplied) in [("[]", true), ("['LAB_API_TOKEN']", false)] {
        fs::write(
            &registry,
            format!("[tools.lab]\nkind = 'command'\nversion = '1'\nargv = ['/operator/lab-collector']\ninherit_env = {inherited}\n"),
        )
        .unwrap();
        let mut command = collector_cli(&directory, &config.executable, &registry);
        if supplied {
            command.env("LAB_API_TOKEN", "synthetic-laboratory-api-credential");
        } else {
            command.env_remove("LAB_API_TOKEN");
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(!marker.exists());
        assert!(output.stdout.is_empty());
        assert!(
            !String::from_utf8_lossy(&output.stderr)
                .contains("synthetic-laboratory-api-credential")
        );
    }
}

#[test]
fn malformed_or_oversized_environment_registries_fail_before_host_execution() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("started");
    let config = mock(&directory, &format!("touch {}", quoted_path(&marker)));
    let registry = directory.path().join("tools.toml");
    for contents in [
        "[tools.lab]\nkind = 'command'\ninherit_env = ['LAB_API_TOKEN', 'LAB_API_TOKEN']\n"
            .to_owned(),
        "[tools.lab]\nkind = 'command'\ninherit_env = [123]\n".to_owned(),
        "[tools.lab]\nkind = 'json_file'\ninherit_env = ['LAB_API_TOKEN']\n".to_owned(),
        "#".repeat(1024 * 1024 + 1),
    ] {
        fs::write(&registry, contents).unwrap();
        let output = collector_cli(&directory, &config.executable, &registry)
            .env("LAB_API_TOKEN", "synthetic-laboratory-api-credential")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!marker.exists());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn operator_public_configuration_preserves_assessment_state_and_sensitive_configuration_refuses_leakage()
 {
    let directory = TempDir::new().unwrap();
    let registry = directory.path().join("public.tools.toml");
    fs::write(&registry,"[tools.lab]\nkind='command'\nversion='1'\nargv=['/operator/lab']\ninherit_env=['LAB_MODE']\n").unwrap();
    let mut envelope = response(json!({"status":"supported"}));
    envelope["operation"] = json!("describe");
    let host = mock(&directory, &emit(&envelope, 0));
    for public in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", "describe", "--workspace"])
            .arg(directory.path())
            .arg("--eal-host")
            .arg(&host.executable)
            .arg("--tools")
            .arg(&registry)
            .args(["--collector-env", "LAB_MODE"])
            .env("LAB_MODE", "supported");
        if public {
            command.args(["--public-collector-env", "LAB_MODE"]);
        }
        let output = command.output().unwrap();
        assert_eq!(
            output.status.success(),
            public,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if public {
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["result"]["status"], "supported");
        } else {
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("EAL_HOST_CREDENTIAL_LEAK"));
        }
    }
}

#[test]
fn sensitive_values_in_nested_keys_values_and_escaped_text_are_refused_without_rewriting() {
    let directory = TempDir::new().unwrap();
    let registry = directory.path().join("sensitive.tools.toml");
    fs::write(&registry,"[tools.lab]\nkind='command'\nversion='1'\nargv=['/operator/lab']\ninherit_env=['LAB_API_TOKEN']\n").unwrap();
    let secret = "synthetic-sensitive-proof";
    let encoded = format!("{{\"evidence\":\"\\u0073{}\"}}", &secret[1..]);
    let encoded_key = format!("{{\"\\u0073{}\":true}}", &secret[1..]);
    for result in [
        json!({"status":secret}),
        json!({"nested":[{"evidence":secret}]}),
        json!({(secret):"public"}),
        json!({"evidence":format!("\"{secret}\"\n")}),
        json!({"encoded_evidence": encoded}),
        json!({(encoded_key): "public"}),
        json!({"encoded_evidence": serde_json::to_string(&json!({"nested": encoded})).unwrap()}),
    ] {
        let mut envelope = response(result);
        envelope["operation"] = json!("describe");
        let config = mock(&directory, &emit(&envelope, 0));
        let output = collector_cli(&directory, &config.executable, &registry)
            .env("LAB_API_TOKEN", secret)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("EAL_HOST_CREDENTIAL_LEAK"));
        assert!(!error.contains(secret));
    }
}

#[test]
fn response_inspection_exhaustion_and_ambiguous_encoded_json_fail_without_output() {
    let directory = TempDir::new().unwrap();
    let registry = directory.path().join("sensitive.tools.toml");
    fs::write(&registry,"[tools.lab]\nkind='command'\nversion='1'\nargv=['/operator/lab']\ninherit_env=['LAB_API_TOKEN']\n").unwrap();
    for result in [
        json!({"evidence":"{\"value\":\"public\",\"value\":\"\\u0073ecret\"}"}),
        json!({"values": vec![0; 100_001]}),
    ] {
        let mut envelope = response(result);
        envelope["operation"] = json!("describe");
        let config = mock(&directory, &emit(&envelope, 0));
        let output = collector_cli(&directory, &config.executable, &registry)
            .env("LAB_API_TOKEN", "secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("EAL_HOST_CREDENTIAL_INSPECTION"));
        assert!(!error.contains("\\u0073ecret"));
    }
}

#[test]
fn unrelated_json_like_prose_remains_exact_with_sensitive_forwarded_configuration() {
    let directory = TempDir::new().unwrap();
    let registry = directory.path().join("sensitive.tools.toml");
    fs::write(&registry,"[tools.lab]\nkind='command'\nversion='1'\nargv=['/operator/lab']\ninherit_env=['LAB_API_TOKEN']\n").unwrap();
    let mut expected = response(json!({
        "[Results]": "[Results] calibration passed",
        "{draft}": "{draft} awaits measurement",
        "quotation": "\"An unfinished quotation",
        "encoded": "{\"public\":true}"
    }));
    expected["operation"] = json!("describe");
    let config = mock(&directory, &emit(&expected, 0));
    let output = collector_cli(&directory, &config.executable, &registry)
        .env("LAB_API_TOKEN", "synthetic-sensitive-proof")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
}

#[tokio::test]
async fn registered_source_scope_is_explicit_and_cannot_be_rebound() {
    let directory = TempDir::new().unwrap();
    let args = directory.path().join("registered-args");
    let envelope = json!({"operation":"assess_known","protocol_version":"2025-11-25","is_error":false,"result":{"entry_id":"ticket_fixture"},"content":[]});
    let script = format!(
        "printf '%s\\n' \"$@\" > {}\n{}",
        quoted_path(&args),
        emit(&envelope, 0)
    );
    let config = mock(&directory, &script);
    let request =
        json!({"operation":"assess_known","entry_id":"ticket_fixture","claim":"current_claim"});
    let wrong = engarg::eal_host::invoke_registered(&config, &request, "another_ticket").await;
    assert!(matches!(wrong, Err(Error::InvalidInput(_))));
    assert!(!args.exists());
    let response = engarg::eal_host::invoke_registered(&config, &request, "ticket_fixture")
        .await
        .unwrap();
    assert_eq!(response["result"]["entry_id"], "ticket_fixture");
    let args = fs::read_to_string(args).unwrap();
    assert!(args.contains("registered"));
    assert!(args.contains("--known-entry"));
    assert!(args.contains("ticket_fixture"));
}
