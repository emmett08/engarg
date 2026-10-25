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

#[allow(dead_code)]
#[path = "fixtures/tooling_collector.rs"]
mod rust_collector_fixture;
#[path = "common/support.rs"]
mod support_fixture;

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
    value = json.dumps({"collected": True, "present": mode == "positive", "mode": mode}, separators=(",", ":"))
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
  const result = '{"value":' + JSON.stringify({collected: true, present: mode === "positive", mode}) +
    ',"context":' + raw.context + ',"request":{"tool":' + raw.tool +
    ',"tool_version":' + raw.tool_version + ',"input":' + raw.input +
    ',"context":' + raw.context + '}}';
  process.stdout.write(result + "\n");
} catch {
  process.exitCode = 2;
}
"###;

const RUST_COLLECTOR: &str = include_str!("fixtures/tooling_collector.rs");

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
    let positive = json!({"collected": true, "present": true,"mode":"positive"});
    let negative = json!({"collected": true, "present": false,"mode":"negative"});
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
            "properties": {"collected": {"type": "boolean"}, "present": {"type": "boolean"},"mode":{"type":"string","enum":["positive","negative","error"]}},
            "required": ["collected", "present","mode"]
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
    let precision = engarg::tooling::verification::protocol_precision_context()?;
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

    let context = engarg::tooling::verification::protocol_precision_context().unwrap();
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
    let envelope =
        eal_collector_contracts::strict_json_exact(&output.stdout, Default::default()).unwrap();
    assert_eq!(
        envelope["value"],
        json!({"collected": true, "present": true,"mode":"positive"})
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
    let envelope =
        eal_collector_contracts::strict_json_exact(&output.stdout, Default::default()).unwrap();
    assert_eq!(envelope["request"]["tool"], aliased["tool"]);
    assert_eq!(envelope["context"], aliased["context"]);

    // Native EAL source IDs are opaque canonical digests, including digit-leading
    // values. Exercise this transport shape independently of a random source hash.
    aliased["evidence_id"] = json!("0".repeat(64));
    let output = invoke(&manifest, &aliased);
    successful(&output);
    let envelope =
        eal_collector_contracts::strict_json_exact(&output.stdout, Default::default()).unwrap();
    assert_eq!(envelope["request"]["input"], aliased["input"]);
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

#[tokio::test]
#[ignore = "requires Docker and the preloaded engarg-rust-tools:1.99.1 image"]
async fn capability_contract_generates_rust_and_reuses_it_for_different_claims() {
    use eal_collector_contracts::capability::{
        LogicalStrength, ProvenanceRequirement, SUPPORT_CONTRACT_SCHEMA, SubjectScope,
        SupportContract, SupportType, TimeRequirement,
    };
    use engarg::{
        provider::{Completion, CompletionRequest, LanguageModel},
        tooling::{
            ToolchainProfile,
            capability::{
                CapabilityImplementationConfig, CapabilityImplementationRequest,
                ImplementationGapKind, implement_capability,
            },
            execute_registered,
        },
    };
    struct Model(Mutex<std::collections::VecDeque<String>>);
    impl LanguageModel for Model {
        async fn complete(&self, _request: CompletionRequest) -> Result<Completion, engarg::Error> {
            Ok(Completion {
                content: self
                    .0
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected coding call"),
                model: "docker-function-contract-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let toolchain = directory.path().join("toolchain.json");
    profile(&toolchain, "rust", "engarg-rust-tools:1.99.1", "main.rs");
    let mut draft = draft_response();
    draft["arguments"] = json!([]);
    draft["unresolved_arguments"] = json!(["mode"]);
    let request = CapabilityImplementationRequest {
        support_contract: SupportContract {
            schema: SUPPORT_CONTRACT_SCHEMA.into(),
            support_type: SupportType::Calculation,
            scope: SubjectScope {
                kind: "synthetic_flag".into(),
                subject: "Caller-supplied mode".into(),
                selectors: Default::default(),
                identity_dimensions: vec!["mode".into()],
            },
            acceptable_methods: vec!["synthetic_flag/1".into()],
            quantities: Vec::new(),
            logical_strength: LogicalStrength::ExactCalculation,
            population: None,
            time: TimeRequirement {
                historical_interval: None,
                max_age_seconds: None,
                max_uncertainty_seconds: None,
                require_original_observation_time: false,
            },
            provenance: ProvenanceRequirement {
                required_authorities: Vec::new(),
                transformation_history: false,
            },
            expected_output_schema_sha256: None,
        },
        input_schema: serde_json::from_str(draft["input_schema_json"].as_str().unwrap()).unwrap(),
        output_schema: serde_json::from_str(draft["result_schema_json"].as_str().unwrap()).unwrap(),
        operation_purpose: draft["purpose"].as_str().unwrap().into(),
        declared_limits: Default::default(),
    };
    let mut selected = CapabilityImplementationConfig {
        language: "rust".into(),
        profile: ToolchainProfile::load(&toolchain).unwrap(),
        store: directory.path().join("store"),
        max_calls: 2,
        max_input_bytes: 1_048_576,
        max_response_bytes: 1_048_576,
        supervisor: PathBuf::from(env!("CARGO_BIN_EXE_engarg-process-supervisor")),
        engarg_executable: std::env::current_exe().unwrap(),
    };
    let model = Model(Mutex::new(std::collections::VecDeque::from([
        draft.to_string(),
        json!({"language":"rust","files":[{"path":"main.rs","content":RUST_COLLECTOR}]})
            .to_string(),
    ])));
    let first = implement_capability(
        &request,
        ImplementationGapKind::Operation,
        selected.clone(),
        &model,
    )
    .await
    .unwrap();
    assert_eq!(first.model_calls, 2);
    assert!(first.capability.semantics.is_none());
    selected.max_calls = 0;
    let second = implement_capability(
        &request,
        ImplementationGapKind::Operation,
        selected.clone(),
        &model,
    )
    .await
    .unwrap();
    assert!(second.reused);
    assert_eq!(second.model_calls, 0);
    assert_eq!(first.package_manifest, second.package_manifest);
    for (id, alias, mode, criterion) in [
        ("claim_a_need", "claim_a_tool", "positive", "p99 < 7"),
        (
            "claim_b_need",
            "claim_b_tool",
            "negative",
            "population == 9",
        ),
    ] {
        let mut context = serde_json::from_str::<Value>(&format!(
            r#"{{"criterion":"{criterion}","integer":1234567890123456789012345678901234567890,"decimal":0.123456789012345678901234567890}}"#
        )).unwrap();
        context["opaque_protocol_metadata"] =
            engarg::tooling::verification::protocol_precision_context().unwrap();
        let runtime = json!({"evidence_id":id,"environment":"development","tool":alias,
            "tool_version":"1","input":{"mode":mode},"context":context});
        let observation = execute_registered(
            &second.package_manifest,
            &serde_json::to_vec(&runtime).unwrap(),
            &selected.supervisor,
        )
        .unwrap();
        assert_eq!(observation["context"], context);
        assert_eq!(observation["request"]["context"], context);
        assert_eq!(observation["request"]["tool"], alias);
        assert_eq!(observation["request"]["input"], runtime["input"]);
        assert_eq!(observation["value"]["present"], mode == "positive");
    }
    assert!(model.0.lock().unwrap().is_empty());
}

fn generated_operation_fixture(
    root: &Path,
    collector: &Path,
) -> (Value, engarg::route_planning::PlanningConfig, Value) {
    use eal_collector_contracts::{
        capability::{CAPABILITY_CATALOGUE_SCHEMA, CapabilityCatalogue},
        execution::SchedulingPolicy,
    };
    use engarg::{
        route_planning::{self, DiscoverySnapshot, PlanningConfig},
        support_requirements::validate_elaboration,
    };
    let (request, case, mut elaboration, _, _, _) = support_fixture::fixture();
    for alternative in &mut elaboration.alternative_support {
        alternative.claim_ids = vec!["current_claim".into(), "stronger_target".into()];
    }
    let mut contract = elaboration.acquisitions[0].contract.clone();
    contract.scope.kind = "synthetic_flag".into();
    contract.scope.subject = "Synthetic supplied mode".into();
    contract.scope.selectors =
        std::collections::BTreeMap::from([("mode".into(), json!("positive"))]);
    contract.scope.identity_dimensions = vec!["mode".into()];
    contract.acceptable_methods = vec!["synthetic_flag/1".into()];
    contract.quantities =
        serde_json::from_value(json!([{"name":"present","unit":"boolean"}])).unwrap();
    contract.time.require_original_observation_time = false;
    let input = json!({"type":"object","additionalProperties":false,"properties":{"mode":{"type":"string","enum":["positive","negative","error"]}},"required":["mode"]});
    let output = json!({"type":"object","additionalProperties":false,"properties":{"present":{"type":"boolean"},"mode":{"type":"string","enum":["positive","negative","error"]}},"required":["present","mode"]});
    elaboration.acquisitions[0].contract = contract.clone();
    elaboration.acquisitions[0].arguments = json!({"mode":"positive"});
    elaboration.acquisitions[0].operation_spec=Some(serde_json::from_value(json!({
        "schema":engarg::support_requirements::OPERATION_SPECIFICATION_SCHEMA,"definition":"Return present=true exactly when caller mode is positive; return false for negative and fail for error.",
        "input_conditions":["mode is positive, negative or error"],"output_guarantees":["Retain the synthetic instrument mode in the measured result. Do not infer or certify any real deployment state."],
        "input_schema":input,"output_schema":output
    })).unwrap());
    for requirement in &mut elaboration.requirements {
        if requirement.id.starts_with("shared_requirement_") {
            requirement.contract = Some(contract.clone());
            requirement.acceptance =
                serde_json::from_value(json!({"kind":"equals","expected":true})).unwrap();
        }
    }
    for evaluation in &mut elaboration.evaluations {
        if evaluation.requirement_id.starts_with("shared_requirement_") {
            evaluation.inputs[0].pointer = "/present".into();
            evaluation.criterion =
                serde_json::from_value(json!({"kind":"equals","expected":true})).unwrap();
        }
    }
    assert!(
        validate_elaboration(&case, &elaboration).is_empty(),
        "{:?}",
        validate_elaboration(&case, &elaboration)
    );
    let connections = root.join("connections.json");
    fs::write(&connections,json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":[],"local_catalogues":[],"approved_catalogues":[]}).to_string()).unwrap();
    let config:PlanningConfig=serde_json::from_value(json!({"schema":"engarg.capability-planning-config/1","collector":collector,"connections":connections,"store":root.join("observations"),"environment":[],"ranking":{"dimensions":["execution_requests"]},"scheduling":SchedulingPolicy::default(),"bindings":[]})).unwrap();
    let planning = root.join("planning.json");
    fs::write(&planning, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let routes = route_planning::compile(
        &case,
        &elaboration,
        &CapabilityCatalogue {
            schema: CAPABILITY_CATALOGUE_SCHEMA.into(),
            capabilities: vec![],
        },
        &config,
        &DiscoverySnapshot::default(),
    )
    .unwrap();
    let implementation = routes
        .gaps
        .iter()
        .find_map(|gap| gap.implementation.as_ref())
        .expect("explicit operation must be a genuine implementation gap");
    let mut draft = draft_response();
    draft["purpose"] = json!(implementation.operation_purpose);
    draft["input_schema_json"] = json!(implementation.input_schema.to_string());
    draft["result_schema_json"] = json!(implementation.output_schema.to_string());
    draft["arguments"] = json!([]);
    draft["unresolved_arguments"] = json!(["mode"]);
    (
        json!({"request":request,"argument":case,"evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":routes}}),
        config,
        draft,
    )
}

fn synthetic_qualification() -> Value {
    json!({"qualification_reference":"synthetic-operator-review/1","support_types":["observation"],"subject_kinds":["synthetic_flag"],"methods":["synthetic_flag/1"],"identity_dimensions":["mode"],"scope_constraints":{},"scope_bindings":{"mode":"/mode"},"quantities":[{"name":"present","unit":"boolean","output_pointer":"/present"}],"coverage":[{"strength":"existential_witness","population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},"provenance":{"authorities":["fixture"],"transformation_history":true},"limitations":["Synthetic operator fixture only, no private claim truth established."]})
}

#[test]
#[ignore = "requires matching EAL_COLLECTOR; runs local synthetic evidence without Docker"]
fn generated_interface_native_preflight_requires_original_time_then_executes_and_reuses_one_observation()
 {
    use eal_collector_contracts::capability::schema_identity;
    let collector =
        PathBuf::from(std::env::var_os("EAL_COLLECTOR").expect("EAL_COLLECTOR is required"))
            .canonicalize()
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source_program = compile_host_rust_collector(root);
    let (report, config, _) = generated_operation_fixture(root, &collector);
    let request = &report["evidence_planning"]["routes"]["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|gap| !gap["implementation"].is_null())
        .unwrap()["implementation"];
    let mut descriptor = serde_json::to_value(support_fixture::descriptor()).unwrap();
    descriptor["id"] = json!("synthetic_generated");
    descriptor["provider"]["id"] = json!("synthetic_generated");
    descriptor["input_schema"] = request["input_schema"].clone();
    descriptor["input_schema_sha256"] =
        json!(schema_identity(&descriptor["input_schema"]).unwrap());
    descriptor["output_schema"] = request["output_schema"].clone();
    descriptor["output_schema_sha256"] =
        json!(schema_identity(&descriptor["output_schema"]).unwrap());
    descriptor["semantics"] = synthetic_qualification();
    let supervisor = collector.with_file_name("eal-process-supervisor");
    let input = root.join("native.json");
    let original = root.join("analysis.json");
    fs::write(&original, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let mut native = json!({"capability":descriptor,"transport":{"kind":"command","program":source_program,"supervisor":supervisor,"arguments":[],"cwd":root,"inherit_env":[],"protocol":{"kind":"eal_acquisition","environment":"synthetic","tool":"synthetic_generated","tool_version":"1","context":{}}},"mapping":{"tool":"synthetic_generated","raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/value","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"mode":"/mode"}},"pinned_files":[],"credential_env":[]});
    // Preparation/discovery/replanning make no source calls. The ready route is
    // subsequently acquired through the same helper used by the Docker package.
    for (name, original_time) in [("collection_time", false), ("original_time", true)] {
        native["capability"]["semantics"]["time"]["original_observation_time"] =
            json!(original_time);
        native["mapping"]["observation_time"] = if original_time {
            json!({"kind":"pointer","pointer":"/observed_at"})
        } else {
            json!({"kind":"collection_time"})
        };
        fs::write(&input, serde_json::to_vec_pretty(&native).unwrap()).unwrap();
        let prepared = Command::new(&collector)
            .args(["prepare-native-provider", "--input"])
            .arg(&input)
            .output()
            .unwrap();
        successful(&prepared);
        let prepared: Value = serde_json::from_slice(&prepared.stdout).unwrap();
        let connections = root.join(format!("{name}-connections.json"));
        fs::write(&connections,json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":[prepared],"local_catalogues":[],"approved_catalogues":[]}).to_string()).unwrap();
        let mut planning = config.clone();
        planning.connections = connections;
        let planning_path = root.join(format!("{name}-planning.json"));
        fs::write(
            &planning_path,
            serde_json::to_vec_pretty(&planning).unwrap(),
        )
        .unwrap();
        let output = root.join(format!("{name}-replanned.json"));
        successful(
            &Command::new(env!("CARGO_BIN_EXE_engarg"))
                .args(["eal", "replan", "--input"])
                .arg(&original)
                .arg("--evidence-config")
                .arg(&planning_path)
                .arg("--output")
                .arg(&output)
                .output()
                .unwrap(),
        );
        let replanned: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        let routes = &replanned["evidence_planning"]["routes"];
        let route = &routes["routes"][0];
        if original_time {
            assert_eq!(route["selected"], "synthetic_generated", "{routes}");
            assert!(
                route["configuration_gaps"].as_array().unwrap().is_empty(),
                "{routes}"
            );
        } else {
            assert!(route["selected"].is_null(), "{routes}");
            assert!(
                serde_json::to_string(&route["candidates"])
                    .unwrap()
                    .contains("original_observation_time_unavailable"),
                "{routes}"
            );
        }
        assert_eq!(routes["discovery"]["metrics"]["requests"], 0, "{routes}");
        assert_eq!(replanned["replanning"]["model_calls"], 0);
        if original_time {
            assert_native_acquisition_and_reuse(&collector, root, &replanned, &planning_path);
            // Exercise the bounded diagnostic transport too, using the actual
            // retained physical source ID and the same admitted environment.
            let plan: eal_collector_contracts::execution::EvidenceExecutionPlan =
                serde_json::from_value(routes["execution"].clone()).unwrap();
            let diagnostic = native_failure_diagnostic(
                &serde_json::to_value(&planning).unwrap(),
                &plan,
                &root.join("acquired-observations"),
            );
            assert_eq!(diagnostic["outcome"]["returncode"], 0, "{diagnostic}");
            assert!(eal_collector_contracts::execution::valid_digest(
                diagnostic["source_id"].as_str().unwrap()
            ));
            let envelope: Value =
                serde_json::from_str(diagnostic["outcome"]["stdout"].as_str().unwrap()).unwrap();
            assert_eq!(envelope["request"]["input"], diagnostic["request"]["input"]);
            assert_eq!(
                envelope["request"]["context"],
                diagnostic["request"]["context"]
            );
        }
    }
}

fn compile_host_rust_collector(root: &Path) -> PathBuf {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = repository.join("target/native-fixture");
    // Cargo selects a coherent dependency graph even when the main target cache
    // contains several incompatible serde/serde_json builds. This isolated target
    // also avoids taking the outer cargo test invocation's build lock.
    successful(
        &Command::new(env!("CARGO"))
            .args(["build", "--locked", "--offline", "--manifest-path"])
            .arg(repository.join("tests/fixtures/native-collector/Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "line-tables-only")
            .output()
            .unwrap(),
    );
    let name = format!(
        "engarg-native-collector-fixture{}",
        std::env::consts::EXE_SUFFIX
    );
    let output = root.join(format!(
        "host-rust-collector{}",
        std::env::consts::EXE_SUFFIX
    ));
    fs::copy(target.join("debug").join(name), &output).unwrap();
    output
}

#[test]
fn native_fixture_lock_is_a_registry_subset_of_the_main_locked_build() {
    let packages = |source: &str| {
        let lock: toml::Value = toml::from_str(source).unwrap();
        lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|package| {
                package
                    .get("source")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|source| source.starts_with("registry+"))
            })
            .map(|package| {
                (
                    package["name"].as_str().unwrap().to_owned(),
                    package["version"].as_str().unwrap().to_owned(),
                    package["source"].as_str().unwrap().to_owned(),
                    package["checksum"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let root = packages(include_str!("../Cargo.lock"));
    let fixture = packages(include_str!("fixtures/native-collector/Cargo.lock"));
    assert!(
        fixture.is_subset(&root),
        "offline fixture dependencies absent from the main locked build: {:?}",
        fixture.difference(&root).collect::<Vec<_>>()
    );
}

#[test]
fn generated_operation_fixture_keeps_an_explicit_parameterized_algorithm_and_units() {
    let directory = tempfile::tempdir().unwrap();
    let (report, _, draft) =
        generated_operation_fixture(directory.path(), Path::new("/operator/eal-collector"));
    let output: Value =
        serde_json::from_str(draft["result_schema_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        output["properties"]["present"]["x-engarg-quantity"]["unit"],
        "boolean"
    );
    assert!(
        draft["purpose"]
            .as_str()
            .unwrap()
            .contains("Return present=true exactly")
    );
    assert!(
        report["evidence_planning"]["routes"]["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["kind"] == "operation_implementation")
    );
}

#[test]
fn rust_fixture_original_observation_clock_matches_utc_calendar_boundaries() {
    for seconds in [
        0,
        951_782_399,
        951_782_400,
        1_704_067_200,
        1_740_787_199,
        1_740_787_200,
        1_792_886_400,
    ] {
        let clock = std::time::UNIX_EPOCH + Duration::from_secs(seconds);
        assert_eq!(
            rust_collector_fixture::observation_time(clock).unwrap(),
            chrono::DateTime::from_timestamp(seconds as i64, 0)
                .unwrap()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string()
        );
    }
}

fn assert_native_acquisition_and_reuse(
    collector: &Path,
    root: &Path,
    selected: &Value,
    registered: &Path,
) {
    // Exercise the registered command through the ordinary EAL support graph
    // path. Replanning itself above performed no evidence calls.
    let execution_path = root.join("execution.json");
    let execution: eal_collector_contracts::execution::EvidenceExecutionPlan =
        serde_json::from_value(selected["evidence_planning"]["routes"]["execution"].clone())
            .unwrap();
    fs::write(
        &execution_path,
        serde_json::to_vec_pretty(&execution).unwrap(),
    )
    .unwrap();
    let planning: Value = serde_json::from_slice(&fs::read(registered).unwrap()).unwrap();
    let evidence_store = root.join("acquired-observations");
    let case_digest =
        eal_collector_contracts::execution::canonical_digest(&selected["argument"]).unwrap();
    let plan_digest = eal_collector_contracts::execution::canonical_digest(&execution).unwrap();
    let context = json!({"engarg_export_digest":case_digest,"engarg_execution_plan_sha256":plan_digest,"opaque":engarg::tooling::verification::protocol_precision_context().unwrap()});
    let acquisition = json!({"evidence_id":"synthetic_requirement","environment":"synthetic","tool":"support_graph","tool_version":"1","input":{"schema":"engarg.support-acquisition/1","case_digest":case_digest,"execution_plan_sha256":plan_digest,"requirement_id":"shared_requirement_0"},"context":context});
    let acquire = || {
        let mut child = Command::new(collector)
            .args(["support-graph", "--plan"])
            .arg(&execution_path)
            .arg("--config")
            .arg(planning["connections"].as_str().unwrap())
            .arg("--store")
            .arg(&evidence_store)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&acquisition).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        if !output.status.success() {
            let diagnostic = native_failure_diagnostic(&planning, &execution, &evidence_store);
            fs::write(
                root.join("native-acquisition.failure.json"),
                serde_json::to_vec_pretty(&diagnostic).unwrap(),
            )
            .unwrap();
            panic!(
                "support-graph failed: {}\nNative command diagnostic (environment values omitted): {}",
                String::from_utf8_lossy(&output.stderr),
                serde_json::to_string_pretty(&diagnostic).unwrap()
            );
        }
        successful(&output);
        eal_collector_contracts::strict_json_exact(&output.stdout, Default::default()).unwrap()
    };
    let observed = acquire();
    assert_eq!(observed["value"]["criterion_satisfied"], true, "{observed}");
    assert_eq!(
        observed["details"]["metrics"]["source_requests"], 1,
        "{observed}"
    );
    assert_eq!(observed["context"], context);
    let original_time =
        chrono::DateTime::parse_from_rfc3339(observed["observed_at"].as_str().unwrap()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let age = now - original_time.timestamp();
    assert!((0..300).contains(&age), "{observed}");
    let snapshot = evidence_store.join(
        observed["details"]["observations"][0]["snapshot"]
            .as_str()
            .unwrap(),
    );
    let retained = fs::read(&snapshot).unwrap();
    let reused_observation = acquire();
    assert_eq!(
        reused_observation["value"]["criterion_satisfied"], true,
        "{reused_observation}"
    );
    assert_eq!(
        reused_observation["details"]["metrics"]["source_requests"], 0,
        "{reused_observation}"
    );
    assert_eq!(
        reused_observation["details"]["metrics"]["reused_sources"], 1,
        "{reused_observation}"
    );
    assert_eq!(reused_observation["observed_at"], observed["observed_at"]);
    assert_eq!(
        reused_observation["value"]["observation_ids"],
        observed["value"]["observation_ids"]
    );
    assert_eq!(fs::read(snapshot).unwrap(), retained);
    assert_export_collection_and_assessment(collector, root, selected, &execution);
}

/// Carry the selected provider through the public export, collection and
/// assessment commands. The synthetic source settles its three predicates;
/// unrelated review and counter-evidence obligations must remain unresolved.
fn assert_export_collection_and_assessment(
    collector: &Path,
    root: &Path,
    selected: &Value,
    execution: &eal_collector_contracts::execution::EvidenceExecutionPlan,
) {
    let analysis = root.join("selected-for-export.json");
    fs::write(&analysis, serde_json::to_vec_pretty(selected).unwrap()).unwrap();
    let source = root.join("selected.eal");
    let manifest = root.join("selected.bundle.json");
    let registry = root.join("selected.tools.toml");
    successful(
        &Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "export", "--input"])
            .arg(&analysis)
            .arg("--output")
            .arg(&source)
            .arg("--manifest")
            .arg(&manifest)
            .arg("--tools-output")
            .arg(&registry)
            .output()
            .unwrap(),
    );
    let bundle: engarg::support_export::SupportExport =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&bundle.support_elaboration).unwrap(),
        selected["evidence_planning"]["elaboration"]
    );
    assert_eq!(
        bundle.execution_plan.acquisitions[0].source,
        execution.acquisitions[0].source
    );
    assert!(!bundle.support_elaboration.alternative_support.is_empty());
    let host = collector.with_file_name(format!("eal-host{}", std::env::consts::EXE_SUFFIX));
    let database = root.join("selected.sqlite");
    let run = |operation: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .args(["eal", operation, "--input"])
            .arg(&manifest)
            .arg("--eal-host")
            .arg(&host)
            .arg("--workspace")
            .arg(root)
            .arg("--database")
            .arg(&database)
            .args(["--timeout-secs", "120"])
            .env_remove("OPENAI_API_KEY")
            .env_remove("ENGARG_API_KEY");
        command
    };
    let collected = run("collect")
        .arg("--tools")
        .arg(&registry)
        .output()
        .unwrap();
    successful(&collected);
    let collected: Value = serde_json::from_slice(&collected.stdout).unwrap();
    assert_eq!(collected["is_error"], false, "{collected}");
    let collection = &collected["result"];
    let shared = bundle
        .requirements
        .iter()
        .filter(|requirement| {
            requirement
                .requirement_id
                .starts_with("shared_requirement_")
        })
        .collect::<Vec<_>>();
    assert_eq!(shared.len(), 3);
    let mut source_ids = None;
    for requirement in &shared {
        let record = &collection["records"][&requirement.eal_evidence_name];
        assert_eq!(record["status"], "ok", "{record}");
        assert_eq!(record["value"]["criterion_satisfied"], true, "{record}");
        assert_eq!(record["value"]["source_ids"].as_array().unwrap().len(), 1);
        let current = record["value"]["source_ids"].clone();
        if let Some(expected) = &source_ids {
            assert_eq!(
                &current, expected,
                "all consumers must retain the same source identity"
            );
        }
        source_ids = Some(current);
    }
    let now = chrono::DateTime::from(std::time::SystemTime::now())
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let assessed = run("assess")
        .arg("--tools")
        .arg(&registry)
        .arg("--now")
        .arg(now)
        .arg("--collection-id")
        .arg(collection["collection_id"].as_str().unwrap())
        .output()
        .unwrap();
    successful(&assessed);
    let assessed: Value = serde_json::from_slice(&assessed.stdout).unwrap();
    assert_eq!(assessed["is_error"], false, "{assessed}");
    let assessment = &assessed["result"];
    assert_eq!(assessment["valid"], true, "{assessment}");
    assert_eq!(assessment["collection_id"], collection["collection_id"]);
    for requirement in shared {
        assert_eq!(
            assessment["evidence"][&requirement.eal_evidence_name]["status"], "available",
            "{}",
            assessment["evidence"][&requirement.eal_evidence_name]
        );
    }
    assert_eq!(
        assessment["claims"][&bundle.claim_names["current_claim"]]["status"], "unsupported",
        "unavailable independent obligations must survive successful acquisition: {assessment}"
    );
}

/// Synthetic test diagnostics only. The failed-graph caller always fails after
/// this bounded direct command probe; its output never replaces the failed result.
fn native_failure_diagnostic(
    planning: &Value,
    execution: &eal_collector_contracts::execution::EvidenceExecutionPlan,
    store: &Path,
) -> Value {
    let config: Value =
        serde_json::from_slice(&fs::read(planning["connections"].as_str().unwrap()).unwrap())
            .unwrap();
    let step = &execution.acquisitions[0];
    let provider = config["native_providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["capability"]["id"] == step.source.capability_id)
        .unwrap();
    let transport = &provider["transport"];
    let protocol = &transport["protocol"];
    let files=fs::read_dir(store).into_iter().flatten().filter_map(Result::ok).filter(|entry|entry.path().extension().is_some_and(|extension|extension=="json"||extension=="lock"||extension=="pending"))
        .map(|entry|json!({"name":entry.file_name().to_string_lossy(),"bytes":entry.metadata().map(|metadata|metadata.len()).ok()})).collect::<Vec<_>>();
    let source_id = files
        .iter()
        .filter_map(|file| file["name"].as_str())
        .filter_map(|name| {
            name.strip_suffix(".lock")
                .or_else(|| name.strip_suffix(".pending"))
        })
        .find(|id| eal_collector_contracts::execution::valid_digest(id))
        .map(str::to_owned)
        .unwrap_or_else(|| "0".repeat(64));
    let request = json!({"evidence_id":source_id,"environment":protocol["environment"],"tool":protocol["tool"],"tool_version":protocol["tool_version"],"input":step.arguments,"context":protocol["context"]});
    let names = transport["inherit_env"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let environment = names
        .iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.clone(), value)))
        .collect();
    let mut argv = vec![transport["program"].as_str().unwrap().to_owned()];
    argv.extend(
        transport["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap().to_owned()),
    );
    let outcome =
        eal_process::Supervisor::new(PathBuf::from(transport["supervisor"].as_str().unwrap()))
            .run_with_stream_limits_and_cancellation(
                eal_process::CommandRequest {
                    argv: argv.clone(),
                    environment,
                    cwd: PathBuf::from(transport["cwd"].as_str().unwrap()),
                    input: serde_json::to_vec(&request).unwrap(),
                    timeout: Duration::from_secs(
                        provider["capability"]["limits"]["timeout_seconds"]
                            .as_u64()
                            .unwrap()
                            .min(120),
                    ),
                    max_output_bytes: 131072,
                },
                None,
                &eal_process::CancellationToken::default(),
                eal_process::StreamLimits {
                    max_stdout_bytes: 65536,
                    max_stderr_bytes: 65536,
                },
            );
    let outcome = match outcome {
        Ok(outcome) => {
            json!({"returncode":outcome.returncode,"execution_error":format!("{:?}",outcome.execution_error),"stdout":String::from_utf8_lossy(&outcome.stdout),"stderr":String::from_utf8_lossy(&outcome.stderr)})
        }
        Err(error) => json!({"supervisor_error":format!("{error:?}")}),
    };
    json!({"source_id":source_id,"request":request,"argv":argv,"inherited_environment_names":names,"store_entries":files,"outcome":outcome,"provider":provider,"execution":execution})
}

/// This exercises normal commands and the authoritative native config loader;
/// neither hand-edited package receipts nor a test-only registration path is used.
#[test]
#[ignore = "requires preloaded Rust SDK Docker image and installed EAL_COLLECTOR"]
fn generated_package_can_be_qualified_registered_and_replanned_without_reanalysis() {
    let collector = PathBuf::from(
        std::env::var_os("EAL_COLLECTOR")
            .expect("EAL_COLLECTOR must select the installed matching runtime"),
    )
    .canonicalize()
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (report, config, draft) = generated_operation_fixture(root, &collector);
    let server = MockServer::new(vec![
        draft,
        json!({"language":"rust","files":[{"path":"main.rs","content":RUST_COLLECTOR}]}),
    ]);
    let analysis = root.join("analysis.json");
    fs::write(&analysis, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let toolchain = root.join("toolchain.json");
    profile(&toolchain, "rust", "engarg-rust-tools:1.99.1", "main.rs");
    let store = root.join("tools");
    let attempt = root.join("attempt");
    successful(&implement(
        &analysis,
        &toolchain,
        &store,
        &attempt,
        &server.endpoint,
        "rust",
    ));
    let with_tools = attempt.join("analysis.with-tools.json");
    let generated: Value = serde_json::from_slice(&fs::read(&with_tools).unwrap()).unwrap();
    assert_eq!(generated["tooling"]["model_calls"], 2);
    let pending_path = attempt.join("generated-native-providers.json");
    let mut pending: Value = serde_json::from_slice(&fs::read(&pending_path).unwrap()).unwrap();
    assert!(pending["native_providers"][0]["capability"]["semantics"].is_null());
    let id = pending["native_providers"][0]["capability"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut pending_config = config.clone();
    pending_config.connections = pending_path;
    let pending_planning = root.join("pending-planning.json");
    fs::write(
        &pending_planning,
        serde_json::to_vec_pretty(&pending_config).unwrap(),
    )
    .unwrap();
    let pending_report = root.join("pending-report.json");
    successful(
        &Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "replan", "--input"])
            .arg(&with_tools)
            .arg("--evidence-config")
            .arg(&pending_planning)
            .arg("--output")
            .arg(&pending_report)
            .output()
            .unwrap(),
    );
    let unresolved: Value = serde_json::from_slice(&fs::read(&pending_report).unwrap()).unwrap();
    assert!(
        unresolved["evidence_planning"]["routes"]["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["kind"] == "qualification")
    );
    assert!(unresolved.get("tooling").is_none());
    assert_eq!(unresolved["replanning"]["model_calls"], 0);
    // This is an explicit independent synthetic operator qualification, not a
    // scientific conclusion inferred from contract tests.
    let provider = &mut pending["native_providers"][0];
    provider["capability"]["semantics"] = synthetic_qualification();
    provider["mapping"]["scope_pointers"] = json!({"mode":"/mode"});
    let qualified = root.join("qualified.json");
    fs::write(&qualified, serde_json::to_vec_pretty(&pending).unwrap()).unwrap();
    let registered = root.join("registered-planning.json");
    successful(
        &Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "register-tools", "--input"])
            .arg(&pending_report)
            .arg("--qualifications")
            .arg(&qualified)
            .arg("--evidence-config")
            .arg(&pending_planning)
            .arg("--output")
            .arg(&registered)
            .output()
            .unwrap(),
    );
    let final_report = root.join("replanned.json");
    successful(
        &Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "replan", "--input"])
            .arg(&pending_report)
            .arg("--evidence-config")
            .arg(&registered)
            .arg("--output")
            .arg(&final_report)
            .output()
            .unwrap(),
    );
    let selected: Value = serde_json::from_slice(&fs::read(final_report).unwrap()).unwrap();
    let route = selected["evidence_planning"]["routes"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|route| route["acquisition_id"] == "shared_source")
        .unwrap();
    assert_eq!(
        route["selected"],
        id,
        "{}",
        serde_json::to_string_pretty(&selected["evidence_planning"]["routes"]).unwrap()
    );
    assert!(
        route["configuration_gaps"].as_array().unwrap().is_empty(),
        "{route}"
    );
    assert_eq!(
        selected["evidence_planning"]["elaboration"],
        generated["evidence_planning"]["elaboration"]
    );
    assert_eq!(selected["replanning"]["model_calls"], 0);
    assert_native_acquisition_and_reuse(&collector, root, &selected, &registered);
    let second = root.join("second-attempt");
    successful(&implement(
        &analysis,
        &toolchain,
        &store,
        &second,
        &server.endpoint,
        "rust",
    ));
    let reused: Value =
        serde_json::from_slice(&fs::read(second.join("analysis.with-tools.json")).unwrap())
            .unwrap();
    assert_eq!(reused["tooling"]["model_calls"], 0);
    assert_eq!(server.received.lock().unwrap().len(), 2);
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
