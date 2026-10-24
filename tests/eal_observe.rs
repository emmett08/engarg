//! Collector observations preserve acquisition identity without judging adequacy.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
};

#[test]
fn http_collector_binds_raw_json_to_exact_eal_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0_u8; 4096];
        let count = socket.read(&mut bytes).unwrap();
        assert!(
            std::str::from_utf8(&bytes[..count])
                .unwrap()
                .starts_with("GET /observations?scope=pilot HTTP/1.1")
        );
        let body = r#"{"result":3,"large_integer":123456789012345678901234567890}"#;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let context = json!({"engarg_export_digest":"example","scope":"pilot"});
    let input = json!({"arguments":{"url":format!("http://{address}/observations?scope=pilot")},"need_id":"E1"});
    let request = json!({"evidence_id":"need_E1","environment":"engarg_case","tool":"engarg_http_json_get","tool_version":"1","input":input,"context":context});
    let mut child = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "observe", "--tool", "http_json_get"])
        .env_remove("ENGARG_COLLECTOR_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observation: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observation["value"]["collected"], true);
    assert_eq!(observation["value"]["observation"]["result"], 3);
    assert_eq!(
        observation["value"]["observation"]["large_integer"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(observation["context"], context);
    assert_eq!(
        observation["request"],
        json!({"tool":request["tool"],"tool_version":"1","input":input,"context":context})
    );
    assert!(observation["value"].get("accepted").is_none());
    server.join().unwrap();
}

fn collector(
    tool: &str,
    arguments: Value,
    search_path: Option<&std::path::Path>,
) -> std::process::Output {
    let request = json!({
        "evidence_id":"need_E1",
        "environment":"engarg_case",
        "tool":"collector",
        "tool_version":"1",
        "input":{"arguments":arguments},
        "context":{}
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .args(["eal", "observe", "--tool", tool])
        .env_remove("ENGARG_COLLECTOR_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = search_path {
        command.env("PATH", path);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn http_collector_rejects_duplicate_response_keys_without_emitting_an_observation() {
    for body in [
        r#"{"status":"failed","status":"healthy"}"#,
        r#"{"nested":{"status":"failed","st\u0061tus":"healthy"}}"#,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = [0_u8; 4096];
            assert!(socket.read(&mut bytes).unwrap() > 0);
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let output = collector(
            "http_json_get",
            json!({"url":format!("http://{address}/observations")}),
            None,
        );
        server.join().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("collector returned invalid JSON"), "{error}");
        assert!(!error.contains("healthy"), "{error}");
    }
}

#[test]
#[cfg(unix)]
fn command_collector_rejects_duplicate_response_keys_and_preserves_exact_integers() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("gh");
    for (body, valid) in [
        (
            r#"{"nested":{"status":"failed","status":"healthy"}}"#,
            false,
        ),
        (r#"{"large_integer":123456789012345678901234567890}"#, true),
    ] {
        std::fs::write(&executable, format!("#!/bin/sh\nprintf '%s' '{body}'\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = collector(
            "gh_api_get",
            json!({"endpoint":"repos/example/repository/commits"}),
            Some(dir.path()),
        );
        assert_eq!(
            output.status.success(),
            valid,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if valid {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                value["value"]["observation"]["large_integer"].to_string(),
                "123456789012345678901234567890"
            );
        } else {
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains("collector returned invalid JSON"), "{error}");
            assert!(!error.contains("healthy"), "{error}");
        }
    }
}
