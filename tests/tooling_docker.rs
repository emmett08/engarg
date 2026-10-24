//! Real isolated-runner coverage. CI preloads the official toolchain images.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const PYTHON_COLLECTOR: &str = r###"import json
import sys

def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result

try:
    raw = sys.stdin.buffer.read(1048577)
    if len(raw) > 1048576:
        raise ValueError("request limit")
    def reject_constant(value):
        raise ValueError("non-finite JSON constant")

    text = raw.decode("utf-8")
    decoder = json.JSONDecoder(object_pairs_hook=unique, parse_constant=reject_constant)
    request = decoder.decode(text)
    # Retain exact JSON tokens for metadata, including arbitrary integers and decimals.
    tokens = {}
    offset = 0
    while offset < len(text) and text[offset].isspace():
        offset += 1
    if offset >= len(text) or text[offset] != "{":
        raise ValueError("request object")
    offset += 1
    while True:
        while text[offset].isspace():
            offset += 1
        if text[offset] == "}":
            break
        key, offset = decoder.raw_decode(text, offset)
        while text[offset].isspace():
            offset += 1
        if text[offset] != ":":
            raise ValueError("field separator")
        offset += 1
        while text[offset].isspace():
            offset += 1
        start = offset
        _, offset = decoder.raw_decode(text, offset)
        tokens[key] = text[start:offset]
        while text[offset].isspace():
            offset += 1
        if text[offset] == "}":
            break
        if text[offset] != ",":
            raise ValueError("field separator")
        offset += 1
    required = {"evidence_id", "environment", "tool", "tool_version", "input", "context"}
    if not isinstance(request, dict) or set(request) != required:
        raise ValueError("request fields")
    for name in ["evidence_id", "environment", "tool", "tool_version"]:
        if not isinstance(request[name], str) or not request[name]:
            raise ValueError("request identity")
    if not isinstance(request["context"], dict) or not isinstance(request["input"], dict):
        raise ValueError("request objects")
    if set(request["input"]) != {"mode"}:
        raise ValueError("input fields")
    mode = request["input"]["mode"]
    if mode not in ["positive", "negative", "error"] or mode == "error":
        raise ValueError("synthetic acquisition error")
    value = json.dumps({"collected": True, "present": mode == "positive"}, separators=(",", ":"))
    acquisition = ",".join(
        json.dumps(name) + ":" + tokens[name]
        for name in ["tool", "tool_version", "input", "context"]
    )
    sys.stdout.write(
        '{"value":' + value + ',"context":' + tokens["context"]
        + ',"request":{' + acquisition + '}}\n'
    )
except (ValueError, TypeError, KeyError, UnicodeError, IndexError):
    raise SystemExit(2)
"###;

const JAVASCRIPT_COLLECTOR: &str = r###"import { readFileSync } from "node:fs";

try {
  const source = readFileSync(0, "utf8");
  if (Buffer.byteLength(source, "utf8") > 1048576) throw new Error("request limit");
  const parsed = JSON.parse(source);
  let offset = 0;
  const raw = {};
  function whitespace() {
    while (offset < source.length && /\s/.test(source[offset])) offset++;
  }
  function string() {
    const start = offset++;
    while (offset < source.length) {
      const character = source[offset++];
      if (character === "\\") offset++;
      else if (character === '"') return source.slice(start, offset);
    }
    throw new Error("unterminated string");
  }
  function value(capture = false) {
    whitespace();
    if (source[offset] === "{") {
      offset++;
      whitespace();
      const keys = new Set();
      if (source[offset] === "}") { offset++; return; }
      while (offset < source.length) {
        whitespace();
        const key = JSON.parse(string());
        if (keys.has(key)) throw new Error("duplicate key");
        keys.add(key);
        whitespace();
        if (source[offset++] !== ":") throw new Error("colon");
        whitespace();
        const start = offset;
        value();
        if (capture) raw[key] = source.slice(start, offset);
        whitespace();
        if (source[offset] === "}") { offset++; return; }
        if (source[offset++] !== ",") throw new Error("comma");
      }
      throw new Error("unterminated object");
    }
    if (source[offset] === "[") {
      offset++;
      whitespace();
      if (source[offset] === "]") { offset++; return; }
      while (offset < source.length) {
        value();
        whitespace();
        if (source[offset] === "]") { offset++; return; }
        if (source[offset++] !== ",") throw new Error("comma");
      }
      throw new Error("unterminated array");
    }
    if (source[offset] === '"') { string(); return; }
    const start = offset;
    while (offset < source.length && !/[\s,\]}]/.test(source[offset])) offset++;
    if (offset === start) throw new Error("empty value");
  }
  value(true);
  whitespace();
  if (offset !== source.length) throw new Error("trailing data");
  const required = ["evidence_id", "environment", "tool", "tool_version", "input", "context"];
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed) ||
      Object.keys(parsed).sort().join(",") !== required.sort().join(",")) {
    throw new Error("request fields");
  }
  for (const key of ["evidence_id", "environment", "tool", "tool_version"]) {
    if (typeof parsed[key] !== "string" || parsed[key].length === 0) {
      throw new Error("request identity");
    }
  }
  if (parsed.context === null || typeof parsed.context !== "object" ||
      Array.isArray(parsed.context) || parsed.input === null ||
      typeof parsed.input !== "object" || Array.isArray(parsed.input) ||
      Object.keys(parsed.input).join(",") !== "mode") {
    throw new Error("request objects");
  }
  const mode = parsed.input.mode;
  if (!["positive", "negative", "error"].includes(mode) || mode === "error") {
    throw new Error("synthetic acquisition error");
  }
  const result = '{"value":' + JSON.stringify({collected: true, present: mode === "positive"}) +
    ',"context":' + raw.context + ',"request":{"tool":' + raw.tool +
    ',"tool_version":' + raw.tool_version + ',"input":' + raw.input +
    ',"context":' + raw.context + '}}';
  process.stdout.write(result + "\n");
} catch {
  process.exitCode = 2;
}
"###;

const RUST_COLLECTOR: &str = r###"use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    evidence_id: String,
    environment: String,
    tool: String,
    tool_version: String,
    input: Input,
    context: Map<String, Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: Mode,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Positive,
    Negative,
    Error,
}

fn collect() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = Vec::new();
    std::io::stdin().lock().take(1_048_577).read_to_end(&mut raw)?;
    if raw.len() > 1_048_576 {
        return Err("request limit".into());
    }
    let request: Request = serde_json::from_slice(&raw)?;
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ].iter().any(|identity| identity.is_empty()) {
        return Err("request identity".into());
    }
    let present = match request.input.mode {
        Mode::Positive => true,
        Mode::Negative => false,
        Mode::Error => return Err("synthetic acquisition error".into()),
    };
    // The toolchain's arbitrary_precision feature retains every JSON number.
    let envelope = json!({
        "value": {"collected": true, "present": present},
        "context": request.context,
        "request": {
            "tool": request.tool,
            "tool_version": request.tool_version,
            "input": request.input,
            "context": request.context,
        },
    });
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &envelope)?;
    output.write_all(b"\n")?;
    Ok(())
}

fn main() {
    if collect().is_err() {
        std::process::exit(2);
    }
}
"###;

struct MockServer {
    endpoint: String,
    stopped: Arc<AtomicBool>,
    received: Arc<Mutex<Vec<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn new(responses: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let received = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::clone(&stopped);
        let requests = Arc::clone(&received);
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(120);
            while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let request = match read_request(&mut stream) {
                            Ok(request) => request,
                            Err(_) => continue,
                        };
                        let index = {
                            let mut list = requests.lock().unwrap();
                            let index = list.len();
                            list.push(request);
                            index
                        };
                        let response = responses
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| json!({"unexpected_model_call": true}));
                        let body = json!({
                            "id": format!("tool-fixture-{index}"),
                            "model": "fixture",
                            "status": "completed",
                            "output": [{
                                "type": "message",
                                "content": [{"type": "output_text", "text": response.to_string()}]
                            }]
                        })
                        .to_string();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            endpoint,
            stopped,
            received,
            worker: Some(worker),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.received.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Value> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "request body"));
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "request limit"));
        }
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if length > 2 * 1024 * 1024 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "request limit"));
            }
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length])
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
            }
        }
    }
}

fn draft_response() -> Value {
    let positive = json!({"collected": true, "present": true});
    let negative = json!({"collected": true, "present": false});
    json!({
        "purpose": "Observe a synthetic local instrument while retaining protocol identity",
        "parameters": ["mode"],
        "input_schema_json": json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"mode": {"type": "string", "enum": ["positive", "negative", "error"]}},
            "required": ["mode"]
        }).to_string(),
        "result_schema_json": json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"collected": {"type": "boolean"}, "present": {"type": "boolean"}},
            "required": ["collected", "present"]
        }).to_string(),
        "arguments": [{"name": "mode", "value": "positive"}],
        "unresolved_arguments": [],
        "fixtures": [
            {
                "name": "observed_positive",
                "kind": "positive",
                "input_json": json!({"mode": "positive"}).to_string(),
                "expected_value_json": positive.to_string(),
                "files": []
            },
            {
                "name": "observed_negative",
                "kind": "negative",
                "input_json": json!({"mode": "negative"}).to_string(),
                "expected_value_json": negative.to_string(),
                "files": []
            },
            {
                "name": "failed_acquisition",
                "kind": "acquisition_error",
                "input_json": json!({"mode": "error"}).to_string(),
                "expected_value_json": null,
                "files": []
            }
        ]
    })
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn saved_report(path: &Path) {
    let request: Value = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let mut argument: Value = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let first = &mut argument["evidence_plan"][0];
    first["question"] = json!("Does the synthetic local instrument report presence?");
    first["method"] =
        json!("Read the configured local instrument mode and retain the measured result.");
    first["measurement"] = json!("One categorical presence measurement.");
    first["comparator"] =
        json!("Positive and negative instrument states with an acquisition error.");
    first["acceptance_criterion"] =
        json!("The acquired observation retains its exact request identity.");
    let plans: Vec<_> = argument["evidence_plan"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, need)| {
            if index == 0 {
                json!({
                    "need_id": need["id"],
                    "tool_id": "custom",
                    "arguments": [],
                    "unresolved_arguments": ["adapter"]
                })
            } else {
                json!({
                    "need_id": need["id"],
                    "tool_id": "artifact_json",
                    "arguments": [],
                    "unresolved_arguments": ["inputs"]
                })
            }
        })
        .collect();
    let content = json!({"argument": argument, "acquisition_plan": plans}).to_string();
    let typed_request: engarg::domain::Request = serde_json::from_value(request.clone()).unwrap();
    let run_config = engarg::engine::RunConfig {
        mode: engarg::engine::Mode::Single,
        max_calls: 1,
        ..engarg::engine::RunConfig::default()
    };
    let report = json!({
        "schema_version": "engarg.run.v2",
        "execution_mode": "offline_synthetic_fixture",
        "input_sha256": digest(&serde_json::to_vec(&typed_request).unwrap()),
        "mode": "single",
        "run_config": run_config,
        "provider_config": null,
        "limitations": ["Synthetic argument used to exercise isolated tooling implementation."],
        "request": request,
        "argument": argument,
        "transcript": [{
            "role": "single",
            "prompt": {"role": "single", "system": "fixture", "user": "{}", "schema": {}},
            "response": {
                "content": content, "model": "fixture", "input_tokens": null,
                "output_tokens": null, "request_id": null, "service_tier": null,
                "reasoning_effort": null
            },
            "response_content_bytes": content.len(),
            "response_content_sha256": digest(content.as_bytes()),
            "response_content_omitted": false,
            "elapsed_ms": 0
        }]
    });
    fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

fn profile(path: &Path, language: &str, image: &str, entry: &str) {
    if language == "rust" {
        let mut value: Value =
            serde_json::from_str(include_str!("../examples/toolchains/rust.json")).unwrap();
        assert_eq!(value["image"], image);
        value["docker"] =
            json!(std::env::var("ENGARG_TOOL_DOCKER").unwrap_or_else(|_| "docker".into()));
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        return;
    }
    let build = if language == "python" {
        json!([
            "python3",
            "-I",
            "-B",
            "-c",
            "__import__('ast').parse(open('/src/collector.py', encoding='utf-8').read())"
        ])
    } else {
        json!(["node", "--check", "/src/collector.mjs"])
    };
    let run = if language == "python" {
        json!(["python3", "-I", "-B", format!("/src/{entry}")])
    } else {
        json!(["node", format!("/src/{entry}")])
    };
    let value = json!({
        "schema": "engarg.toolchain/1",
        "language": language,
        "docker": std::env::var("ENGARG_TOOL_DOCKER").unwrap_or_else(|_| "docker".into()),
        "image": image,
        "build_argv": build,
        "run_argv": run,
        "inherit_env": [],
        "runtime_network": "none",
        "timeout_seconds": 30,
        "memory_mb": 128,
        "output_bytes": 1_048_576
    });
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn implement(
    input: &Path,
    profile: &Path,
    store: &Path,
    output: &Path,
    endpoint: &str,
    language: &str,
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "implement", "--input"])
        .arg(input)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            endpoint,
            "--api-key-env",
            "UNSET_TOOL_TEST_KEY",
        ])
        .args(["--tool-language", language, "--toolchain"])
        .arg(profile)
        .arg("--tool-store")
        .arg(store)
        .arg("--tool-output-dir")
        .arg(output)
        .args([
            "--tool-max-calls",
            "2",
            "--tool-timeout-secs",
            "120",
            "--http-retries",
            "0",
        ])
        .env_remove("UNSET_TOOL_TEST_KEY")
        .output()
        .unwrap()
}

fn successful(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn replay_synthetic_fixture(attempt: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use engarg::tooling::{
        ToolchainProfile,
        contracts::{CodeCandidate, NeedContract},
        runner::{DockerRunner, ExecutionMode, FixtureRunner},
    };

    let contract: NeedContract = serde_json::from_slice(&fs::read(attempt.join("contract.json"))?)?;
    let candidate: CodeCandidate =
        serde_json::from_slice(&fs::read(attempt.join("candidate.json"))?)?;
    let profile = ToolchainProfile::load(&attempt.join("profile.json"))?;
    let supervisor =
        Path::new(env!("CARGO_BIN_EXE_engarg")).with_file_name("engarg-process-supervisor");
    let runner = DockerRunner::new(profile, supervisor)?;
    let fixture = contract
        .draft
        .fixtures
        .first()
        .ok_or("synthetic contract has no fixture")?;
    let precision: Value = serde_json::from_str(
        r#"{"integer":90071992547409931234567890,"decimal":1.234567890123456789,"escaped":"quote \" slash \\ CRLF\r\n\u2028"}"#,
    )?;
    let request = json!({
        "evidence_id": contract.need.id,
        "environment": "engarg_tool_fixture",
        "tool": "engarg_generated_fixture",
        "tool_version": "1",
        "input": fixture.input,
        "context": {"precision": precision, "fixture": fixture.name},
    });
    let outcome = runner.execute(
        attempt,
        &candidate.files,
        &fixture.files,
        &serde_json::to_vec(&request)?,
        ExecutionMode::Fixture,
        &eal_process::CancellationToken::default(),
    )?;
    // This test supplies only frozen synthetic data and forwards no credentials.
    // Production receipts intentionally retain stream digests rather than content.
    let limit = 65_536;
    Ok(format!(
        "synthetic replay: returncode={} execution_error={:?} elapsed_ms={}\nstdout (first {limit} bytes):\n{}\nstderr (first {limit} bytes):\n{}",
        outcome.returncode,
        outcome.execution_error,
        outcome.elapsed_ms,
        String::from_utf8_lossy(&outcome.stdout[..outcome.stdout.len().min(limit)]),
        String::from_utf8_lossy(&outcome.stderr[..outcome.stderr.len().min(limit)]),
    ))
}

fn successful_implementation(output: &Output, store: &Path) {
    if output.status.success() {
        return;
    }
    let mut diagnostics = String::new();
    if let Ok(attempts) = fs::read_dir(store.join("attempts")) {
        for attempt in attempts.take(8).flatten() {
            let attempt = attempt.path();
            if let Ok(bytes) = fs::read(attempt.join("test-results.json")) {
                diagnostics.push_str(&format!(
                    "\n{}:\n{}\n",
                    attempt.join("test-results.json").display(),
                    String::from_utf8_lossy(&bytes[..bytes.len().min(65_536)]),
                ));
            }
            match replay_synthetic_fixture(&attempt) {
                Ok(replay) => diagnostics.push_str(&replay),
                Err(error) => diagnostics.push_str(&format!("synthetic replay failed: {error}")),
            }
        }
    }
    panic!(
        "stdout:\n{}\nstderr:\n{}\nretained fixture diagnostics:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        diagnostics,
    );
}

fn export_manifest(report: &Path, directory: &Path) -> (String, PathBuf) {
    let source = directory.join("argument.eal");
    let tools = directory.join("argument.tools.toml");
    let bundle = directory.join("argument.bundle.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "export", "--input"])
        .arg(report)
        .arg("--output")
        .arg(&source)
        .arg("--tools-output")
        .arg(&tools)
        .arg("--manifest")
        .arg(bundle)
        .output()
        .unwrap();
    successful(&output);
    let registry: toml::Value = toml::from_str(&fs::read_to_string(tools).unwrap()).unwrap();
    for (name, binding) in registry["tools"].as_table().unwrap() {
        let Some(argv) = binding.get("argv").and_then(toml::Value::as_array) else {
            continue;
        };
        if argv.iter().any(|value| value.as_str() == Some("tool-run")) {
            assert_eq!(binding["kind"].as_str(), Some("command"));
            let path = PathBuf::from(argv.last().unwrap().as_str().unwrap());
            assert!(path.is_file());
            assert!(fs::read_to_string(&source).unwrap().contains(name));
            return (name.clone(), path);
        }
    }
    panic!("verified generated collector was absent from exported registry");
}

fn invoke(manifest: &Path, request: &Value) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "tool-run", "--package"])
        .arg(manifest)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn exercise(language: &str, image: &str, entry: &str, code: &str) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    let toolchain = directory.path().join("toolchain.json");
    let store = directory.path().join("store");
    let first = directory.path().join("first");
    saved_report(&input);
    profile(&toolchain, language, image, entry);
    let server = MockServer::new(vec![
        draft_response(),
        json!({"language": language, "files": [{"path": entry, "content": code}]}),
    ]);
    let output = implement(
        &input,
        &toolchain,
        &store,
        &first,
        &server.endpoint,
        language,
    );
    successful_implementation(&output, &store);
    assert_eq!(server.requests().len(), 2);
    let report_path = first.join("analysis.with-tools.json");
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["tooling"]["language"], language);
    assert_eq!(report["tooling"]["model_calls"], 2);
    assert_eq!(report["transcript"].as_array().unwrap().len(), 1);
    let (tool_name, manifest) = export_manifest(&report_path, &first);
    let receipt: Value = serde_json::from_slice(
        &fs::read(manifest.parent().unwrap().join("test-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["passed"], true);
    assert_eq!(receipt["language"], language);
    let results = receipt["results"].as_array().unwrap();
    assert!(results.len() >= 6);
    assert!(results.iter().all(|result| result["passed"] == true));
    for field in [
        "profile_sha256",
        "docker_sha256",
        "runner_sha256",
        "supervisor_sha256",
    ] {
        assert_eq!(receipt["runner"][field].as_str().unwrap().len(), 64);
    }
    assert!(
        receipt["runner"]["image_id"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );

    // A closed endpoint makes any accidental coding call fail this cached retry.
    let second = directory.path().join("second");
    let output = implement(
        &input,
        &toolchain,
        &store,
        &second,
        "http://127.0.0.1:1/v1/responses",
        language,
    );
    successful_implementation(&output, &store);
    let repeated: Value =
        serde_json::from_slice(&fs::read(second.join("analysis.with-tools.json")).unwrap())
            .unwrap();
    assert_eq!(repeated["tooling"]["model_calls"], 0);
    assert!(
        repeated["tooling"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|package| package["reused"] == true)
    );
    let (reused_tool, reused_manifest) =
        export_manifest(&second.join("analysis.with-tools.json"), &second);
    assert_eq!(reused_tool, tool_name);
    assert_eq!(reused_manifest, manifest);
    assert_eq!(server.requests().len(), 2);

    // Verified cache reuse also works with the default provider and no key.
    let third = directory.path().join("without-credentials");
    let output = implement(
        &input,
        &toolchain,
        &store,
        &third,
        "https://api.openai.com/v1/responses",
        language,
    );
    successful_implementation(&output, &store);
    let unkeyed: Value =
        serde_json::from_slice(&fs::read(third.join("analysis.with-tools.json")).unwrap()).unwrap();
    assert_eq!(unkeyed["tooling"]["model_calls"], 0);
    assert!(
        unkeyed["tooling"]["packages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|package| package["reused"] == true)
    );
    let (unkeyed_tool, unkeyed_manifest) =
        export_manifest(&third.join("analysis.with-tools.json"), &third);
    assert_eq!(unkeyed_tool, tool_name);
    assert_eq!(unkeyed_manifest, manifest);
    assert_eq!(server.requests().len(), 2);

    let context: Value = serde_json::from_str(
        r#"{"exact_integer":90071992547409931234567890,"exact_decimal":1.234567890123456789,"escaped_text":"A quote \" and backslash \\ with \r\n and \u2028."}"#,
    )
    .unwrap();
    let request = json!({
        "evidence_id": tool_name.replacen("acquisition_tool_", "acquisition_", 1),
        "environment": "engarg_case",
        "tool": tool_name,
        "tool_version": "1",
        "input": {"mode": "positive"},
        "context": context
    });
    let output = invoke(&manifest, &request);
    successful(&output);
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        envelope["value"],
        json!({"collected": true, "present": true})
    );
    assert_eq!(envelope["context"], request["context"]);
    assert_eq!(
        envelope["request"],
        json!({
            "tool": request["tool"],
            "tool_version": request["tool_version"],
            "input": request["input"],
            "context": request["context"]
        })
    );

    let mut aliased = request.clone();
    aliased["tool"] = json!("operator_registered_synthetic");
    aliased["evidence_id"] = json!("synthetic_measurement");
    let output = invoke(&manifest, &aliased);
    successful(&output);
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["request"]["tool"], aliased["tool"]);
    assert_eq!(envelope["context"], aliased["context"]);

    // A changed retained source cannot inherit the old test receipt.
    let candidate_path = manifest.parent().unwrap().join("candidate.json");
    let original_candidate = fs::read(&candidate_path).unwrap();
    let mut candidate: Value = serde_json::from_slice(&original_candidate).unwrap();
    candidate["files"][0]["content"] = json!("unverified changed source");
    fs::write(&candidate_path, serde_json::to_vec(&candidate).unwrap()).unwrap();
    let changed = invoke(&manifest, &request);
    assert!(!changed.status.success());
    assert!(changed.stdout.is_empty());

    fs::write(&candidate_path, original_candidate).unwrap();
    let source_path = manifest.parent().unwrap().join("source").join(entry);
    fs::remove_file(&source_path).unwrap();
    fs::write(&source_path, "unverified replaced implementation").unwrap();
    let changed = invoke(&manifest, &request);
    assert!(!changed.status.success());
    assert!(changed.stdout.is_empty());
}

#[test]
#[ignore = "requires Docker and the preloaded python:3.13-slim image"]
fn python_collector_is_tested_registered_and_reused_without_new_coding() {
    exercise(
        "python",
        "python:3.13-slim",
        "collector.py",
        PYTHON_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the preloaded node:24-slim image"]
fn javascript_collector_preserves_exact_protocol_identity_and_reuses_verified_code() {
    exercise(
        "javascript",
        "node:24-slim",
        "collector.mjs",
        JAVASCRIPT_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the preloaded engarg-rust-tools:1.99.1 image"]
fn rust_collector_is_compiled_registered_and_reused_with_exact_protocol_identity() {
    exercise(
        "rust",
        "engarg-rust-tools:1.99.1",
        "main.rs",
        RUST_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the preloaded python:3.13-slim image"]
fn coding_failure_preserves_the_original_analysis_output() {
    let directory = tempfile::tempdir().unwrap();
    let fixture_path = directory.path().join("fixture.json");
    saved_report(&fixture_path);
    let fixture: Value = serde_json::from_slice(&fs::read(&fixture_path).unwrap()).unwrap();
    let planned: Value = serde_json::from_str(
        fixture["transcript"][0]["response"]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let request_path = directory.path().join("request.json");
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&fixture["request"]).unwrap(),
    )
    .unwrap();
    let toolchain = directory.path().join("toolchain.json");
    profile(&toolchain, "python", "python:3.13-slim", "collector.py");
    let tool_store = directory.path().join("store");
    let tool_output = directory.path().join("tools");
    let normal_output = directory.path().join("analysis.json");
    let server = MockServer::new(vec![
        planned,
        draft_response(),
        json!({
            "language": "python",
            "files": [{"path": "collector.py", "content": "def invalid_python(:\n"}]
        }),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--input"])
        .arg(&request_path)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            &server.endpoint,
            "--api-key-env",
            "UNSET_TOOL_TEST_KEY",
            "--mode",
            "single",
            "--max-calls",
            "1",
            "--implement-tools",
            "--tool-language",
            "python",
            "--toolchain",
        ])
        .arg(&toolchain)
        .arg("--tool-store")
        .arg(&tool_store)
        .arg("--tool-output-dir")
        .arg(&tool_output)
        .args([
            "--tool-max-calls",
            "2",
            "--tool-timeout-secs",
            "120",
            "--http-retries",
            "0",
            "--format",
            "json",
            "--output",
        ])
        .arg(&normal_output)
        .env_remove("UNSET_TOOL_TEST_KEY")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "invalid Python unexpectedly passed collector admission"
    );
    assert_eq!(
        server.requests().len(),
        3,
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let original: Value = serde_json::from_slice(&fs::read(&normal_output).unwrap()).unwrap();
    assert_eq!(original["request"], fixture["request"]);
    assert_eq!(original["argument"], fixture["argument"]);
    assert_eq!(original["transcript"].as_array().unwrap().len(), 1);
    assert!(original.get("tooling").is_none());
    let checkpoint: Value =
        serde_json::from_slice(&fs::read(tool_output.join("argument.analysis.json")).unwrap())
            .unwrap();
    assert_eq!(checkpoint["argument"], original["argument"]);
    assert!(checkpoint.get("tooling").is_none());
    assert!(!tool_output.join("analysis.with-tools.json").exists());
}

#[test]
#[ignore = "requires Docker and the preloaded python:3.13-slim image"]
fn coding_cancellation_removes_the_running_container_without_registering() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    let toolchain = directory.path().join("toolchain.json");
    let store = directory.path().join("store");
    let output_dir = directory.path().join("tools");
    saved_report(&input);
    profile(&toolchain, "python", "python:3.13-slim", "collector.py");
    let mut settings: Value = serde_json::from_slice(&fs::read(&toolchain).unwrap()).unwrap();
    settings["timeout_seconds"] = json!(120);
    fs::write(&toolchain, serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let server = MockServer::new(vec![
        draft_response(),
        json!({
            "language": "python",
            "files": [{"path": "collector.py", "content": "import time\ntime.sleep(120)\n"}]
        }),
    ]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "implement", "--input"])
        .arg(&input)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            &server.endpoint,
            "--api-key-env",
            "UNSET_TOOL_TEST_KEY",
            "--tool-language",
            "python",
            "--toolchain",
        ])
        .arg(&toolchain)
        .arg("--tool-store")
        .arg(&store)
        .arg("--tool-output-dir")
        .arg(&output_dir)
        .args([
            "--tool-max-calls",
            "2",
            "--tool-timeout-secs",
            "120",
            "--http-retries",
            "0",
        ])
        .env_remove("UNSET_TOOL_TEST_KEY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let launcher_pid = child.id();
    let prefix = format!("engarg-{launcher_pid}-");
    let docker = std::env::var("ENGARG_TOOL_DOCKER").unwrap_or_else(|_| "docker".into());
    let startup_deadline = Instant::now() + Duration::from_secs(45);
    let observed_running = loop {
        let running = Command::new(&docker)
            .args(["container", "ls", "--filter"])
            .arg(format!("name={prefix}"))
            .args(["--format", "{{.Names}}"])
            .output()
            .unwrap();
        successful(&running);
        if String::from_utf8_lossy(&running.stdout)
            .lines()
            .any(|name| name.starts_with(&prefix))
        {
            break true;
        }
        if child.try_wait().unwrap().is_some() || Instant::now() >= startup_deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(200));
    };

    let cancellation_started = Instant::now();
    if child.try_wait().unwrap().is_none() {
        let signal = Command::new("/bin/kill")
            .args(["-TERM", &launcher_pid.to_string()])
            .output()
            .unwrap();
        successful(&signal);
    }
    let exited_in_time = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if cancellation_started.elapsed() >= Duration::from_secs(20) {
            break false;
        }
        thread::sleep(Duration::from_millis(50));
    };
    if !exited_in_time {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();
    let remaining = Command::new(&docker)
        .args(["container", "ls", "--all", "--filter"])
        .arg(format!("name={prefix}"))
        .args(["--format", "{{.Names}}"])
        .output()
        .unwrap();
    successful(&remaining);
    let names = String::from_utf8_lossy(&remaining.stdout);
    if !names.trim().is_empty() {
        // A failing regression must still clean up its own daemon resources.
        for name in names.lines().filter(|name| name.starts_with(&prefix)) {
            let _ = Command::new(&docker)
                .args(["container", "rm", "--force", name])
                .output();
        }
    }
    assert!(
        exited_in_time,
        "launcher did not finish within 20 seconds of SIGTERM"
    );
    assert!(
        observed_running,
        "no running collector container was observed for {prefix}; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "cancelled candidate unexpectedly passed collector admission"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("tool implementation terminated"),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(server.requests().len(), 2);
    assert!(
        names.trim().is_empty(),
        "cancelled launcher left containers:\n{names}"
    );
    assert!(output_dir.join("argument.analysis.json").is_file());
    let audit = fs::read_to_string(output_dir.join("tooling.audit.jsonl")).unwrap();
    assert!(audit.lines().any(|line| {
        let event: Value = serde_json::from_str(line).unwrap();
        event["event"] == "cancelled" && event["passed"] == false
    }));
    for file in [
        "analysis.with-tools.json",
        "collectors.json",
        "tooling.json",
    ] {
        assert!(!output_dir.join(file).exists());
    }
    assert_eq!(fs::read_dir(store.join("index")).unwrap().count(), 0);
    let attempts: Vec<_> = fs::read_dir(store.join("attempts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(attempts.len(), 1);
    assert!(attempts[0].join("candidate.json").is_file());
    assert!(!attempts[0].join("manifest.json").exists());
}
