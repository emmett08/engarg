//! Direct operation authoring retains every provider call without leaking bindings.
#[path = "common/argument_assertions.rs"]
mod argument_assertions;

use engarg::{
    domain::{ArgumentCase, Request},
    support_requirements::validate_elaboration,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[path = "common/live_output.rs"]
mod live_output;
#[path = "common/pending_support.rs"]
mod pending_support;
use pending_support::pending;

fn receive(listener: &TcpListener, index: usize) -> (TcpStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "local provider timed out waiting for call {index}"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("local provider accept failed: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        assert!(
            bytes.len() <= 2 * 1_048_576,
            "provider fixture request exceeds its bound"
        );
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = header
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return (
                    stream,
                    serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap(),
                );
            }
        }
    }
}

fn load(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn direct_analyse_retains_six_local_calls_and_redacts_operator_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("tools.toml");
    let input_schema = json!({"type":"object","description":"Observe the declared subject.","properties":{"subject":{"type":"string"}},"required":["subject"],"additionalProperties":false});
    let output_schema = json!({"type":"object","properties":{"count":{"type":"integer"}},"required":["count"],"additionalProperties":false});
    let input_bytes = serde_json::to_vec(&input_schema).unwrap();
    let output_bytes = serde_json::to_vec(&output_schema).unwrap();
    std::fs::write(directory.path().join("input.schema.json"), &input_bytes).unwrap();
    std::fs::write(directory.path().join("output.schema.json"), &output_bytes).unwrap();
    let descriptor = json!({"operations":[{"operation":"explicit_source","version":"7","input_schema":input_schema,"output_schema":output_schema}]});
    std::fs::write(
        &config,
        format!(
            r#"[tools.explicit_source]
kind = "mcp_stdio"
version = "7"
argv = ["DO_NOT_EXECUTE_OR_SEND_OPERATOR_CONFIGURATION"]
env = {{ OPERATOR_SECRET = "DO_NOT_SEND_OPERATOR_CONFIGURATION" }}
[tools.explicit_source.direct]
tool = "source_observe"
[tools.explicit_source.direct.input_schema]
path = "input.schema.json"
sha256 = "{}"
[tools.explicit_source.direct.output_schema]
path = "output.schema.json"
sha256 = "{}"
"#,
            engarg::authoring_checkpoint::digest(&input_bytes),
            engarg::authoring_checkpoint::digest(&output_bytes)
        ),
    )
    .unwrap();
    let case: ArgumentCase = live_output::case(include_str!("../examples/claim-only-case.json"));
    let request: Request =
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap();
    let elaboration = pending(&case);
    assert!(validate_elaboration(&case, &elaboration).is_empty());
    let response_case=json!({"argument":case,"support_elaboration_json":serde_json::to_string(&elaboration).unwrap()}).to_string();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for index in 0..6 {
            let (mut stream, body) = receive(&listener, index);
            let data: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
            assert!(
                !body
                    .to_string()
                    .contains("DO_NOT_SEND_OPERATOR_CONFIGURATION")
            );
            for forbidden in ["acquisition_tools", "collector_catalogue", "configuration"] {
                assert!(
                    data.get(forbidden).is_none(),
                    "unexpected {forbidden} in provider call {index}"
                );
            }
            assert_eq!(data["available_operation_bindings"], descriptor);
            assert!(
                !body
                    .to_string()
                    .contains("DO_NOT_EXECUTE_OR_SEND_OPERATOR_CONFIGURATION")
            );
            if index < 4 {
                assert!(data.get("route_preview").is_none());
            }
            if index == 4 {
                assert_eq!(data["route_preview"]["schema"], "engarg.direct-bindings/1");
                assert_eq!(data["route_preview"]["bindings"], json!({}));
                assert_eq!(data["route_preview"]["unbound_operations"], json!([]));
                assert!(data["route_preview"].get("configuration").is_none());
            }
            let is_case = body["text"]["format"]["schema"]["properties"]
                .get("support_elaboration_json")
                .is_some();
            assert_eq!(is_case, matches!(index, 3 | 5));
            let content = if is_case {
                response_case.clone()
            } else {
                json!({"findings":[],"objections":[],"evidence_gaps":[]}).to_string()
            };
            let response=json!({"id":format!("local-{index}"),"status":"completed","model":"local-fixture","output":[{"type":"message","content":[{"type":"output_text","text":content}]}],"usage":{"input_tokens":1,"output_tokens":1}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
    });
    let report_path = directory.path().join("analysis.json");
    let checkpoint_path = directory.path().join("authoring.checkpoint.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--evidence-planning", "direct", "--tools"])
        .arg(&config)
        .args([
            "--input",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/claim-only-request.json"
            ),
            "--endpoint",
        ])
        .arg(endpoint)
        .args([
            "--api",
            "responses",
            "--model",
            "local-fixture",
            "--mode",
            "hybrid",
            "--max-calls",
            "6",
            "--max-repairs",
            "0",
            "--http-retries",
            "0",
            "--request-timeout-secs",
            "10",
            "--timeout-secs",
            "60",
            "--format",
            "json",
            "--output",
        ])
        .arg(&report_path)
        .arg("--checkpoint")
        .arg(&checkpoint_path)
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "direct CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();
    let saved = load(&report_path);
    assert_eq!(saved["request"], serde_json::to_value(request).unwrap());
    argument_assertions::assert_authored_argument(&saved, &serde_json::to_value(&case).unwrap());
    assert_eq!(
        saved["support_elaboration"],
        serde_json::to_value(elaboration).unwrap()
    );
    assert_eq!(saved["transcript"].as_array().unwrap().len(), 6);
    assert_eq!(
        saved["direct_bindings"]["schema"],
        "engarg.direct-bindings/1"
    );
    assert_eq!(saved["direct_bindings"]["bindings"], json!({}));
    assert_eq!(saved["direct_bindings"]["unbound_operations"], json!([]));
    assert!(saved.get("evidence_planning").is_none());
    assert!(
        !saved
            .to_string()
            .contains("DO_NOT_SEND_OPERATOR_CONFIGURATION")
    );
    assert!(saved["transcript"].as_array().unwrap().iter().all(|call| {
        call["prompt"]["user"]
            .as_str()
            .unwrap()
            .contains("available_operation_bindings")
    }));
    // A valid retained final candidate needs no further provider request.
    let resumed_path = directory.path().join("resumed.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["authoring", "resume", "--checkpoint"])
        .arg(&checkpoint_path)
        .arg("--tools")
        .arg(&config)
        .arg("--output")
        .arg(&resumed_path)
        .arg("--audit")
        .arg(directory.path().join("resume.audit.jsonl"))
        .arg("--next-checkpoint")
        .arg(directory.path().join("resume.checkpoint.json"))
        .args([
            "--model",
            "local-fixture",
            "--endpoint",
            "http://127.0.0.1:9/v1/responses",
            "--api",
            "responses",
            "--http-retries",
            "0",
        ])
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "direct checkpoint resume failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resumed = load(&resumed_path);
    assert_eq!(resumed["support_elaboration"], saved["support_elaboration"]);
    assert_eq!(resumed["direct_bindings"], saved["direct_bindings"]);
    assert_eq!(resumed["authoring_recovery"]["new_model_calls"], 0);
    engarg::authoring_checkpoint::validate_recovery_report(&resumed).unwrap();
}

#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn authored_revision_package(host: Option<&std::path::Path>) {
    use std::fs;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (request, case, elaboration) = executable_case::fixture();
    fs::create_dir(root.join("tools")).unwrap();
    fs::write(
        root.join("tools/source.py"),
        r#"import json,sys
request=json.load(sys.stdin)
if request['input']['operation']=='premise_source':
    values=['a','b','c']
    value={'measurement':{'count':len(values),'distinct':len(set(values))==len(values)}}
else:
    value={'applies':True}
print(json.dumps({'value':value,'observed_at':'2026-10-25T12:00:00Z'}))
"#,
    )
    .unwrap();
    let manifest = root.join("tools.toml");
    fs::write(&manifest,"[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n").unwrap();
    let input = root.join("request.json");
    fs::write(&input, serde_json::to_vec_pretty(&request).unwrap()).unwrap();
    let provider_value = json!({"argument":live_output::generation_value(&case),"support_elaboration_json":serde_json::to_string(&elaboration).unwrap()});
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = receive(&listener, 0);
        let response = json!({"id":"revision-source","status":"completed","model":"fixture","output":[{"type":"message","content":[{"type":"output_text","text":provider_value.to_string()}]}]}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
    });
    let authored = root.join("analysis.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--input"])
        .arg(&input)
        .arg("--tools")
        .arg(&manifest)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            &endpoint,
            "--mode",
            "single",
            "--max-calls",
            "1",
            "--max-repairs",
            "0",
            "--http-retries",
            "0",
            "--format",
            "json",
            "--output",
        ])
        .arg(&authored)
        .env_remove("OPENAI_API_KEY")
        .env_remove("EAL_HOST")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();
    let original_bytes = fs::read(&authored).unwrap();
    let original = load(&authored);
    assert_eq!(original["schema_version"], "engarg.run.v3");
    assert!(original["design_review"].is_object());
    let mut changed_review = original.clone();
    changed_review["design_review"]["active_acquisitions"] = json!(999);
    assert!(engarg::report::read_direct(&changed_review).is_err());
    let mut stale_target = original.clone();
    stale_target["target_runtime"]["language"] = json!("obsolete-language");
    assert!(engarg::report::read_direct(&stale_target).is_err());
    let mut unrecorded_revision = original.clone();
    unrecorded_revision["argument"]["adjudication"] =
        json!("Edited outside the revision protocol.");
    assert!(engarg::report::read_direct(&unrecorded_revision).is_err());
    let mut omitted_review = original.clone();
    omitted_review
        .as_object_mut()
        .unwrap()
        .remove("design_review");
    assert!(engarg::report::read_direct(&omitted_review).is_err());

    let mut argument: ArgumentCase = serde_json::from_value(original["argument"].clone()).unwrap();
    let support = root.join("support.json");
    fs::write(&support, serde_json::to_vec_pretty(&elaboration).unwrap()).unwrap();
    let candidate = root.join("candidate.json");
    argument
        .adjudication
        .push_str(" The explicit revision retains the declared finite snapshot.");
    fs::write(&candidate, serde_json::to_vec_pretty(&argument).unwrap()).unwrap();
    let missing_support = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "revise", "--input"])
        .arg(&authored)
        .arg("--argument")
        .arg(&candidate)
        .arg("--output")
        .arg(root.join("rejected.json"))
        .output()
        .unwrap();
    assert!(!missing_support.status.success());
    assert!(String::from_utf8_lossy(&missing_support.stderr).contains("--support-elaboration"));
    assert!(!root.join("rejected.json").exists());
    let mut previous = authored.clone();
    for index in 1..=2 {
        let target = root.join(format!("revision-{index}.json"));
        let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "revise", "--input"])
            .arg(&previous)
            .arg("--argument")
            .arg(&candidate)
            .arg("--support-elaboration")
            .arg(&support)
            .arg("--tools")
            .arg(&manifest)
            .arg("--output")
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let revised = load(&target);
        assert_eq!(
            revised["argument_revisions"].as_array().unwrap().len(),
            index
        );
        assert_eq!(revised["transcript"], original["transcript"]);
        assert_eq!(revised["direct_bindings"]["unbound_operations"], json!([]));
        engarg::report::read_direct(&revised).unwrap();
        argument = serde_json::from_value(revised["argument"].clone()).unwrap();
        argument
            .adjudication
            .push_str(" All prior dissent remains retained.");
        fs::write(&candidate, serde_json::to_vec_pretty(&argument).unwrap()).unwrap();
        previous = target;
    }
    assert_eq!(fs::read(&authored).unwrap(), original_bytes);
    let mut tampered = load(&previous);
    tampered["argument_revisions"][0]["previous_report"]["argument"]["adjudication"] =
        json!("replacement");
    assert!(engarg::report::read_direct(&tampered).is_err());
    let package = root.join("package");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["build", "--input"])
        .arg(&previous)
        .arg("--tools")
        .arg(&manifest)
        .arg("--output-dir")
        .arg(&package)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = fs::read_to_string(package.join("case.eal")).unwrap();
    assert!(source.starts_with("language \"EAL/4\""));
    assert!(!source.contains("profile \"execution"));
    if let Some(host) = host {
        let host = host.canonicalize().unwrap();
        let moved = tempfile::tempdir().unwrap();
        fs::create_dir(moved.path().join("tools")).unwrap();
        for file in ["case.eal", "tools.toml", "tools/source.py"] {
            fs::copy(package.join(file), moved.path().join(file)).unwrap();
        }
        drop(directory);
        let claim = receipt["claims"]["current_claim"].as_str().unwrap();
        let output = Command::new(host)
            .current_dir(moved.path())
            .args([
                "run",
                "case.eal",
                "--tools",
                "tools.toml",
                "--workspace",
                "run",
                "--claim",
                claim,
                "--now",
                reasoning::NOW,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let executed: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(executed["is_error"], false, "{executed}");
        assert_eq!(
            executed["result"]["assessment"]["claims"][claim]["status"], "supported",
            "{executed}"
        );
    }
}

#[test]
fn public_authoring_revision_revision_and_build_share_one_report_contract() {
    authored_revision_package(None);
}

#[test]
#[ignore = "requires installed ENGARG_EAL_HOST"]
fn revised_public_authoring_package_executes_after_original_sources_are_removed() {
    let host =
        std::path::PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("ENGARG_EAL_HOST"));
    authored_revision_package(Some(&host));
}
