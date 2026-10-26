//! Progress is visible while a provider is waiting and never enters JSON stdout.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[path = "common/live_output.rs"]
mod live_output;

fn cli(endpoint: &str, progress: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .args([
            "analyse",
            "--mode",
            "single",
            "--max-calls",
            "1",
            "--no-tool-planning",
            "--http-retries",
            "0",
            "--model",
            "private-model-name",
            "--api-key-env",
            "ENGARG_PROGRESS_TEST_KEY",
            "--format",
            "json",
            "--input",
        ])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/claim-only-request.json"
        ))
        .arg("--endpoint")
        .arg(endpoint)
        .env("ENGARG_PROGRESS_TEST_KEY", "private-test-credential");
    if progress {
        command.arg("--progress");
    }
    command
}

fn provider(
    status: &str,
    body: String,
) -> (
    String,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (arrived, arrival) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut bytes = [0; 8192];
            let count = socket.read(&mut bytes).unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&bytes[..count]);
            if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length:")?
                            .trim()
                            .parse::<usize>()
                            .ok()
                    })
                    .unwrap();
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        arrived.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
        socket.write_all(response.as_bytes()).unwrap();
    });
    (endpoint, arrival, release, handle)
}

#[test]
fn progress_precedes_provider_completion_and_stdout_remains_the_canonical_report() {
    let argument: Value = live_output::case(include_str!("../examples/claim-only-case.json"));
    let body = json!({"id":"private-provider-id","model":"private-model-name","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":argument.to_string()}]}]}).to_string();
    for progress in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let audit = directory.path().join("audit.jsonl");
        let (endpoint, arrival, release, server) = provider("200 OK", body.clone());
        let mut child = cli(&endpoint, progress)
            .arg("--audit")
            .arg(&audit)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut captured = String::new();
            for line in BufReader::new(stderr).lines() {
                let line = line.unwrap();
                if line.contains("call 1 single started") {
                    sender.send(()).unwrap();
                }
                captured.push_str(&line);
                captured.push('\n');
            }
            captured
        });
        arrival.recv_timeout(Duration::from_secs(10)).unwrap();
        if progress {
            lines.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        // The provider cannot complete before this explicit release. Status must
        // therefore have reached the terminal while the model call was pending.
        release.send(()).unwrap();
        let result = child.wait_with_output().unwrap();
        let stderr = reader.join().unwrap();
        server.join().unwrap();
        assert!(result.status.success(), "{stderr}");
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["argument"], argument);
        assert_eq!(report["transcript"].as_array().unwrap().len(), 1);
        let events: Vec<Value> = std::fs::read_to_string(audit)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            events
                .iter()
                .map(|event| event["event"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["start", "http_attempt", "completion", "success"]
        );
        assert_eq!(events[1]["record"]["status"], 200);
        assert_eq!(events[2]["record"], report["transcript"][0]);
        assert_eq!(
            events[2]["record"]["response"]["content"],
            argument.to_string()
        );
        if progress {
            assert!(stderr.contains("call 1 single response received"));
            assert!(stderr.contains("analysis completed"));
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
        assert!(!stderr.contains("private-"));
        assert!(!stderr.contains(report["request"]["claim"].as_str().unwrap()));
    }
}

#[test]
fn provider_failure_reports_status_without_echoing_the_response_or_credential() {
    let (endpoint, arrival, release, server) =
        provider("400 Bad Request", "private-provider-error-body".into());
    let child = cli(&endpoint, true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    arrival.recv_timeout(Duration::from_secs(10)).unwrap();
    release.send(()).unwrap();
    let result = child.wait_with_output().unwrap();
    server.join().unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("call 1 single request failed"));
    assert!(stderr.contains("analysis failed"));
    assert!(!stderr.contains("private-"));
}

#[test]
fn independent_tool_implementation_accepts_progress_and_reports_preflight_failure() {
    let result = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args([
            "eal",
            "implement",
            "--progress",
            "--input",
            "/missing/progress-fixture.json",
            "--model",
            "private-model-name",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("tool implementation started"));
    assert!(stderr.contains("tool implementation failed"));
    assert!(!stderr.contains("private-model-name"));
}
