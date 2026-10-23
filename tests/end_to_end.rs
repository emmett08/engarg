//! Entire CLI and HTTP workflow against a deterministic local API double.
//! This verifies integration, not live model quality.

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::Duration,
};

fn receive(stream: &mut std::net::TcpStream) -> Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut received = Vec::new();
    let mut buf = [0; 8192];
    loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        received.extend_from_slice(&buf[..n]);
        if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap();
            if received.len() >= end + 4 + length {
                return serde_json::from_slice(&received[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}

fn run_case(corrupt_final: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for index in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let body = receive(&mut stream);
            assert_eq!(body["model"], "gpt-6.1-sol");
            assert_eq!(body["service_tier"], "ultrafast");
            let is_case = body["text"]["format"]["schema"]["properties"]
                .get("current_claim")
                .is_some();
            let content = if is_case {
                let mut case: Value =
                    serde_json::from_str(include_str!("../examples/case.json")).unwrap();
                if corrupt_final && index == 5 {
                    case["original_claim"] = json!("silently changed claim");
                }
                case.to_string()
            } else {
                json!({"findings":["Synthetic API test fixture."],"objections":[],"evidence_gaps":[]}).to_string()
            };
            let response=json!({"id":format!("test-response-{index}"),"status":"completed","model":"deterministic-http-double","service_tier":"default","output":[{"type":"message","content":[{"type":"output_text","text":content}]}],"usage":{"input_tokens":10,"output_tokens":20}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let output = directory.path().join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args([
            "analyse",
            "--format",
            "json",
            "--input",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/request.json"),
            "--model",
            "gpt-6.1-sol",
            "--service-tier",
            "ultrafast",
            "--endpoint",
        ])
        .arg(format!("http://{address}/v1/responses"))
        .arg("--audit")
        .arg(&audit)
        .arg("--output")
        .arg(&output)
        .env_remove("ENGARG_API_KEY")
        .output()
        .unwrap();
    server.join().unwrap();
    let events: Vec<Value> = std::fs::read_to_string(audit)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(events.len(), 8);
    assert_eq!(events[0]["event"], "start");
    assert_eq!(
        events.iter().filter(|e| e["event"] == "completion").count(),
        6
    );
    if corrupt_final {
        assert!(!result.status.success());
        assert_eq!(events.last().unwrap()["event"], "failure");
        assert!(!output.exists());
        assert!(String::from_utf8_lossy(&result.stderr).contains("original_claim"));
    } else {
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(events.last().unwrap()["event"], "success");
        let report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(report["transcript"].as_array().unwrap().len(), 6);
        assert_eq!(report["input_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(report["provider_config"]["default_model"], "gpt-6.1-sol");
        assert_eq!(
            report["provider_config"]["requested_service_tier"],
            "ultrafast"
        );
        let typed_report: engarg::engine::RunReport =
            serde_json::from_value(report.clone()).unwrap();
        assert_eq!(
            typed_report
                .provider_config
                .unwrap()
                .requested_service_tier
                .as_deref(),
            Some("ultrafast")
        );
        assert_eq!(events[0]["requested_service_tier"], "ultrafast");
        assert_eq!(
            report["transcript"][0]["response"]["model"],
            "deterministic-http-double"
        );
        assert!(
            report["transcript"]
                .as_array()
                .unwrap()
                .iter()
                .all(|record| { record["response"]["service_tier"] == "default" })
        );
    }
}

#[test]
fn six_call_cli_http_workflow_retains_complete_audit() {
    run_case(false)
}

#[test]
fn invalid_final_case_fails_and_preserves_paid_call_receipts() {
    run_case(true)
}
