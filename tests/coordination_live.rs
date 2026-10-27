//! Real eal-coordinate execution preserves graph routing through the public engine and CLI.
//!
//! Set ENGARG_EAL_COORDINATE to an absolute native eal-coordinate executable and
//! run `cargo test --test coordination_live -- --include-ignored`.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use eal_core::{ExecutionLimits, composition, planning::EvidencePlanner, validation};
use engarg::{
    Error,
    domain::ArgumentCase,
    engine::{
        CoordinatorConfig, RunConfig, RunReport, run,
        topology::{DebateConfig, Topology},
    },
    provider::{Completion, CompletionRequest, LanguageModel, Role},
};
use serde_json::{Value, json};

#[path = "common/live_output.rs"]
mod live_output;
#[path = "common/reasoning.rs"]
#[allow(dead_code)]
mod reasoning;
#[path = "common/reasoning_graph.rs"]
#[allow(dead_code)]
mod reasoning_graph;

const TOPOLOGIES: [&str; 9] = [
    "fully_connected",
    "grouped",
    "chain",
    "ring",
    "star",
    "tree",
    "layered",
    "sparse",
    "bipartite",
];

struct FixtureProvider {
    case: ArgumentCase,
    calls: Mutex<Vec<CompletionRequest>>,
}

fn assessment(data: &Value) -> Value {
    let finding = match (
        data["debate"]["participant"].as_u64(),
        data["debate"]["round"].as_u64(),
    ) {
        (Some(participant), Some(round)) => format!("Participant {participant} round {round} retains the independent source and model obligations."),
        _ => "The final case retains the declared bounded computation and its independent evidence obligations.".into(),
    };
    json!({"findings":[finding],"objections":[],"evidence_gaps":[]})
}

fn assert_schema(schema: &Value, output: &Value) {
    let validator = jsonschema::validator_for(schema).unwrap();
    let errors = validator
        .iter_errors(output)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{}", errors.join("; "));
}

impl LanguageModel for FixtureProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let data: Value = serde_json::from_str(&request.user)?;
        let output = if matches!(request.role, Role::Synthesiser | Role::Adjudicator) {
            live_output::generation_value(&self.case)
        } else {
            assessment(&data)
        };
        assert_schema(&request.schema, &output);
        self.calls.lock().unwrap().push(request);
        Ok(Completion {
            content: serde_json::to_string(&output)?,
            model: "native-coordination-fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

#[tokio::test]
async fn native_provider_responses_satisfy_generation_schema_without_a_coordinator() {
    let (_, case) = reasoning::fixture(&reasoning::cases().remove(0));
    let original = serde_json::to_value(&case).unwrap();
    let provider = FixtureProvider {
        case,
        calls: Mutex::new(Vec::new()),
    };
    for role in [Role::Synthesiser, Role::Adjudicator] {
        let schema = engarg::domain::case_schema();
        let response = provider
            .complete(CompletionRequest {
                role,
                system: String::new(),
                user: "{}".into(),
                schema: schema.clone(),
            })
            .await
            .unwrap();
        let output: Value = serde_json::from_str(&response.content).unwrap();
        assert_schema(&schema, &output);
        assert_eq!(output.get("dissent_revisions"), Some(&Value::Null));
        assert!(output["warrants"][0]["reasoning_method"]["method"]["query_json"].is_string());
        assert!(
            output["warrants"][0]["reasoning_method"]["method"]
                .get("query")
                .is_none()
        );
        let canonical: ArgumentCase = serde_json::from_value(output).unwrap();
        assert_eq!(
            serde_json::to_value(canonical).unwrap()["warrants"],
            original["warrants"]
        );
    }
    let mut revised = original;
    revised["dissent_revisions"] = json!({
        "schema": "engarg.dissent-revisions/1",
        "applications": [{"objection_id": "retained-objection"}],
    });
    let mut wire = live_output::generation_value(&revised);
    assert_eq!(wire["dissent_revisions"], revised["dissent_revisions"]);
    // This deliberately partial revision tests preservation of authored values;
    // the remainder is decoded through the same method transport as production.
    wire["dissent_revisions"] = Value::Null;
    let canonical: ArgumentCase = serde_json::from_value(wire).unwrap();
    assert_eq!(
        serde_json::to_value(canonical).unwrap()["warrants"],
        revised["warrants"]
    );
}

fn native_coordinator() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("ENGARG_EAL_COORDINATE")
            .expect("set ENGARG_EAL_COORDINATE to an absolute native eal-coordinate executable"),
    );
    assert!(path.is_absolute() && path.is_file(), "{path:?}");
    path
}

fn config(topology: &str, executable: PathBuf, directory: PathBuf) -> RunConfig {
    RunConfig {
        debate: Some(DebateConfig {
            participants: 4,
            rounds: 1,
            topology: Topology::preset(topology, 4).unwrap(),
            adaptation: Default::default(),
        }),
        coordinator: Some(CoordinatorConfig {
            executable,
            run_directory: directory,
        }),
        max_calls: 11,
        ..RunConfig::default()
    }
}

fn native_json(executable: &Path, operation: &str, run_directory: &Path) -> Value {
    let output = Command::new(executable)
        .args([operation, "--run"])
        .arg(run_directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{operation}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_committed_session(executable: &Path, run_directory: &Path) {
    assert!(run_directory.join("debate.sqlite").is_file());
    let status = native_json(executable, "debate-status", run_directory);
    assert_eq!(status["schema"], "EAL/debate-session-status/1");
    assert_eq!(status["status"], "completed", "{status}");
    assert_eq!(status["expected_calls"], 8);
    assert_eq!(status["committed_calls"], 8);
    assert_eq!(status["uncertain_calls"], 0);
    assert_eq!(status["automatic_reexecution"], false);
    let exported = native_json(executable, "debate-export", run_directory);
    assert_eq!(exported["schema"], "EAL/debate-session-export/1");
    assert_eq!(exported["status"]["status"], "completed");
    assert_eq!(exported["config"]["expected_calls"], 8);
    let effects = exported["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 8);
    assert!(effects.iter().all(|effect| effect["status"] == "committed"));
    assert!(effects.iter().all(|effect| {
        effect["payload"]["dispatch_frame"]["path"].is_string()
            && effect["result"]["response_frame"]["path"].is_string()
    }));
    assert_eq!(
        exported["config"]["spec_sha256"].as_str().unwrap().len(),
        64
    );
    assert!(
        exported["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty())
    );
}

fn assert_routing(calls: &[CompletionRequest], debate: &DebateConfig) {
    let graphs = debate.graphs().unwrap();
    let mut seen = 0;
    for call in calls {
        let data: Value = serde_json::from_str(&call.user).unwrap();
        let Some(round) = data["debate"]["round"].as_u64() else {
            continue;
        };
        let participant = data["debate"]["participant"].as_u64().unwrap() as usize;
        assert!(participant < usize::from(debate.participants));
        seen += 1;
        if round == 0 {
            assert!(data.get("peer_assessments").is_none());
            continue;
        }
        let graph = &graphs[round as usize - 1];
        let incoming = graph
            .edges
            .iter()
            .filter(|edge| edge.to == participant)
            .map(|edge| edge.from)
            .collect::<Vec<_>>();
        assert_eq!(
            data["debate"]["edges"],
            serde_json::to_value(&graph.edges).unwrap()
        );
        assert_eq!(data["debate"]["incoming"], json!(incoming));
        let peers = data["peer_assessments"].as_array().unwrap();
        assert_eq!(
            peers
                .iter()
                .map(|peer| peer["participant"].as_u64().unwrap() as usize)
                .collect::<Vec<_>>(),
            incoming,
        );
        assert!(peers.iter().all(|peer| peer["round"] == round - 1));
        assert_eq!(data["previous_assessment"]["participant"], participant);
        assert_eq!(data["previous_assessment"]["round"], round - 1);
        assert!(data.get("independent_assessments").is_none());
        assert!(data.get("discussion_assessments").is_none());
    }
    assert_eq!(seen, 8);
}

fn assert_native_export(report: &RunReport) {
    let (_, _, elaboration, _, _) = reasoning_graph::graph_fixture(&reasoning::cases().remove(0));
    let versions = elaboration
        .acquisitions
        .iter()
        .map(|source| (source.id.clone(), "1".into()))
        .collect();
    let bundle = engarg::support_export::compile_direct(
        &report.request,
        &report.argument,
        &elaboration,
        &versions,
        &engarg::export_policy::ExportPolicy::default(),
    )
    .unwrap()
    .bundle;
    let program =
        composition::compile(&bundle.source, None, "", ExecutionLimits::default()).unwrap();
    let diagnostics = validation::validate_default(&program);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let plan = EvidencePlanner::new(&program)
        .unwrap()
        .plan(&bundle.claim_names["current_claim"])
        .unwrap();
    assert!(
        plan.calls
            .iter()
            .all(|call| call.tool.as_deref() != Some("engarg_reasoning_input"))
    );
    assert!(
        program
            .reasoning
            .values()
            .any(|reasoning| reasoning.method != "structured/1")
    );
}

#[tokio::test]
#[ignore = "requires native eal-coordinate; set ENGARG_EAL_COORDINATE"]
async fn all_topologies_execute_in_the_native_coordinator_and_compile_as_eal() {
    let executable = native_coordinator();
    let (request, case) = reasoning::fixture(&reasoning::cases().remove(0));
    let workspace = tempfile::tempdir().unwrap();
    for topology in TOPOLOGIES {
        let directory = workspace.path().join(topology);
        let config = config(topology, executable.clone(), directory.clone());
        let provider = FixtureProvider {
            case: case.clone(),
            calls: Mutex::new(Vec::new()),
        };
        let report = run(&provider, &request, &config).await.unwrap();
        let calls = provider.calls.lock().unwrap();
        assert_eq!(calls.len(), 11, "{topology}");
        assert_routing(&calls, config.debate.as_ref().unwrap());
        assert_native_export(&report);
        assert_committed_session(&executable, &directory);
    }
}

#[tokio::test]
async fn a_missing_coordinator_fails_before_any_provider_call() {
    let workspace = tempfile::tempdir().unwrap();
    let (request, case) = reasoning::fixture(&reasoning::cases().remove(0));
    let provider = FixtureProvider {
        case,
        calls: Mutex::new(Vec::new()),
    };
    let config = config(
        "ring",
        workspace.path().join("missing-eal-coordinate"),
        workspace.path().join("session"),
    );
    assert!(run(&provider, &request, &config).await.is_err());
    assert!(provider.calls.lock().unwrap().is_empty());
}

fn receive(listener: &TcpListener) -> (TcpStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "EngArg CLI stopped before all calls"
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
        assert_ne!(count, 0, "provider request ended before its body");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 2 * 1024 * 1024);
        let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
            continue;
        };
        let length = String::from_utf8_lossy(&bytes[..header_end])
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .expect("content length");
        if bytes.len() >= header_end + 4 + length {
            let value =
                serde_json::from_slice(&bytes[header_end + 4..header_end + 4 + length]).unwrap();
            return (stream, value);
        }
    }
}

#[test]
#[ignore = "requires native eal-coordinate; set ENGARG_EAL_COORDINATE"]
fn the_actual_cli_coordinates_a_ring_through_a_schema_checked_http_provider() {
    let executable = native_coordinator();
    let workspace = tempfile::tempdir().unwrap();
    let directory = workspace.path().join("session");
    let request_path = workspace.path().join("request.json");
    let output_path = workspace.path().join("report.json");
    let (request, case) = reasoning::fixture(&reasoning::cases().remove(0));
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut calls = Vec::new();
        for _ in 0..11 {
            let (mut stream, body) = receive(&listener);
            assert_eq!(body["text"]["format"]["strict"], true);
            assert_eq!(body["store"], false);
            let data: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
            let schema = &body["text"]["format"]["schema"];
            let output = if schema["properties"].get("current_claim").is_some() {
                live_output::generation_value(&case)
            } else {
                assessment(&data)
            };
            assert_schema(schema, &output);
            let response = json!({
                "id":"native-cli-fixture", "model":"fixture", "status":"completed",
                "output":[{"type":"message","content":[{"type":"output_text","text":output.to_string()}]}],
            }).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            stream.flush().unwrap();
            calls.push(data);
        }
        calls
    });
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--input"])
        .arg(&request_path)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            &endpoint,
            "--no-tool-planning",
            "--topology",
            "ring",
            "--debate-participants",
            "4",
            "--max-calls",
            "11",
            "--eal-coordinate",
        ])
        .arg(&executable)
        .arg("--coordination-run")
        .arg(&directory)
        .args(["--format", "json", "--output"])
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 11);
    let report: RunReport = serde_json::from_slice(&std::fs::read(output_path).unwrap()).unwrap();
    let saved = report.run_config.as_ref().unwrap();
    let coordinator = saved.coordinator.as_ref().unwrap();
    assert_eq!(coordinator.executable, executable);
    assert_eq!(coordinator.run_directory, directory);
    assert_routing(
        &report
            .transcript
            .iter()
            .map(|record| record.prompt.clone())
            .collect::<Vec<_>>(),
        saved.debate.as_ref().unwrap(),
    );
    assert_native_export(&report);
    assert_committed_session(&executable, &directory);
}
