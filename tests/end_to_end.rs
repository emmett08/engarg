//! Entire CLI and HTTP workflow against a deterministic local API double.
//! This verifies integration, not live model quality.

#[path = "common/argument_assertions.rs"]
mod argument_assertions;

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn accept_call(listener: &TcpListener, index: usize) -> std::net::TcpStream {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "API double timed out waiting for call {}",
                    index + 1
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("API double could not accept call {}: {error}", index + 1),
        }
    }
}

fn receive(stream: &mut std::net::TcpStream) -> Value {
    // Accepted sockets can inherit a nonblocking listener's mode on macOS.
    // Read timeouts bound blocking reads, so establish that mode explicitly.
    stream.set_nonblocking(false).unwrap();
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

#[test]
fn receive_reads_delayed_fragments_from_an_initially_nonblocking_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut stream = accept_call(&listener, 0);
    // Force the inherited mode on every platform rather than depending on the OS.
    stream.set_nonblocking(true).unwrap();
    assert_eq!(
        stream.peek(&mut [0_u8; 1]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let expected = json!({"claim":"Retain the complete fragmented request.","fragmented":true});
    let body = expected.to_string();
    let header = format!(
        "POST /v1/responses HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let (ready, start) = std::sync::mpsc::sync_channel(0);
    let writer = thread::spawn(move || {
        start.recv_timeout(Duration::from_secs(5)).unwrap();
        thread::sleep(Duration::from_millis(50));
        for fragment in [
            &header.as_bytes()[..header.len() - 2],
            &header.as_bytes()[header.len() - 2..],
            &body.as_bytes()[..body.len() / 2],
            &body.as_bytes()[body.len() / 2..],
        ] {
            client.write_all(fragment).unwrap();
            thread::sleep(Duration::from_millis(20));
        }
    });
    ready.send(()).unwrap();
    assert_eq!(receive(&mut stream), expected);
    assert_eq!(
        stream.read_timeout().unwrap(),
        Some(Duration::from_secs(10))
    );
    writer.join().unwrap();
}

#[derive(Clone, Copy)]
enum Input {
    Evidence,
    Claim,
    MinimalJson,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ReadingFixture {
    #[default]
    Original,
    Concise,
}

struct CaseOptions {
    repair_missing_plan: bool,
    tool_planning: bool,
    reading: ReadingFixture,
}

impl Default for CaseOptions {
    fn default() -> Self {
        Self {
            repair_missing_plan: false,
            tool_planning: true,
            reading: ReadingFixture::Original,
        }
    }
}

fn case_fixture(claim_only: bool, reading: ReadingFixture) -> Value {
    let text = if claim_only {
        include_str!("../examples/claim-only-case.json")
    } else {
        include_str!("../examples/case.json")
    };
    let mut case: Value = serde_json::from_str(text).unwrap();
    if reading == ReadingFixture::Concise {
        assert!(claim_only);
        let original = case.clone();
        case["reading"] = json!([
            {
                "claim_id":"current_claim",
                "title":"Conditional benefit from parallel execution",
                "paragraphs":[{
                    "purpose":"answer",
                    "text":"Parallel CI groups conditionally shorten the complete pipeline and preserve outcomes because critical-path savings must exceed added scheduling, setup and contention costs, and equivalent isolated execution must preserve every required check. G1 and G2 are assumptions without supplied observations. A remaining bottleneck or hidden shared state can defeat those premises, while comparison and coverage remain untested. First profile complete-pipeline timing and audit selection, isolation, outcomes and inference backing under controlled, representative conditions. Paired observations remain necessary for the stronger empirical target.",
                    "reference_ids":["current_claim","S1","G1","G2","G3","W1","W2","W3","A1","A2","D1","D2","stronger_target","N-G1","N-G2","N-G3","N-W1","N-W2","N-W3"]
                }]
            },
            {
                "claim_id":"S1",
                "title":"When overlap shortens the pipeline",
                "paragraphs":[{
                    "purpose":"answer",
                    "text":"Useful overlap conditionally shortens the complete pipeline because removed critical-path work must exceed all added costs while the remaining work stays comparable. A different bottleneck, contention or setup can erase that benefit. Complete-pipeline profiles and a justified timing decomposition must test the mechanism under the declared comparison conditions.",
                    "reference_ids":["S1","G1","W1","A1","D1","N-G1","N-W1"]
                }]
            }
        ]);
        let mut restored = case.clone();
        restored["reading"] = original["reading"].clone();
        assert_eq!(
            restored, original,
            "the concise fixture only changes reading"
        );
    }
    case
}

fn run_case(corrupt_final: bool, input: Input, mode: &str) {
    run_configured(corrupt_final, input, mode, CaseOptions::default());
}

fn run_configured(corrupt_final: bool, input: Input, mode: &str, options: CaseOptions) {
    let CaseOptions {
        repair_missing_plan,
        tool_planning,
        reading,
    } = options;
    let claim_only = !matches!(input, Input::Evidence);
    let request_text = if claim_only {
        include_str!("../examples/claim-only-request.json")
    } else {
        include_str!("../examples/request.json")
    };
    let request: Value = serde_json::from_str(request_text).unwrap();
    let expected_claim = request["claim"].as_str().unwrap().to_owned();
    let expected_argument = case_fixture(claim_only, reading);
    let response_case = expected_argument.clone();
    let base_calls = if mode == "single" { 1 } else { 6 };
    let calls = base_calls + usize::from(repair_missing_plan);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for index in 0..calls {
            let mut stream = accept_call(&listener, index);
            let body = receive(&mut stream);
            assert_eq!(body["model"], "gpt-6.1-sol");
            assert_eq!(body["service_tier"], "ultrafast");
            assert_eq!(body["reasoning"], json!({"effort":"high"}));
            assert!(body.get("reasoning_effort").is_none());
            let instructions = body["instructions"].as_str().unwrap();
            assert!(instructions.contains("ENGINEERING WRITING SKILL"));
            assert!(instructions.contains("ARGUMENT ROLE GLOSSARY"));
            assert!(instructions.contains(&engarg::glossary::find("W").unwrap().definition));
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
            if repair_missing_plan {
                assert!(
                    body["instructions"]
                        .as_str()
                        .unwrap()
                        .contains("Use at most 2 warrant layers")
                );
            }
            let properties = &body["text"]["format"]["schema"]["properties"];
            let planned = properties.get("argument").is_some();
            let is_case = planned || properties.get("current_claim").is_some();
            let content = if is_case {
                assert_eq!(planned, tool_planning);
                let mut case = response_case.clone();
                if corrupt_final && index == 5 {
                    case["original_claim"] = json!("silently changed claim");
                }
                if repair_missing_plan {
                    let mut objection = case["defeaters"][0].clone();
                    objection["id"] = json!("auditor-5");
                    case["defeaters"].as_array_mut().unwrap().push(objection);
                    if index == base_calls {
                        assert_eq!(envelope["repair_attempt"], 1);
                        assert!(envelope["validation_errors"].as_array().unwrap().iter().any(|error| error == "unresolved material defeater auditor-5 needs an evidence plan"));
                        let mut plan = case["evidence_plan"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|need| need["target_id"] == "D1")
                            .unwrap()
                            .clone();
                        plan["id"] = json!("repair-plan");
                        plan["target_id"] = json!("auditor-5");
                        case["evidence_plan"].as_array_mut().unwrap().push(plan);
                    }
                }
                if planned {
                    let plans = case["evidence_plan"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|need| {
                            json!({
                                "need_id":need["id"],
                                "tool_id":"custom",
                                "arguments":[],
                                "unresolved_arguments":["adapter"],
                            })
                        })
                        .collect::<Vec<_>>();
                    json!({"argument":case,"acquisition_plan":plans}).to_string()
                } else {
                    case.to_string()
                }
            } else {
                json!({"findings":["Synthetic API test fixture."],"objections":[],"evidence_gaps":[]}).to_string()
            };
            let response=json!({"id":format!("test-response-{index}"),"status":"completed","model":"deterministic-http-double","service_tier":"default","reasoning":{"effort":"medium"},"output":[{"type":"message","content":[{"type":"output_text","text":content}]}],"usage":{"input_tokens":10,"output_tokens":20}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
    });
    let directory = tempfile::tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let output = directory.path().join("report.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command.args(["analyse", "--format", "json", "--mode", mode]);
    if !tool_planning {
        command.arg("--no-tool-planning");
    }
    if repair_missing_plan {
        command.args([
            "--argument-depth",
            "2",
            "--display-depth",
            "1",
            "--max-repairs",
            "1",
            "--max-calls",
            "7",
        ]);
    }
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
    if matches!(input, Input::MinimalJson) {
        command.env("ENGARG_REASONING_EFFORT", "high");
    } else {
        command
            .args(["--reasoning-effort", "high"])
            .env("ENGARG_REASONING_EFFORT", "low");
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
    assert_eq!(events.len(), 2 * calls + 2);
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "http_attempt"
                && event["record"]["kind"] == "response"
                && event["record"]["status"] == 200)
            .count(),
        calls
    );
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
        if reading == ReadingFixture::Concise {
            argument_assertions::assert_authored_argument(&report, &expected_argument);
            for section in report["argument"]["reading"].as_array().unwrap() {
                let paragraphs = section["paragraphs"].as_array().unwrap();
                assert_eq!(paragraphs.len(), 1);
                assert_eq!(paragraphs[0]["purpose"], "answer");
            }
        }
        assert_eq!(report["transcript"].as_array().unwrap().len(), calls);
        let final_content = report["transcript"][calls - 1]["response"]["content"]
            .as_str()
            .unwrap();
        if tool_planning {
            let planned = engarg::acquisition::parse_completion(final_content).unwrap();
            argument_assertions::assert_authored_argument(
                &report,
                &serde_json::to_value(&planned.argument).unwrap(),
            );
            assert_eq!(
                planned.acquisition_plan.len(),
                planned.argument.evidence_plan.len()
            );
            assert!(
                engarg::acquisition::validate(&planned.argument, &planned.acquisition_plan)
                    .is_empty()
            );
            assert!(report.get("acquisition_plan").is_none());
        } else {
            argument_assertions::assert_authored_argument(
                &report,
                &serde_json::from_str::<Value>(final_content).unwrap(),
            );
            assert!(
                report["transcript"][calls - 1]["prompt"]["schema"]["properties"]
                    .get("argument")
                    .is_none()
            );
        }
        assert_eq!(report["input_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(report["provider_config"]["default_model"], "gpt-6.1-sol");
        assert_eq!(
            report["provider_config"]["requested_service_tier"],
            "ultrafast"
        );
        assert_eq!(
            report["provider_config"]["requested_reasoning_effort"],
            "high"
        );
        let mut legacy = report.clone();
        legacy.as_object_mut().unwrap().remove("run_config");
        legacy["provider_config"]
            .as_object_mut()
            .unwrap()
            .remove("requested_reasoning_effort");
        for call in legacy["transcript"].as_array_mut().unwrap() {
            call["response"]
                .as_object_mut()
                .unwrap()
                .remove("reasoning_effort");
        }
        let legacy: engarg::engine::RunReport = serde_json::from_value(legacy).unwrap();
        assert!(legacy.run_config.is_none());
        assert_eq!(
            legacy.provider_config.unwrap().requested_reasoning_effort,
            None
        );
        assert!(
            legacy
                .transcript
                .iter()
                .all(|call| call.response.reasoning_effort.is_none())
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
        assert_eq!(events[0]["requested_reasoning_effort"], "high");
        if repair_missing_plan {
            assert_eq!(report["run_config"]["max_argument_depth"], 2);
            assert_eq!(report["run_config"]["max_repairs"], 1);
            assert_eq!(events[0]["config"]["max_argument_depth"], 2);
            assert_eq!(events[0]["config"]["max_repairs"], 1);
            assert!(
                report["argument"]["evidence_plan"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|need| need["target_id"] == "auditor-5")
            );
            assert_eq!(
                events[events.len() - 2]["record"]["prompt"]["role"],
                "adjudicator"
            );
        }
        assert!(
            report["transcript"]
                .as_array()
                .unwrap()
                .iter()
                .all(|record| { record["response"]["reasoning_effort"] == "medium" })
        );
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

#[test]
fn concise_reading_survives_hybrid_and_single_workflows_and_all_presentations() {
    for mode in ["hybrid", "single"] {
        run_configured(
            false,
            Input::Claim,
            mode,
            CaseOptions {
                reading: ReadingFixture::Concise,
                ..CaseOptions::default()
            },
        );
    }
}

#[test]
fn cli_repairs_a_missing_material_defeater_plan_within_the_recorded_call_budget() {
    run_configured(
        false,
        Input::Claim,
        "hybrid",
        CaseOptions {
            repair_missing_plan: true,
            ..CaseOptions::default()
        },
    );
}

#[test]
fn cli_can_retain_the_direct_case_provider_contract_without_tool_planning() {
    run_configured(
        false,
        Input::MinimalJson,
        "single",
        CaseOptions {
            tool_planning: false,
            ..CaseOptions::default()
        },
    );
}
