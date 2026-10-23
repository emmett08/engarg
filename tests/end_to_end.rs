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

#[derive(Clone, Copy)]
enum Input {
    Evidence,
    Claim,
    MinimalJson,
}

fn run_case(corrupt_final: bool, input: Input, mode: &str) {
    let claim_only = !matches!(input, Input::Evidence);
    let request_text = if claim_only {
        include_str!("../examples/claim-only-request.json")
    } else {
        include_str!("../examples/request.json")
    };
    let request: Value = serde_json::from_str(request_text).unwrap();
    let expected_claim = request["claim"].as_str().unwrap().to_owned();
    let calls = if mode == "single" { 1 } else { 6 };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for index in 0..calls {
            let (mut stream, _) = listener.accept().unwrap();
            let body = receive(&mut stream);
            assert_eq!(body["model"], "gpt-6.1-sol");
            assert_eq!(body["service_tier"], "ultrafast");
            let envelope: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
            assert_eq!(envelope["request"]["claim"], expected_claim);
            if claim_only {
                assert_eq!(envelope["request"]["context"], "");
                assert_eq!(envelope["request"]["evidence"], json!([]));
                assert!(
                    body["instructions"]
                        .as_str()
                        .unwrap()
                        .contains("A claim alone is a complete starting input")
                );
            }
            let is_case = body["text"]["format"]["schema"]["properties"]
                .get("current_claim")
                .is_some();
            let content = if is_case {
                let case_text = if claim_only {
                    include_str!("../examples/claim-only-case.json")
                } else {
                    include_str!("../examples/case.json")
                };
                let mut case: Value = serde_json::from_str(case_text).unwrap();
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
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command.args(["analyse", "--format", "json", "--mode", mode]);
    match input {
        Input::Evidence => {
            command.args([
                "--input",
                concat!(env!("CARGO_MANIFEST_DIR"), "/examples/request.json"),
            ]);
        }
        Input::Claim => {
            command
                .arg("--claim")
                .arg(request["claim"].as_str().unwrap());
        }
        Input::MinimalJson => {
            let path = directory.path().join("claim.json");
            std::fs::write(&path, request_text).unwrap();
            command.arg("--input").arg(path);
        }
    }
    let result = command
        .args([
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
    assert_eq!(events.len(), calls + 2);
    assert_eq!(events[0]["event"], "start");
    assert_eq!(
        events.iter().filter(|e| e["event"] == "completion").count(),
        calls
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
        let report: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(report["transcript"].as_array().unwrap().len(), calls);
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
        if claim_only {
            assert_eq!(report["request"]["evidence"], json!([]));
            assert_eq!(report["argument"]["status"], "conditional");
            assert!(
                report["argument"]["grounds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|ground| {
                        ground["status"] != "reported" && ground["evidence_ids"] == json!([])
                    })
            );
            assert!(
                !report["argument"]["warrants"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                !report["argument"]["evidence_plan"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            if matches!(input, Input::Claim) {
                check_claim_only_presentations_and_export(&output, directory.path());
            }
        }
    }
}

fn check_claim_only_presentations_and_export(
    report: &std::path::Path,
    directory: &std::path::Path,
) {
    for (format, marker) in [
        ("markdown", "14. Decision basis"),
        ("outline", "current"),
        ("prose", "parallel"),
        ("html", "<details"),
    ] {
        let rendered = Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["render", "--input"])
            .arg(report)
            .args(["--format", format])
            .output()
            .unwrap();
        assert!(
            rendered.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&rendered.stderr)
        );
        assert!(String::from_utf8_lossy(&rendered.stdout).contains(marker));
    }
    let bundle_path = directory.join("case.eal.json");
    let exported = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "export", "--input"])
        .arg(report)
        .args(["--format", "json", "--output"])
        .arg(&bundle_path)
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let bundle: Value = serde_json::from_slice(&std::fs::read(bundle_path).unwrap()).unwrap();
    assert_eq!(bundle["original_request"]["evidence"], json!([]));
    assert!(!bundle["obligations"].as_array().unwrap().is_empty());
    assert!(
        bundle["source"]
            .as_str()
            .unwrap()
            .starts_with("language \"EAL/3\"")
    );
}

#[test]
fn six_call_cli_http_workflow_retains_complete_audit() {
    run_case(false, Input::Evidence, "hybrid")
}

#[test]
fn invalid_final_case_fails_and_preserves_paid_call_receipts() {
    run_case(true, Input::Evidence, "hybrid")
}

#[test]
fn claim_alone_constructs_the_hybrid_argument_plan_and_all_presentations() {
    run_case(false, Input::Claim, "hybrid");
}

#[test]
fn minimal_claim_json_constructs_a_single_call_argument_without_context_or_evidence() {
    run_case(false, Input::MinimalJson, "single");
}
