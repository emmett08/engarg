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
    let request =
        json!({"operation":"validate", "source":"argument Test {}", "test_number": number});
    assert_eq!(invoke(&config, &request).await.unwrap(), expected);
    let received: Value = serde_json::from_slice(&fs::read(input).unwrap()).unwrap();
    assert_eq!(received, request);
    assert_eq!(received["test_number"].to_string(), integer);
    let actual_args = fs::read_to_string(args).unwrap();
    let actual_args: Vec<_> = actual_args.lines().collect();
    let canonical_workspace = config.workspace.canonicalize().unwrap();
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
        ]
    );
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
async fn redacts_the_named_bearer_token_from_host_diagnostics() {
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
    let result = invoke(&config, &json!({"operation":"validate"}))
        .await
        .unwrap();
    assert_eq!(result["error"], "token was [REDACTED]");
    assert!(!result.to_string().contains(&token));
}
