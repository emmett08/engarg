//! Oversized assessment lists remain complete and produce visible, durable warnings.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[path = "common/live_output.rs"]
mod live_output;

fn receive(listener: &TcpListener) -> (TcpStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "expected provider call did not arrive"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("provider accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut request = Vec::new();
    loop {
        let mut bytes = [0; 8192];
        let count = stream.read(&mut bytes).unwrap();
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
                let body = serde_json::from_slice(&request[end + 4..end + 4 + length]).unwrap();
                return (stream, body);
            }
        }
    }
}

#[test]
fn oversized_assessments_warn_with_or_without_progress_and_retain_every_gap() {
    let argument: Value = live_output::case(include_str!("../examples/claim-only-case.json"));
    let gaps = (1..=41)
        .map(|index| format!("private evidence gap {index}: retain its complete condition"))
        .collect::<Vec<_>>();
    let assessment = json!({"findings":[], "objections":[], "evidence_gaps": gaps});
    for progress in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let audit = directory.path().join("audit.jsonl");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let provider_argument = argument.clone();
        let provider_assessment = assessment.clone();
        let server = thread::spawn(move || {
            for index in 0..6 {
                let (mut stream, body) = receive(&listener);
                let envelope: Value =
                    serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
                let properties = &body["text"]["format"]["schema"]["properties"];
                let assessment_role = properties.get("evidence_gaps").is_some();
                assert_eq!(assessment_role, matches!(index, 0..=2 | 4));
                if index >= 3 {
                    let independent = envelope["independent_assessments"].as_array().unwrap();
                    assert_eq!(independent.len(), 3);
                    for source in independent {
                        assert_eq!(
                            source["assessment"]["evidence_gaps"],
                            provider_assessment["evidence_gaps"]
                        );
                    }
                }
                if index == 5 {
                    assert_eq!(
                        envelope["final_critique"]["evidence_gaps"],
                        provider_assessment["evidence_gaps"]
                    );
                }
                let content = if assessment_role {
                    for field in ["findings", "objections", "evidence_gaps"] {
                        assert_eq!(properties[field]["maxItems"], 32);
                    }
                    provider_assessment.to_string()
                } else {
                    provider_argument.to_string()
                };
                let response = json!({"id":"private-provider-id", "model":"private-model-name", "status":"completed", "output":[{"type":"message", "content":[{"type":"output_text", "text":content}]}]}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args([
                "analyse",
                "--mode",
                "hybrid",
                "--max-calls",
                "6",
                "--max-repairs",
                "0",
                "--no-tool-planning",
                "--http-retries",
                "0",
                "--max-concurrent-requests",
                "1",
                "--model",
                "private-model-name",
                "--api-key-env",
                "ENGARG_WARNING_TEST_KEY",
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
            .arg("--audit")
            .arg(&audit)
            .env("ENGARG_WARNING_TEST_KEY", "private-test-credential");
        if progress {
            command.arg("--progress");
        }
        let output = command.output().unwrap();
        server.join().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["argument"], argument);
        let transcript = report["transcript"].as_array().unwrap();
        assert_eq!(transcript.len(), 6);
        for index in [0, 1, 2, 4] {
            let content: Value =
                serde_json::from_str(transcript[index]["response"]["content"].as_str().unwrap())
                    .unwrap();
            assert_eq!(content, assessment);
        }
        let warnings = stderr
            .lines()
            .filter(|line| line.contains("warning:"))
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 4, "{stderr}");
        for role in ["constructor", "sceptic", "evidence_auditor", "final_critic"] {
            let warning = warnings.iter().find(|line| line.contains(role)).unwrap();
            assert!(warning.contains("evidence_gaps"), "{warning}");
            assert!(warning.contains("41"), "{warning}");
            assert!(warning.contains("32"), "{warning}");
        }
        if progress {
            assert!(stderr.contains("analysis completed"));
        } else {
            assert_eq!(stderr.lines().count(), 4, "{stderr}");
        }
        assert!(!stderr.contains("private"));
        let events: Vec<Value> = std::fs::read_to_string(audit)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "completion")
                .count(),
            6
        );
        assert_eq!(events.last().unwrap()["event"], "success");
        let audit_warnings = events
            .iter()
            .filter(|event| event["event"] == "warning")
            .collect::<Vec<_>>();
        assert_eq!(audit_warnings.len(), 4);
        for event in audit_warnings {
            assert_eq!(event["code"], "assessment_list_limit_exceeded");
            assert_eq!(event["warning"]["field"], "evidence_gaps");
            assert_eq!(event["warning"]["count"], 41);
            assert_eq!(event["warning"]["requested_maximum"], 32);
            let role = event["warning"]["role"].as_str().unwrap();
            let warning = warnings.iter().find(|line| line.contains(role)).unwrap();
            let message = warning.strip_prefix("engarg: warning: ").unwrap();
            assert!(
                report["limitations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value == message)
            );
        }
        for record in transcript {
            assert!(
                events
                    .iter()
                    .any(|event| event["event"] == "completion" && event["record"] == *record)
            );
        }
    }
}
