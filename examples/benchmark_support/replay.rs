//! Source-bound fixture construction and a bounded local Responses double.

use super::{Result, digest, json_digest};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct Fixture {
    request: Value,
    final_case: Value,
    synthesis: Value,
    assessment: Value,
    repair: bool,
    source: Option<Value>,
    source_digest: Option<String>,
}

impl Fixture {
    pub(super) fn public(repo: &Path, workload: &str) -> Result<Self> {
        if !["claim_only", "supplied", "objections", "repair"].contains(&workload) {
            return Err("unknown workload".into());
        }
        let supplied = workload == "supplied";
        let read = |name: &str| -> Result<Value> {
            Ok(serde_json::from_slice(&fs::read(
                repo.join("examples").join(name),
            )?)?)
        };
        let mut case = read(if supplied {
            "case.json"
        } else {
            "claim-only-case.json"
        })?;
        let request = read(if supplied {
            "request.json"
        } else {
            "claim-only-request.json"
        })?;
        let request =
            serde_json::to_value(serde_json::from_value::<engarg::domain::Request>(request)?)?;
        let count = if ["objections", "repair"].contains(&workload) {
            3
        } else {
            0
        };
        let mut findings=vec![format!("Conditional reasoning only. {}",case["warrants"][0]["rule"].as_str().unwrap()),"The available observations and the proposed conditions must retain their exact evidential status.".into()];
        let objections:Vec<_>=(1..=count).map(|i|format!("Hypothetical concern {i}: {} Preserve this condition, its target, scope and proposed discriminating check.",case["defeaters"][0]["challenge"].as_str().unwrap())).collect();
        for i in 1..=count {
            findings.push(format!(
                "Conditional assessment {i}. {} {}",
                case["warrants"][0]["applicability"].as_str().unwrap(),
                case["assumptions"][0]["statement"].as_str().unwrap()
            ));
        }
        let assessment = json!({"findings":findings,"objections":objections,"evidence_gaps":[case["evidence_plan"][0]["question"]]});
        let original = case["defeaters"][0].clone();
        let need = case["evidence_plan"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["target_id"] == original["id"])
            .unwrap()
            .clone();
        for role in ["constructor", "sceptic", "auditor", "critic"] {
            for (i, challenge) in objections.iter().enumerate() {
                let id = format!("{role}-{}", i + 1);
                let mut d = original.clone();
                d["id"] = json!(id);
                d["challenge"] = json!(challenge);
                let mut n = need.clone();
                n["id"] = json!(format!("need-{id}"));
                n["target_id"] = json!(id);
                case["defeaters"].as_array_mut().unwrap().push(d);
                case["evidence_plan"].as_array_mut().unwrap().push(n);
            }
        }
        let mut synthesis = case.clone();
        synthesis["defeaters"]
            .as_array_mut()
            .unwrap()
            .retain(|d| !d["id"].as_str().unwrap().starts_with("critic-"));
        synthesis["evidence_plan"]
            .as_array_mut()
            .unwrap()
            .retain(|n| !n["target_id"].as_str().unwrap().starts_with("critic-"));
        Ok(Self {
            request,
            final_case: case,
            synthesis,
            assessment,
            repair: workload == "repair",
            source: None,
            source_digest: None,
        })
    }

    pub(super) fn retained(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        let report: Value = serde_json::from_slice(&bytes)?;
        if report["run_config"].is_null() || report["run_config"]["mode"] != report["mode"] {
            return Err("retained report needs consistent explicit RunConfig".into());
        }
        if report["transcript"]
            .as_array()
            .ok_or("transcript absent")?
            .iter()
            .any(|c| c["response_content_omitted"] == true)
        {
            return Err("retained completion omitted".into());
        }
        Ok(Self {
            request: report["request"].clone(),
            final_case: report["argument"].clone(),
            synthesis: Value::Null,
            assessment: Value::Null,
            repair: report["transcript"].as_array().unwrap().len() > 6,
            source: Some(report),
            source_digest: Some(digest(bytes)),
        })
    }

    pub(super) fn annotate(&self, row: &mut Value) {
        if let Some(source) = &self.source {
            row["source_report_sha256"] = json!(self.source_digest);
            row["source_run_config_sha256"] = json!(json_digest(&source["run_config"]));
            row["source_response_sequence_sha256"] = json!(json_digest(&json!(
                source["transcript"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| json!({"role":c["role"],"response":c["response"]}))
                    .collect::<Vec<_>>()
            )));
            row["recorded_completions_preserved"] = json!(true);
        }
    }
}

struct State {
    bodies: usize,
    calls: usize,
    case_calls: usize,
    errors: Vec<String>,
    retained: BTreeMap<String, VecDeque<Value>>,
    clauses: BTreeMap<String, Vec<String>>,
}

fn clause(system: &str) -> Result<&str> {
    Ok(system
        .rsplit_once("ROLE TASK: ")
        .ok_or("ROLE TASK clause absent")?
        .1
        .split("\n\nCONSTRUCTION LIMIT:")
        .next()
        .unwrap())
}

fn read_request(stream: &mut TcpStream) -> Result<(Value, usize)> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut bytes = Vec::new();
    let mut buf = [0_u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            return Err("truncated HTTP header".into());
        }
        bytes.extend_from_slice(&buf[..read]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if bytes.len() > 65536 {
            return Err("oversized HTTP header".into());
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end])?;
    if header
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("authorization:"))
    {
        return Err("benchmark must send no credentials".into());
    }
    let length = header
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>())
        })
        .ok_or("Content-Length absent")??;
    if length > 16 * 1_048_576 {
        return Err("oversized HTTP body".into());
    }
    while bytes.len() - header_end < length {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            return Err("truncated HTTP body".into());
        }
        bytes.extend_from_slice(&buf[..read]);
    }
    Ok((
        serde_json::from_slice(&bytes[header_end..header_end + length])?,
        length,
    ))
}

fn answer(message: &Value, fixture: &Fixture, state: &mut State) -> Result<Value> {
    let retained = fixture.source.is_some();
    let response = if retained {
        let actual = message["instructions"]
            .as_str()
            .ok_or("instructions absent")?;
        let actual = clause(actual)?;
        let matches: Vec<_> = state
            .clauses
            .iter()
            .filter(|(_, values)| values.iter().any(|value| actual.starts_with(value)))
            .map(|(role, _)| role.clone())
            .collect();
        if matches.len() != 1 {
            return Err("request does not match exactly one retained role".into());
        }
        state
            .retained
            .get_mut(&matches[0])
            .unwrap()
            .pop_front()
            .ok_or("retained role sequence exhausted")?
    } else {
        let schema = &message["text"]["format"]["schema"]["properties"];
        let is_case = schema.get("current_claim").is_some() || schema.get("argument").is_some();
        let content = if is_case {
            state.case_calls += 1;
            let mut case = if state.case_calls == 1 {
                fixture.synthesis.clone()
            } else {
                fixture.final_case.clone()
            };
            if fixture.repair && state.case_calls == 2 {
                case["evidence_plan"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|n| n["target_id"] != "auditor-3");
            }
            if schema.get("argument").is_some() {
                let plans: Vec<_> = case["evidence_plan"]
                    .as_array()
                    .ok_or("fixture evidence plan absent")?
                    .iter()
                    .map(|need| json!({"need_id":need["id"],"tool_id":"custom","arguments":[],"unresolved_arguments":["adapter"]}))
                    .collect();
                json!({"argument":case,"acquisition_plan":plans})
            } else {
                case
            }
        } else {
            fixture.assessment.clone()
        };
        json!({"content":serde_json::to_string(&content)?,"model":"deterministic-context-volume-replay"})
    };
    let mut envelope = json!({"status":"completed","model":response["model"],"output":[{"type":"message","content":[{"type":"output_text","text":response["content"]}]}]});
    for (source, target) in [("request_id", "id"), ("service_tier", "service_tier")] {
        if !response[source].is_null() {
            envelope[target] = response[source].clone();
        }
    }
    if !response["reasoning_effort"].is_null() {
        envelope["reasoning"] = json!({"effort":response["reasoning_effort"]});
    }
    let mut usage = serde_json::Map::new();
    for key in ["input_tokens", "output_tokens"] {
        if !response[key].is_null() {
            usage.insert(key.into(), response[key].clone());
        }
    }
    if !usage.is_empty() {
        envelope["usage"] = json!(usage);
    }
    Ok(envelope)
}

pub(super) struct Run {
    pub report: Value,
    pub wire_bytes: usize,
    pub elapsed: f64,
}

pub(super) fn run(
    binary: &Path,
    fixture: &Fixture,
    tool_planning: bool,
    legacy_binary: bool,
) -> Result<Run> {
    if tool_planning && fixture.source.is_some() {
        return Err("retained report replays exact legacy completions; measure planned outputs with the four public fixture workloads".into());
    }
    let mut state = State {
        bodies: 0,
        calls: 0,
        case_calls: 0,
        errors: Vec::new(),
        retained: BTreeMap::new(),
        clauses: BTreeMap::new(),
    };
    if let Some(source) = &fixture.source {
        for call in source["transcript"].as_array().unwrap() {
            let role = call["role"].as_str().unwrap();
            let value = clause(call["prompt"]["system"].as_str().unwrap())?;
            if state.clauses.get(role).is_some_and(|c| c[0] != value) {
                return Err("retained task clauses differ for same role".into());
            }
            let current = engarg::prompts::system(role.parse()?);
            state
                .clauses
                .insert(role.into(), vec![value.into(), clause(&current)?.into()]);
            state
                .retained
                .entry(role.into())
                .or_default()
                .push_back(call["response"].clone());
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let shared = Arc::new(Mutex::new(state));
    let server_state = Arc::clone(&shared);
    let server_fixture = Fixture {
        request: fixture.request.clone(),
        final_case: fixture.final_case.clone(),
        synthesis: fixture.synthesis.clone(),
        assessment: fixture.assessment.clone(),
        repair: fixture.repair,
        source: fixture.source.clone(),
        source_digest: fixture.source_digest.clone(),
    };
    let server = thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let result = (|| -> Result<()> {
                        let (message, bytes) = read_request(&mut stream)?;
                        let mut state = server_state.lock().unwrap();
                        state.bodies += bytes;
                        state.calls += 1;
                        let response =
                            serde_json::to_vec(&answer(&message, &server_fixture, &mut state)?)?;
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response.len()
                        )?;
                        stream.write_all(&response)?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        server_state.lock().unwrap().errors.push(error.to_string());
                        let _=stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(error) => {
                    server_state.lock().unwrap().errors.push(error.to_string());
                    break;
                }
            }
        }
    });
    let result = (|| -> Result<Run> {
        let directory = tempfile::tempdir()?;
        let input = directory.path().join("request.json");
        let output = directory.path().join("report.json");
        let audit = directory.path().join("audit.jsonl");
        fs::write(&input, serde_json::to_vec(&fixture.request)?)?;
        let source = fixture.source.as_ref();
        let config=source.map(|s|s["run_config"].clone()).unwrap_or(json!({"mode":"hybrid","max_calls":if fixture.repair{7}else{6},"max_repairs":if fixture.repair{1}else{0},"max_argument_depth":2,"max_input_bytes":1048576,"max_response_bytes":1048576}));
        let provider = source
            .map(|s| s["provider_config"].clone())
            .unwrap_or(Value::Null);
        let mut command = Command::new(binary);
        command
            .arg("analyse")
            .arg("--input")
            .arg(&input)
            .arg("--model")
            .arg(
                provider["default_model"]
                    .as_str()
                    .unwrap_or("benchmark-model"),
            )
            .arg("--mode")
            .arg(config["mode"].as_str().ok_or("mode absent")?)
            .args(["--api", "responses", "--endpoint"])
            .arg(format!("http://127.0.0.1:{port}/v1/responses"))
            .args(["--api-key-env", "ENGARG_BENCH_KEY"]);
        for (name, key) in [
            ("max-calls", "max_calls"),
            ("max-repairs", "max_repairs"),
            ("max-input-bytes", "max_input_bytes"),
            ("max-response-bytes", "max_response_bytes"),
        ] {
            command
                .arg(format!("--{name}"))
                .arg(config[key].to_string());
        }
        if !config["max_argument_depth"].is_null() {
            command
                .arg("--argument-depth")
                .arg(config["max_argument_depth"].to_string());
        }
        for (name, key, default) in [
            ("max-output-tokens", "max_output_tokens", 10000),
            ("request-timeout-secs", "request_timeout_secs", 30),
            ("timeout-secs", "timeout_secs", 90),
        ] {
            command
                .arg(format!("--{name}"))
                .arg(provider[key].as_u64().unwrap_or(default).to_string());
        }
        for (name, key) in [
            ("reasoning-effort", "requested_reasoning_effort"),
            ("service-tier", "requested_service_tier"),
        ] {
            if let Some(value) = provider[key].as_str() {
                command.arg(format!("--{name}")).arg(value);
            }
        }
        if let Some(roles) = provider["role_models"].as_object() {
            for (role, model) in roles {
                command
                    .arg("--role-model")
                    .arg(format!("{role}={}", model.as_str().unwrap()));
            }
        }
        if !tool_planning && !legacy_binary {
            command.arg("--no-tool-planning");
        }
        command
            .args(["--format", "json", "--audit"])
            .arg(&audit)
            .arg("--output")
            .arg(&output);
        for name in [
            "ENGARG_API_KEY",
            "OPENAI_API_KEY",
            "ENGARG_BENCH_KEY",
            "ENGARG_REASONING_EFFORT",
            "ENGARG_SERVICE_TIER",
        ] {
            command.env_remove(name);
        }
        let started = Instant::now();
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if started.elapsed() > Duration::from_secs(120) {
                child.kill()?;
                let _ = child.wait();
                return Err("CLI replay exceeded 120 seconds".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
        let result = child.wait_with_output()?;
        let elapsed = started.elapsed().as_secs_f64();
        if !result.status.success() {
            return Err(format!(
                "replay failed exit={:?} stderr_sha256={} errors={:?}",
                result.status.code(),
                digest(result.stderr),
                shared.lock().unwrap().errors
            )
            .into());
        }
        let report: Value = serde_json::from_slice(&fs::read(output)?)?;
        let events: Vec<Value> = fs::read_to_string(audit)?
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        let state = shared.lock().unwrap();
        let calls = report["transcript"]
            .as_array()
            .ok_or("report transcript absent")?;
        let expected = source
            .map(|s| s["transcript"].as_array().unwrap().len())
            .unwrap_or(if fixture.repair { 7 } else { 6 });
        if source.is_none() {
            let cases = calls.iter().filter(|call| {
                ["synthesiser", "adjudicator"].contains(&call["role"].as_str().unwrap_or(""))
            });
            for call in cases {
                let content: Value = serde_json::from_str(
                    call["response"]["content"]
                        .as_str()
                        .ok_or("case response absent")?,
                )?;
                if content.get("argument").is_some() != tool_planning {
                    return Err(
                        "observed case contract differs from requested benchmark mode".into(),
                    );
                }
            }
        }
        if report["request"] != fixture.request
            || report["argument"] != fixture.final_case
            || calls.len() != expected
            || state.calls != expected
            || events.last().ok_or("audit absent")?["event"] != "success"
            || events.iter().filter(|e| e["event"] == "completion").count() != expected
            || !state.errors.is_empty()
        {
            return Err(format!("replay acceptance invariant failed request={} argument={} calls={}/{} expected={} audit_success={} completion_events={} server_errors={:?}",report["request"]==fixture.request,report["argument"]==fixture.final_case,calls.len(),state.calls,expected,events.last().map(|e|e["event"]=="success").unwrap_or(false),events.iter().filter(|e|e["event"]=="completion").count(),state.errors).into());
        }
        if let Some(source) = source
            && (report["run_config"] != config
                || state.retained.values().any(|queue| !queue.is_empty())
                || calls
                    .iter()
                    .zip(source["transcript"].as_array().unwrap())
                    .any(|(new, old)| {
                        new["role"] != old["role"] || new["response"] != old["response"]
                    }))
        {
            return Err("retained completion or config changed".into());
        }
        Ok(Run {
            report,
            wire_bytes: state.bodies,
            elapsed,
        })
    })();
    stop.store(true, Ordering::Relaxed);
    server.join().map_err(|_| "replay server panicked")?;
    result
}
