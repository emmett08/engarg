//! Recovery checkpoints one new exchange while retaining the paid authoring history.
#![cfg(unix)]
#[path = "common/support.rs"]
mod support_fixture;

use eal_collector_contracts::capability::CapabilityCatalogue;
use engarg::{
    route_planning::{DiscoverySnapshot, PlanningConfig},
    support_requirements::SupportElaboration,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn write_json(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn digest(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn receipt(role: &str, content: String) -> Value {
    json!({
        "role":role,
        "prompt":{"role":role,"system":"private retained instructions","user":"{}","schema":{}},
        "response":{"content":content,"model":"private-retained-model","input_tokens":7,"output_tokens":5,"request_id":"private-retained-request","service_tier":null,"reasoning_effort":null},
        "response_content_bytes":content.len(),"response_content_sha256":digest(&content),
        "response_content_omitted":false,"elapsed_ms":1,
    })
}

struct Fixture {
    directory: tempfile::TempDir,
    input: PathBuf,
    config: PathBuf,
    original: Value,
    repaired: SupportElaboration,
    response_limit: Option<usize>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let (request, case, mut repaired, _, route_proofs, _) = support_fixture::fixture();
        repaired.schema = engarg::support_requirements::SUPPORT_ELABORATION_SCHEMA.into();
        let mut pending = repaired.clone();
        pending.acquisitions.clear();
        pending
            .requirements
            .retain(|requirement| !requirement.id.starts_with("shared_requirement_"));
        pending
            .evaluations
            .retain(|evaluation| !evaluation.requirement_id.starts_with("shared_requirement_"));
        assert!(engarg::support_requirements::validate_elaboration(&case, &pending).is_empty());
        assert!(
            engarg::support_requirements::validate_revision(&case, &pending, &case, &repaired)
                .is_empty()
        );

        let catalogue = CapabilityCatalogue {
            schema: eal_collector_contracts::capability::CAPABILITY_CATALOGUE_SCHEMA.into(),
            capabilities: vec![route_proofs[0].capability.clone()],
        };
        let connections = directory.path().join("connections.json");
        write_json(
            &connections,
            &json!({
                "schema":"eal.evidence-config/1","connections":[],"approved_catalogues":[],
                "local_catalogues":[catalogue],
            }),
        );
        let collector = directory.path().join("discovery");
        std::fs::write(&collector, r#"#!/usr/bin/python3
import json,pathlib,sys
assert sys.argv[1]=='evidence-discover',sys.argv
config=json.loads(pathlib.Path(sys.argv[sys.argv.index('--config')+1]).read_text())
catalogue=config['local_catalogues'][0]
print(json.dumps({'schema':'eal.evidence-discovery/1','catalogue':catalogue,'admitted_capabilities':[item['id'] for item in catalogue['capabilities']],'interfaces':[],'gaps':[],'incomplete':False,'metrics':{'requests':1,'request_bytes':0,'response_bytes':0,'elapsed_ms':0,'server_starts':1},'limits':{'requests':64,'bytes':4194304,'timeout_seconds':60,'pages_per_connection':16},'budget_usage':{'requests_upper_bound':1,'bytes_upper_bound':1024,'pages_by_connection':{}}}))
"#).unwrap();
        std::fs::set_permissions(&collector, std::fs::Permissions::from_mode(0o755)).unwrap();
        let configuration: PlanningConfig = serde_json::from_value(json!({
            "schema":engarg::route_planning::PLANNING_CONFIG_SCHEMA,
            "collector":collector,"connections":connections,"store":directory.path().join("store"),
            "environment":[],"ranking":{"dimensions":["execution_requests"]},
            "scheduling":{"maximum_concurrent_sources":2,"per_provider":{}},"bindings":[],
        }))
        .unwrap();
        let routes = engarg::route_planning::compile(
            &case,
            &pending,
            &catalogue,
            &configuration,
            &DiscoverySnapshot::default(),
        )
        .unwrap();
        let config = directory.path().join("planning.json");
        write_json(&config, &configuration);
        let assessment = json!({"findings":[],"objections":[],"evidence_gaps":[]}).to_string();
        let draft = json!({"argument":case,"support_elaboration_json":serde_json::to_string(&repaired).unwrap()}).to_string();
        let final_content = json!({"argument":case,"support_elaboration_json":serde_json::to_string(&pending).unwrap()}).to_string();
        let original = json!({
            "schema_version":"engarg.run.v2","execution_mode":"provider_calls","mode":"hybrid",
            "input_sha256":digest(&serde_json::to_string(&request).unwrap()),
            "provider_config":{"retained":"private-provider-selection"},
            "run_config":engarg::engine::RunConfig {max_calls:7,max_repairs:1,..Default::default()},
            "request":request,"argument":case,"limitations":["private original limitation"],"tooling":null,
            "transcript":[
                receipt("constructor",assessment.clone()),receipt("sceptic",assessment.clone()),
                receipt("evidence_auditor",assessment.clone()),receipt("synthesiser",draft),
                receipt("final_critic",assessment),receipt("adjudicator",final_content.clone()),
                receipt("adjudicator",final_content),
            ],
            "evidence_planning":{"schema":"engarg.support-planning/1","elaboration":pending,"routes":routes},
        });
        let input = directory.path().join("original.json");
        write_json(&input, &original);
        Self {
            directory,
            input,
            config,
            original,
            repaired,
            response_limit: None,
        }
    }

    fn response(&self) -> Value {
        json!({"argument":self.original["argument"],"support_elaboration_json":serde_json::to_string(&self.repaired).unwrap()})
    }

    fn command(&self, endpoint: &str, output: &Path, audit: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", "repair", "--input"])
            .arg(&self.input)
            .arg("--evidence-config")
            .arg(&self.config)
            .arg("--output")
            .arg(output)
            .arg("--audit")
            .arg(audit)
            .args([
                "--max-repairs",
                "1",
                "--model",
                "fixture",
                "--endpoint",
                endpoint,
                "--http-retries",
                "0",
                "--request-timeout-secs",
                "10",
                "--timeout-secs",
                "20",
                "--api-key-env",
                "ENGARG_RECOVERY_FIXTURE_KEY",
                "--progress",
            ])
            .env_remove("OPENAI_API_KEY")
            .env_remove("ENGARG_API_KEY")
            .env_remove("ENGARG_MODEL")
            .env(
                "ENGARG_RECOVERY_FIXTURE_KEY",
                "private-synthetic-credential",
            );
        if let Some(limit) = self.response_limit {
            command.arg("--max-response-bytes").arg(limit.to_string());
        }
        command
    }
}

fn events(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn read_request(stream: &mut TcpStream) -> Value {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut part = [0; 8192];
        let count = stream.read(&mut part).unwrap();
        assert_ne!(count, 0, "provider connection closed before request");
        bytes.extend_from_slice(&part[..count]);
        assert!(bytes.len() <= 2_097_152, "unbounded fixture request");
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = header
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}

struct Exchange {
    body: Value,
    before_response: Vec<Value>,
}

fn run(fixture: &Fixture, content: String, output: &Path, audit: &Path) -> (Output, Vec<Exchange>) {
    run_command(content, audit, |endpoint| {
        fixture.command(endpoint, output, audit)
    })
}

fn run_command(
    content: String,
    audit: &Path,
    command: impl FnOnce(&str) -> Command,
) -> (Output, Vec<Exchange>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let (stop, stopped) = mpsc::channel();
    let audit_path = audit.to_owned();
    let server = thread::spawn(move || {
        let mut exchanges = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            if stopped.try_recv().is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "CLI did not finish within its fixture deadline"
            );
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let body = read_request(&mut stream);
                    // This read occurs before any response bytes are sent: diagnostics
                    // must already be durable while the paid request is pending.
                    exchanges.push(Exchange {
                        body,
                        before_response: events(&audit_path),
                    });
                    let response = json!({"id":"private-recovery-request","model":"private-recovery-model","status":"completed",
                        "output":[{"type":"message","content":[{"type":"output_text","text":content}]}]}).to_string();
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("provider accept failed: {error}"),
            }
        }
        exchanges
    });
    let result = command(&endpoint).output().unwrap();
    stop.send(()).unwrap();
    (result, server.join().unwrap())
}

fn assert_checkpoint(exchange: &Exchange, fixture: &Fixture) {
    assert_eq!(exchange.before_response.len(), 2);
    assert_eq!(
        exchange.before_response[0]["event"],
        "support_recovery_start"
    );
    let request = &exchange.before_response[1];
    assert_eq!(request["event"], "repair_request");
    assert_eq!(request["request"]["role"], "adjudicator");
    let prompt: Value = serde_json::from_str(request["request"]["user"].as_str().unwrap()).unwrap();
    assert!(!prompt["validation_errors"].as_array().unwrap().is_empty());
    assert_eq!(prompt["frozen_argument"], fixture.original["argument"]);
    assert_eq!(prompt["repair_attempt"], 1);
    assert_eq!(prompt["repair_limit"], 1);
    assert!(!prompt["retained_acquisition_designs"].is_null());
    assert_eq!(exchange.body["instructions"], request["request"]["system"]);
    assert_eq!(exchange.body["input"], request["request"]["user"]);
    assert_eq!(exchange.body["max_output_tokens"], 65_536);
    assert_eq!(
        exchange.body["text"]["format"]["schema"],
        request["request"]["schema"]
    );
}

#[test]
fn recovery_reuses_seven_authoring_calls_and_checkpoints_one_new_adjudicator() {
    let fixture = Fixture::new();
    let output = fixture.directory.path().join("repaired.json");
    let audit = fixture.directory.path().join("repair.audit.jsonl");
    let original_bytes = std::fs::read(&fixture.input).unwrap();
    let content = fixture.response().to_string();
    let (result, exchanges) = run(&fixture, content.clone(), &output, &audit);
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(result.status.success(), "{stderr}");
    assert!(result.stdout.is_empty());
    assert_eq!(
        exchanges.len(),
        1,
        "previous authoring calls must not be repeated"
    );
    assert_checkpoint(&exchanges[0], &fixture);
    assert_eq!(std::fs::read(&fixture.input).unwrap(), original_bytes);
    let updated: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    for key in [
        "argument",
        "request",
        "input_sha256",
        "run_config",
        "provider_config",
    ] {
        assert_eq!(
            updated[key], fixture.original[key],
            "recovery changed {key}"
        );
    }
    let transcript = updated["transcript"].as_array().unwrap();
    assert_eq!(transcript.len(), 8);
    assert_eq!(
        &transcript[..7],
        fixture.original["transcript"].as_array().unwrap()
    );
    assert_eq!(transcript[7]["role"], "adjudicator");
    assert_eq!(transcript[7]["response"]["content"], content);
    assert_eq!(
        updated["support_recovery"]["previous_report"],
        fixture.original
    );
    assert_eq!(updated["support_recovery"]["recorded_calls"], 7);
    assert_eq!(updated["support_recovery"]["model_calls"], 1);
    assert_eq!(updated["support_recovery"]["repair_limit"], 1);
    assert_eq!(
        updated["evidence_planning"]["elaboration"],
        serde_json::to_value(&fixture.repaired).unwrap()
    );
    assert_eq!(
        updated["evidence_planning"]["routes"]["routes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    engarg::report::validate_support_recovery(&updated).unwrap();
    // Both retained copies of the paid request must agree.
    let mut tampered = updated.clone();
    tampered["support_recovery"]["provider_requests"][0]["user"] = json!("{}");
    assert!(engarg::report::validate_support_recovery(&tampered).is_err());
    tampered = updated.clone();
    tampered["transcript"][7]["prompt"]["user"] = json!("{}");
    assert!(engarg::report::validate_support_recovery(&tampered).is_err());
    tampered = updated.clone();
    tampered["support_recovery"]["provider_requests"][0]["role"] = json!("single");
    tampered["transcript"][7]["prompt"]["role"] = json!("single");
    assert!(engarg::report::validate_support_recovery(&tampered).is_err());
    tampered = updated.clone();
    tampered.as_object_mut().unwrap().remove("transcript");
    assert!(engarg::report::classify_evidence_planning(&tampered).is_err());

    // Route-only replanning must remain usable after recovery and preserve its
    // exact history while replacing only route metadata.
    let replanned_path = fixture.directory.path().join("replanned.json");
    let replanned = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "replan", "--input"])
        .arg(&output)
        .arg("--evidence-config")
        .arg(&fixture.config)
        .arg("--output")
        .arg(&replanned_path)
        .env_remove("OPENAI_API_KEY")
        .env_remove("ENGARG_API_KEY")
        .output()
        .unwrap();
    assert!(
        replanned.status.success(),
        "{}",
        String::from_utf8_lossy(&replanned.stderr)
    );
    let replanned: Value =
        serde_json::from_slice(&std::fs::read(&replanned_path).unwrap()).unwrap();
    for key in ["argument", "request", "transcript", "support_recovery"] {
        assert_eq!(replanned[key], updated[key]);
    }
    assert_eq!(replanned["replanning"]["model_calls"], 0);
    engarg::report::validate_support_recovery(&replanned).unwrap();
    engarg::report::validate_replanning(&replanned, Path::new(env!("CARGO_BIN_EXE_engarg")))
        .unwrap();
    let audit_events = events(&audit);
    assert_eq!(
        audit_events
            .iter()
            .map(|event| event["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "support_recovery_start",
            "repair_request",
            "completion",
            "success"
        ]
    );
    assert_eq!(audit_events[2]["record"], transcript[7]);
    assert!(stderr.contains("call 1 adjudicator started"));
    assert!(stderr.contains("support recovery completed"));
    assert!(
        !stderr.contains("private"),
        "private content reached progress: {stderr}"
    );
    for path in [&audit, &output] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}

#[test]
fn new_response_limit_does_not_discard_larger_retained_acquisition_designs() {
    let mut fixture = Fixture::new();
    let content = fixture.response().to_string();
    let limit = content.len() + 1024;
    fixture.response_limit = Some(limit);
    let draft = &mut fixture.original["transcript"][3];
    let mut retained = draft["response"]["content"].as_str().unwrap().to_owned();
    // Valid retained JSON can exceed the selected bound for a new response.
    retained.push_str(&" ".repeat(limit));
    assert!(retained.len() > limit);
    draft["response"]["content"] = json!(retained);
    draft["response_content_bytes"] = json!(retained.len());
    draft["response_content_sha256"] = json!(digest(&retained));
    write_json(&fixture.input, &fixture.original);
    let output = fixture.directory.path().join("bounded-repair.json");
    let audit = fixture.directory.path().join("bounded-repair.audit.jsonl");
    let (result, exchanges) = run(&fixture, content, &output, &audit);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(exchanges.len(), 1);
    let prompt: Value = serde_json::from_str(exchanges[0].body["input"].as_str().unwrap()).unwrap();
    let retained_designs = prompt["retained_acquisition_designs"]["pending_designs"]
        .as_array()
        .unwrap();
    assert!(
        retained_designs.iter().any(|design| {
            design["source_call"] == 4
                && design["missing_acquisition_ids"] == json!(["shared_source"])
        }),
        "the new-call response bound hid a paid historical acquisition design"
    );
}

#[test]
fn rejected_replies_retain_completion_and_diagnostics_without_publishing_a_report() {
    for changed_argument in [false, true] {
        let fixture = Fixture::new();
        let output = fixture.directory.path().join("rejected.json");
        let audit = fixture.directory.path().join("rejected.audit.jsonl");
        let original_bytes = std::fs::read(&fixture.input).unwrap();
        let content = if changed_argument {
            let mut response = fixture.response();
            response["argument"]["adjudication"] = json!("private changed final argument");
            response.to_string()
        } else {
            "{".into()
        };
        let (result, exchanges) = run(&fixture, content.clone(), &output, &audit);
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(
            exchanges.len(),
            1,
            "failed recovery exceeded its separate allowance"
        );
        assert_checkpoint(&exchanges[0], &fixture);
        assert_eq!(std::fs::read(&fixture.input).unwrap(), original_bytes);
        assert!(!output.exists(), "invalid recovery published a report");
        let audit_events = events(&audit);
        assert_eq!(
            audit_events
                .iter()
                .map(|event| event["event"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "support_recovery_start",
                "repair_request",
                "completion",
                "failure"
            ]
        );
        assert_eq!(audit_events[2]["record"]["response"]["content"], content);
        assert_eq!(
            audit_events[2]["record"]["response_content_sha256"],
            digest(&content)
        );
        assert!(
            stderr.contains("support repair exhausted 1 calls"),
            "{stderr}"
        );
        if changed_argument {
            assert!(
                stderr.contains("preserve frozen_argument exactly"),
                "{stderr}"
            );
        }
        assert!(
            !stderr.contains("private"),
            "private content reached progress: {stderr}"
        );
    }
}

#[test]
fn existing_report_or_audit_is_rejected_before_a_provider_request() {
    let fixture = Fixture::new();
    for existing_audit in [false, true] {
        let output = fixture
            .directory
            .path()
            .join(format!("existing-{existing_audit}.json"));
        let audit = fixture
            .directory
            .path()
            .join(format!("existing-{existing_audit}.audit.jsonl"));
        let occupied = if existing_audit { &audit } else { &output };
        std::fs::write(occupied, b"private existing artifact\n").unwrap();
        let original_bytes = std::fs::read(&fixture.input).unwrap();
        let (result, exchanges) = run(&fixture, fixture.response().to_string(), &output, &audit);
        assert!(!result.status.success());
        assert!(
            exchanges.is_empty(),
            "overwrite rejection spent a model call"
        );
        assert_eq!(
            std::fs::read(occupied).unwrap(),
            b"private existing artifact\n"
        );
        assert_eq!(std::fs::read(&fixture.input).unwrap(), original_bytes);
        let unoccupied = if existing_audit { &output } else { &audit };
        assert!(!unoccupied.exists());
    }
}

fn initial_authoring_checkpoint(fixture: &Fixture) -> PathBuf {
    let request = serde_json::from_value(fixture.original["request"].clone()).unwrap();
    let config = serde_json::from_value(fixture.original["run_config"].clone()).unwrap();
    let mut checkpoint = engarg::authoring_checkpoint::Checkpoint::new(
        request,
        config,
        json!({"model":"original"}),
        engarg::authoring_checkpoint::digest(std::fs::read(&fixture.config).unwrap()),
    );
    checkpoint.transcript = serde_json::from_value(fixture.original["transcript"].clone()).unwrap();
    for call in &mut checkpoint.transcript {
        call.prompt.user = json!({"request":checkpoint.request}).to_string();
    }
    // A genuine initial-authoring failure, rather than the support-only pending
    // design exercised by the older recovery tests.
    for call in checkpoint.transcript.iter_mut().skip(5) {
        let mut envelope: Value = serde_json::from_str(&call.response.content).unwrap();
        envelope["argument"]["warrants"][2]["rule"] = json!("Altered challenged warrant");
        call.response.content = envelope.to_string();
        call.response_content_bytes = call.response.content.len();
        call.response_content_sha256 =
            engarg::authoring_checkpoint::digest(call.response.content.as_bytes());
    }
    let path = fixture.directory.path().join("authoring-checkpoint.json");
    write_json(&path, &checkpoint);
    path
}
fn resume_command(
    fixture: &Fixture,
    checkpoint: &Path,
    endpoint: &str,
    output: &Path,
    audit: &Path,
    next: &Path,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .args(["authoring", "resume", "--checkpoint"])
        .arg(checkpoint)
        .arg("--evidence-config")
        .arg(&fixture.config)
        .arg("--output")
        .arg(output)
        .arg("--audit")
        .arg(audit)
        .arg("--next-checkpoint")
        .arg(next)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            endpoint,
            "--http-retries",
            "0",
            "--timeout-secs",
            "20",
            "--api-key-env",
            "FIXTURE_RESUME_KEY",
        ])
        .env("FIXTURE_RESUME_KEY", "fixture-private");
    command
}
#[test]
fn initial_authoring_resume_reuses_history_and_accepts_one_new_call() {
    let fixture = Fixture::new();
    let checkpoint = initial_authoring_checkpoint(&fixture);
    let output = fixture.directory.path().join("resumed.json");
    let audit = fixture.directory.path().join("resumed.audit.jsonl");
    let next = fixture.directory.path().join("resumed.checkpoint.json");
    let (result, exchanges) = run_command(fixture.response().to_string(), &audit, |endpoint| {
        resume_command(&fixture, &checkpoint, endpoint, &output, &audit, &next)
    });
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(exchanges.len(), 1);
    let mut report: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    engarg::authoring_checkpoint::validate_recovery_report(&report).unwrap();
    assert_eq!(report["transcript"].as_array().unwrap().len(), 8);
    assert_eq!(report["authoring_recovery"]["new_model_calls"], 1);
    assert!(
        report["authoring_recovery"]["previous_checkpoint"]["transcript"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let original_prefix = report["transcript"][0].clone();
    report["transcript"][0]["response"]["content"] = json!("tampered prefix");
    assert!(engarg::authoring_checkpoint::validate_recovery_report(&report).is_err());
    report["transcript"][0] = original_prefix;
    let saved = engarg::authoring_checkpoint::Checkpoint::read(&next).unwrap();
    assert_eq!(saved.transcript.len(), 8);
    assert_eq!(events(&audit)[0]["recorded_calls"], 7);
    assert_eq!(exchanges[0].before_response[1]["event"], "resume_request");
    report["authoring_recovery"]["new_model_calls"] = json!(2);
    assert!(engarg::authoring_checkpoint::validate_recovery_report(&report).is_err());
}
#[test]
fn initial_authoring_invalid_repair_keeps_checkpoint_without_report() {
    let fixture = Fixture::new();
    let checkpoint = initial_authoring_checkpoint(&fixture);
    let output = fixture.directory.path().join("rejected-resume.json");
    let audit = fixture.directory.path().join("rejected-resume.audit.jsonl");
    let next = fixture
        .directory
        .path()
        .join("rejected-resume.checkpoint.json");
    let (result, exchanges) = run_command("{}".into(), &audit, |endpoint| {
        resume_command(&fixture, &checkpoint, endpoint, &output, &audit, &next)
    });
    assert!(!result.status.success());
    assert_eq!(exchanges.len(), 1);
    assert!(!output.exists());
    let saved = engarg::authoring_checkpoint::Checkpoint::read(&next).unwrap();
    assert_eq!(saved.transcript.len(), 8);
    assert!(saved.failure.is_some());
}
#[test]
fn inspect_is_offline_and_protocol_migration_requires_explicit_acknowledgement() {
    let fixture = Fixture::new();
    let checkpoint = initial_authoring_checkpoint(&fixture);
    let mut saved = engarg::authoring_checkpoint::Checkpoint::read(&checkpoint).unwrap();
    saved.protocol_identity = "legacy-unrecorded".into();
    write_json(&checkpoint, &saved);
    let inspection = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["authoring", "inspect", "--checkpoint"])
        .arg(&checkpoint)
        .output()
        .unwrap();
    assert!(inspection.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&inspection.stdout).unwrap()["protocol_migration_required"],
        true
    );
    let output = fixture.directory.path().join("migration.json");
    let audit = fixture.directory.path().join("migration.audit.jsonl");
    let next = fixture.directory.path().join("migration.checkpoint.json");
    let (result, exchanges) = run_command(fixture.response().to_string(), &audit, |endpoint| {
        resume_command(&fixture, &checkpoint, endpoint, &output, &audit, &next)
    });
    assert!(!result.status.success());
    assert!(exchanges.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--accept-protocol-migration"));
    assert!(!audit.exists());
}
