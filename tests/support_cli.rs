//! Native discovery and the CLI graph path against a local deterministic provider.
#[path = "common/argument_assertions.rs"]
mod argument_assertions;

use engarg::{
    domain::{ArgumentCase, Request},
    engine::SupportRunReport,
    route_planning::RoutePlan,
    support_requirements::{
        SUPPORT_ELABORATION_SCHEMA, SupportElaboration, alternative_obligations, material_targets,
        validate_elaboration,
    },
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

fn pending(case: &ArgumentCase) -> SupportElaboration {
    let mut requirements = Vec::new();
    let mut evaluations = Vec::new();
    for target in material_targets(case) {
        let mut needs = case
            .evidence_plan
            .iter()
            .filter(|need| need.target_id == target)
            .collect::<Vec<_>>();
        needs.sort_by_key(|need| &need.id);
        let id = format!("requirement_{target}");
        let criterion = json!({"kind":"unavailable","reason":"The source and method require operator clarification."});
        requirements.push(json!({
            "id":id,"target_id":target,"evidence_need_ids":needs.iter().map(|need|&need.id).collect::<Vec<_>>(),
            "original_acceptance_criteria":needs.iter().map(|need|&need.acceptance_criterion).collect::<Vec<_>>(),
            "contract":null,"discriminating_evidence":["Retain adverse findings against the same scope."],
            "acceptance":criterion,"acceptance_rationale":"Source availability has not been established.",
            "gaps":[{"kind":"clarification","description":"Declare the source and qualified method before acquisition."}],
        }));
        evaluations.push(json!({"id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[],"criterion":criterion,"max_age_seconds":null,"required_coverage":"unspecified","applicability":[],"challenges":[]}));
    }
    let alternative_support=alternative_obligations(case).into_iter().map(|alternative| {
        let requirement=requirements.last_mut().unwrap();
        requirement["discriminating_evidence"].as_array_mut().unwrap().push(json!(alternative.distinguishing_test));
        json!({"alternative_id":alternative.alternative_id,"distinguishing_test":alternative.distinguishing_test,"claim_ids":alternative.claim_ids,"requirement_ids":[requirement["id"]]})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({"schema":SUPPORT_ELABORATION_SCHEMA,"requirements":requirements,"acquisitions":[],"derivations":[],"evaluations":evaluations,"revisions":[],"alternative_support":alternative_support})).unwrap()
}

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

fn successful(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn load(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "requires the matching native EAL_COLLECTOR binary; exercised by EAL compatibility CI"]
fn graph_analyse_uses_native_discovery_six_local_calls_and_exports_without_flat_overrides() {
    let collector = std::env::var_os("EAL_COLLECTOR")
        .expect("EAL_COLLECTOR must identify the matching native binary");
    let collector = Path::new(&collector).canonicalize().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let connections = directory.path().join("connections.json");
    std::fs::write(
        &connections,
        json!({"schema":"eal.evidence-config/1","connections":[]}).to_string(),
    )
    .unwrap();
    let config = directory.path().join("planning.json");
    std::fs::write(
        &config,
        json!({
            "schema":"engarg.capability-planning-config/1","collector":collector,
            "connections":connections,"store":"DO_NOT_SEND_OPERATOR_CONFIGURATION","environment":[],
            "ranking":{"dimensions":["execution_requests"]},
            "scheduling":{"maximum_concurrent_sources":2,"per_provider":{}},"bindings":[],
        })
        .to_string(),
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
            if index < 4 {
                assert!(data.get("route_preview").is_none());
            }
            if index == 4 {
                assert_eq!(data["route_preview"]["acquisition_count"], 0);
                assert!(!data["route_preview"]["gaps"].as_array().unwrap().is_empty());
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
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args([
            "analyse",
            "--evidence-planning",
            "graph",
            "--evidence-config",
        ])
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
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "graph CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();
    let saved = load(&report_path);
    let report: SupportRunReport<RoutePlan> = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&report.report.request).unwrap(),
        serde_json::to_value(request).unwrap()
    );
    argument_assertions::assert_authored_argument(&saved, &serde_json::to_value(&case).unwrap());
    assert_eq!(
        serde_json::to_value(&report.evidence_planning.elaboration).unwrap(),
        serde_json::to_value(elaboration).unwrap()
    );
    assert_eq!(report.report.transcript.len(), 6);
    assert!(
        report
            .evidence_planning
            .routes
            .discovery
            .admitted_capabilities
            .is_empty()
    );
    assert!(!report.evidence_planning.routes.gaps.is_empty());
    assert!(
        saved
            .to_string()
            .contains("DO_NOT_SEND_OPERATOR_CONFIGURATION")
    );
    let source = directory.path().join("argument.eal");
    let manifest = directory.path().join("argument.bundle.json");
    successful(
        Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "export", "--input"])
            .arg(&report_path)
            .arg("--output")
            .arg(&source)
            .arg("--tools-output")
            .arg(directory.path().join("argument.tools.toml"))
            .arg("--manifest")
            .arg(&manifest),
    );
    assert_eq!(load(&manifest)["schema_version"], "engarg.support-export/1");
    assert!(source.with_extension("evidence-plan.json").is_file());
    assert!(source.with_extension("execution-plan.json").is_file());
    let prepared = directory.path().join("prepared");
    successful(
        Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "prepare", "--input"])
            .arg(&report_path)
            .arg("--output-dir")
            .arg(&prepared),
    );
    assert_eq!(
        load(&prepared.join("argument.bundle.json"))["schema_version"],
        "engarg.support-export/1"
    );
    assert!(prepared.join("evidence-plan.json").is_file());
    assert_eq!(
        load(&prepared.join("review-inputs.json"))["reviews"],
        json!([])
    );
}
