//! Real isolated-runner coverage. CI preloads the official toolchain images.
use serde_json::{Value, json};
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

const GO_COLLECTOR: &str = include_str!("fixtures/tooling_collector.go.txt");
const TYPESCRIPT_COLLECTOR: &str = include_str!("fixtures/tooling_collector.ts.txt");

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

#[path = "common/executable_case.rs"]
mod executable_case;
#[path = "common/live_output.rs"]
mod live_output;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn direct_case() -> (
    engarg::domain::Request,
    engarg::domain::ArgumentCase,
    engarg::support_requirements::SupportElaboration,
) {
    let (request, case, mut support) = executable_case::fixture();
    let mut case = serde_json::to_value(case).unwrap();
    case["warrants"][0]["reasoning_method"]["conclusion"]["valid_from"] =
        json!("2020-01-01T00:00:00Z");
    case["warrants"][0]["reasoning_method"]["conclusion"]["valid_until"] =
        json!("2100-01-01T00:00:00Z");
    // This case concerns the supplied synthetic mode. Its method target and all
    // source scopes must identify that same mode, not the inventory fixture from
    // which the general executable-case template was derived.
    case["warrants"][0]["reasoning_method"]["conclusion"]["subject"] =
        json!("Synthetic supplied mode");
    case["warrants"][0]["reasoning_method"]["conclusion"]["scope"] = json!({
        "kind":"synthetic_flag", "selectors":[{"name":"mode","value":"positive"}], "identity_dimensions":["mode"]
    });
    let case = serde_json::from_value(case).unwrap();
    let operation = support
        .acquisitions
        .iter_mut()
        .find(|operation| operation.id == "premise_source")
        .unwrap();
    operation.arguments = json!({"mode":"positive"});
    operation.contract.scope.kind = "synthetic_flag".into();
    operation.contract.scope.subject = "Synthetic supplied mode".into();
    operation.contract.scope.selectors =
        std::collections::BTreeMap::from([("mode".into(), json!("positive"))]);
    operation.contract.scope.identity_dimensions = vec!["mode".into()];
    operation.contract.acceptable_methods = vec!["synthetic_flag/1".into()];
    operation.contract.quantities =
        serde_json::from_value(json!([{"name":"present","unit":"boolean"}])).unwrap();
    operation.contract.time.require_original_observation_time = false;
    operation.operation_spec = Some(serde_json::from_value(json!({
        "schema":engarg::support_requirements::OPERATION_SPECIFICATION_SCHEMA,
        "definition":"Return present=true exactly when caller mode is positive; return false for negative and fail for error.",
        "input_conditions":["mode is positive, negative or error"],
        "output_guarantees":["Retain the synthetic instrument mode in the measured result. Do not infer or certify any real deployment state."],
        "input_schema":{"type":"object","properties":{"mode":{"type":"string","enum":["positive","negative","error"]}},"required":["mode"],"additionalProperties":false},
        "output_schema":{"type":"object","properties":{"present":{"type":"boolean"},"mode":{"type":"string","enum":["positive","negative","error"]}},"required":["present","mode"],"additionalProperties":false}
    })).unwrap());
    let contract = operation.contract.clone();
    for source in &mut support.acquisitions {
        source.contract.scope = contract.scope.clone();
        source.arguments = json!({"mode":"positive"});
    }
    for requirement in &mut support.requirements {
        if let Some(source) = &mut requirement.contract {
            source.scope = contract.scope.clone();
        }
    }
    let requirement = support
        .requirements
        .iter_mut()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap();
    requirement.contract = Some(contract);
    requirement.acceptance = eal_collector_contracts::execution::EvaluationCriterion::Equals {
        expected: json!(true),
    };
    let evaluation = support
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .unwrap();
    evaluation.criterion = requirement.acceptance.clone();
    evaluation.inputs[0].pointer = "/present".into();
    let errors = engarg::support_requirements::validate_elaboration(&case, &support);
    assert!(errors.is_empty(), "{errors:?}");
    (request, case, support)
}

fn direct_draft() -> Value {
    let (_, _, support) = direct_case();
    let request = engarg::tooling::direct::operation_request(
        support
            .acquisitions
            .iter()
            .find(|operation| operation.id == "premise_source")
            .unwrap(),
        &support,
    )
    .unwrap();
    let mut draft = draft_response();
    draft["purpose"] = json!(request.operation_purpose);
    draft["input_schema_json"] = json!(request.input_schema.to_string());
    draft["result_schema_json"] = json!(request.output_schema.to_string());
    draft["arguments"] = json!([]);
    draft["unresolved_arguments"] = json!(["mode"]);
    draft
}

#[test]
fn parameterised_direct_fixture_compiles_without_discovery_or_custom_stubs() {
    let (request, case, support) = direct_case();
    let draft = direct_draft();
    let output_schema: Value =
        serde_json::from_str(draft["result_schema_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        output_schema["properties"]["present"]["x-engarg-quantity"]["unit"],
        "boolean"
    );
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("observation.json"), "{}").unwrap();
    let tools = root.path().join("tools.toml");
    fs::write(&tools,"[tools.premise_source]\nkind='json_file'\nversion='1'\npath='observation.json'\n[tools.applicability_source]\nkind='json_file'\nversion='1'\npath='observation.json'\n").unwrap();
    engarg::package::build(
        &request,
        &case,
        &support,
        &tools,
        &root.path().join("package"),
        &Default::default(),
    )
    .unwrap();
    let source = fs::read_to_string(root.path().join("package/case.eal")).unwrap();
    assert!(source.starts_with("language \"EAL/4\""));
    assert!(!source.contains("engarg_support_graph"));
}

fn host() -> PathBuf {
    PathBuf::from(
        std::env::var_os("ENGARG_EAL_HOST")
            .expect("matching ENGARG_EAL_HOST installation required"),
    )
    .canonicalize()
    .unwrap()
}

fn authoring_fixture(root: &Path) -> Value {
    let (request, case, support) = direct_case();
    let input = root.join("request.json");
    fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
    fs::create_dir(root.join("tools")).unwrap();
    fs::write(root.join("tools/applicability.py"), "import json,sys,datetime
request=json.load(sys.stdin)
print(json.dumps({'value':{'applies':request['input']['mode']=='positive'},'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z')}))
").unwrap();
    let tools = root.join("tools.toml");
    fs::write(&tools, "[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/applicability.py']\ninherit_env=['PATH']\n".replace("\\n", "\n")).unwrap();
    json!({"argument":live_output::generation_value(&case),"support_elaboration_json":serde_json::to_string(&support).unwrap()})
}

fn authoring_command(root: &Path, endpoint: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .args(["analyse", "--input"])
        .arg(root.join("request.json"))
        .arg("--tools")
        .arg(root.join("tools.toml"))
        .arg("--eal-host")
        .arg(host())
        .args([
            "--model",
            "fixture",
            "--endpoint",
            endpoint,
            "--api-key-env",
            "UNSET_TOOL_TEST_KEY",
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
        ])
        .env_remove("UNSET_TOOL_TEST_KEY");
    command
}

fn saved_report(path: &Path) {
    let root = path.parent().unwrap();
    let server = MockServer::new(vec![authoring_fixture(root)]);
    successful(
        &authoring_command(root, &server.endpoint)
            .arg("--output")
            .arg(path)
            .output()
            .unwrap(),
    );
    assert_eq!(server.requests().len(), 1);
    let report: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    engarg::report::read_direct(&report).unwrap();
    assert_eq!(
        report["direct_bindings"]["unbound_operations"],
        json!(["premise_source"])
    );
}

fn profile(path: &Path, language: &str, image: &str, entry: &str) {
    let builtin = match language {
        "rust" => Some(include_str!("../examples/toolchains/rust.json")),
        "go" => Some(include_str!("../examples/toolchains/go.json")),
        "typescript" => Some(include_str!("../examples/toolchains/typescript.json")),
        "javascript" => Some(include_str!("../examples/toolchains/javascript.json")),
        _ => None,
    };
    if let Some(builtin) = builtin {
        let mut value: Value = serde_json::from_str(builtin).unwrap();
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
        "schema": "engarg.toolchain/2",
        "language": language,
        "docker": std::env::var("ENGARG_TOOL_DOCKER").unwrap_or_else(|_| "docker".into()),
        "image": image,
        "build_argv": build,
        "run_argv": run,
        "inherit_env": [],
        "runtime_network": "none",
        "timeout_seconds": 30,
        "memory_mb": 128,
        "output_bytes": 1_048_576,
        "generation":{"libraries":{"use_defaults":false,"fallback":"standard_library","tasks":{}}}
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
        .arg("--tools")
        .arg(input.parent().unwrap().join("tools.toml"))
        .arg("--eal-host")
        .arg(host())
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

fn snapshot_volume_names(mounts: &Value) -> Vec<String> {
    let mut volumes = mounts
        .as_array()
        .unwrap()
        .iter()
        .filter(|mount| matches!(mount["Destination"].as_str(), Some("/src" | "/fixtures")))
        .map(|mount| {
            assert_eq!(mount["Type"], "volume", "{mounts}");
            let name = mount["Name"].as_str().unwrap();
            assert!(!name.is_empty() && name.len() <= 200);
            assert!(
                name.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            );
            name.to_owned()
        })
        .collect::<Vec<_>>();
    volumes.sort();
    volumes.dedup();
    assert_eq!(volumes.len(), 2, "{mounts}");
    volumes
}

fn assert_snapshot_volumes_removed(docker: &str, volumes: &[String]) {
    let mut retained = Vec::new();
    for volume in volumes {
        let inspection = Command::new(docker)
            .args(["volume", "inspect", volume])
            .output()
            .unwrap();
        let absent = !inspection.status.success()
            && String::from_utf8_lossy(&inspection.stderr)
                .to_lowercase()
                .contains("no such volume");
        if !absent {
            // A regression cleans up only the volume IDs captured from its own
            // staging container, without comparing or pruning global Docker state.
            let _ = Command::new(docker).args(["volume", "rm", volume]).output();
            retained.push(format!(
                "{volume}: {}",
                String::from_utf8_lossy(&inspection.stderr)
            ));
        }
    }
    assert!(
        retained.is_empty(),
        "snapshot volumes were not removed: {retained:?}"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires Docker and the preloaded python:3.13-slim image"]
fn model_free_preflight_uploads_real_snapshots_and_removes_its_anonymous_volumes() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let docker = std::env::var("ENGARG_TOOL_DOCKER").unwrap_or_else(|_| "docker".into());
    let log = directory.path().join("snapshot-mounts.jsonl");
    let wrapper = directory.path().join("logged-docker");
    let config = json!({"docker":docker,"log":log});
    let encoded = serde_json::to_string(&serde_json::to_string(&config).unwrap()).unwrap();
    let script = r#"
import subprocess
import sys

args = sys.argv[1:]
capture = args[:2] == ["container", "inspect"] and ".Mounts" in " ".join(args)
stdin = sys.stdin.buffer if args[0] in ["cp", "run"] else subprocess.DEVNULL
if capture:
    child = subprocess.Popen([CONFIG["docker"], *args], stdin=stdin, stdout=subprocess.PIPE)
    output = child.stdout.read(65537)
    if len(output) > 65536:
        child.kill()
        child.wait()
        raise SystemExit("snapshot inspection exceeds its test bound")
    status = child.wait()
    if status == 0:
        mounts = json.loads(output)
        with open(CONFIG["log"], "a", encoding="utf-8") as target:
            target.write(json.dumps(mounts) + "\n")
    sys.stdout.buffer.write(output)
    raise SystemExit(status)
raise SystemExit(subprocess.run([CONFIG["docker"], *args], stdin=stdin).returncode)
"#;
    fs::write(
        &wrapper,
        format!("#!/usr/bin/env python3\nimport json\nCONFIG = json.loads({encoded})\n{script}"),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let toolchain = directory.path().join("toolchain.json");
    profile(&toolchain, "python", "python:3.13-slim", "collector.py");
    let mut settings: Value = serde_json::from_slice(&fs::read(&toolchain).unwrap()).unwrap();
    settings["docker"] = json!(wrapper);
    fs::write(&toolchain, serde_json::to_vec(&settings).unwrap()).unwrap();
    let receipt = directory.path().join("toolchain-check.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "toolchain-check", "--toolchain"])
        .arg(toolchain)
        .arg("--output")
        .arg(&receipt)
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    let captures = fs::read_to_string(&log).unwrap_or_default();
    let volumes = captures
        .lines()
        .flat_map(|line| snapshot_volume_names(&serde_json::from_str::<Value>(line).unwrap()))
        .collect::<Vec<_>>();
    assert_snapshot_volumes_removed(&docker, &volumes);
    successful(&output);
    assert_eq!(volumes.len(), 2, "expected one real snapshot inspection");
    let receipt: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
    assert_eq!(receipt["passed"], true);
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
    let request = json!({
        "evidence_id": contract.need.id,
        "environment": "engarg_tool_fixture",
        "tool": "engarg_generated_fixture",
        "tool_version": "1",
        "input": fixture.input,
        "context": engarg::tooling::verification::protocol_fixture_context(&fixture.name)?,
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

fn export_manifest(report_path: &Path, directory: &Path) -> (String, PathBuf) {
    let report: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    let tools = Path::new(report["tooling"]["tools"].as_str().unwrap());
    let mut argument = report["argument"].clone();
    argument["adjudication"] = json!(format!(
        "{} The explicit revision preserves the synthetic mode scope.",
        argument["adjudication"].as_str().unwrap()
    ));
    let proposed = directory.join("revised-argument.json");
    fs::write(&proposed, serde_json::to_vec(&argument).unwrap()).unwrap();
    let support = directory.join("revised-support.json");
    fs::write(
        &support,
        serde_json::to_vec(&report["support_elaboration"]).unwrap(),
    )
    .unwrap();
    let revised = directory.join("revised.json");
    successful(
        &Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "revise", "--input"])
            .arg(report_path)
            .arg("--argument")
            .arg(proposed)
            .arg("--support-elaboration")
            .arg(support)
            .arg("--tools")
            .arg(tools)
            .arg("--output")
            .arg(&revised)
            .output()
            .unwrap(),
    );
    let package = directory.join("package");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["build", "--input"])
        .arg(&revised)
        .arg("--tools")
        .arg(tools)
        .arg("--output-dir")
        .arg(&package)
        .output()
        .unwrap();
    successful(&output);
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    let claim = receipt["claims"]["current_claim"].as_str().unwrap();
    let selected_host = host();
    let execution_path = std::env::join_paths(
        std::iter::once(selected_host.parent().unwrap().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ),
    )
    .unwrap();
    let output = Command::new(selected_host)
        .env("PATH", execution_path)
        .arg("run")
        .arg(package.join("case.eal"))
        .arg("--tools")
        .arg(package.join("tools.toml"))
        .arg("--workspace")
        .arg(directory.join("run"))
        .arg("--claim")
        .arg(claim)
        .output()
        .unwrap();
    successful(&output);
    let executed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(executed["is_error"], false, "{executed}");
    assert_eq!(
        executed["result"]["assessment"]["claims"][claim]["status"], "supported",
        "{executed}"
    );
    assert!(
        !fs::read_to_string(package.join("case.eal"))
            .unwrap()
            .contains("engarg_support_graph")
    );
    (
        engarg::support_export::operation_tool_name("premise_source"),
        PathBuf::from(
            report["tooling"]["operations"][0]["implementation"]["package_manifest"]
                .as_str()
                .unwrap(),
        ),
    )
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
    profile(&toolchain, language, image, entry);
    let mut responses = vec![
        direct_draft(),
        json!({"language": language, "files": [{"path": entry, "content": code}]}),
    ];
    let combined = language == "python";
    if combined {
        responses.insert(0, authoring_fixture(directory.path()));
    } else {
        saved_report(&input);
    }
    let expected_calls = responses.len();
    let server = MockServer::new(responses);
    let output = if combined {
        authoring_command(directory.path(), &server.endpoint)
            .arg("--implement-tools")
            .args(["--tool-language", language, "--toolchain"])
            .arg(&toolchain)
            .arg("--tool-store")
            .arg(&store)
            .arg("--tool-output-dir")
            .arg(&first)
            .args(["--tool-max-calls", "2", "--tool-timeout-secs", "120"])
            .arg("--output")
            .arg(directory.path().join("combined.json"))
            .output()
            .unwrap()
    } else {
        implement(
            &input,
            &toolchain,
            &store,
            &first,
            &server.endpoint,
            language,
        )
    };
    successful_implementation(&output, &store);
    assert_eq!(server.requests().len(), expected_calls);
    let report_path = if combined {
        directory.path().join("combined.json")
    } else {
        first.join("analysis.with-tools.json")
    };
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    if combined {
        let implementation: Value =
            serde_json::from_slice(&fs::read(first.join("implementation.json")).unwrap()).unwrap();
        assert_eq!(implementation, report["tooling"]);
        engarg::report::read_direct(&report).unwrap();
        // A later independent implementation consumes the exact pre-tool report
        // retained by combined authoring, without reconstructing an envelope.
        fs::copy(
            report["tooling"]["retained_input"].as_str().unwrap(),
            &input,
        )
        .unwrap();
    }
    assert_eq!(report["tooling"]["operations"].as_array().unwrap().len(), 1);
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
    let retained_contract: Value = serde_json::from_slice(
        &fs::read(manifest.parent().unwrap().join("contract.json")).unwrap(),
    )
    .unwrap();
    let frozen_context: Value =
        serde_json::from_str(retained_contract["request_context"].as_str().unwrap()).unwrap();
    assert_eq!(frozen_context["library_policy"]["language"], language);
    if combined {
        assert_eq!(
            frozen_context["library_policy"]["fallback"],
            "standard_library"
        );
        assert_eq!(frozen_context["library_policy"]["tasks"], json!({}));
    }
    let runtime_profile: Value =
        serde_json::from_slice(&fs::read(manifest.parent().unwrap().join("profile.json")).unwrap())
            .unwrap();
    assert_eq!(runtime_profile["schema"], "engarg.toolchain/1");
    assert!(runtime_profile.get("generation").is_none());
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
        repeated["tooling"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operation| operation["implementation"]["reused"] == true)
    );
    let (reused_tool, reused_manifest) =
        export_manifest(&second.join("analysis.with-tools.json"), &second);
    assert_eq!(reused_tool, tool_name);
    assert_eq!(reused_manifest, manifest);
    assert_eq!(server.requests().len(), expected_calls);

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
        unkeyed["tooling"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operation| operation["implementation"]["reused"] == true)
    );
    let (unkeyed_tool, unkeyed_manifest) =
        export_manifest(&third.join("analysis.with-tools.json"), &third);
    assert_eq!(unkeyed_tool, tool_name);
    assert_eq!(unkeyed_manifest, manifest);
    assert_eq!(server.requests().len(), expected_calls);

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
fn python_combined_authoring_implements_revises_builds_executes_and_reuses() {
    exercise(
        "python",
        "python:3.13-slim",
        "collector.py",
        PYTHON_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the prepared engarg-javascript-tools:24 image"]
fn javascript_collector_preserves_exact_protocol_identity_and_reuses_verified_code() {
    exercise(
        "javascript",
        "engarg-javascript-tools:24",
        "collector.mjs",
        JAVASCRIPT_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the preloaded engarg-rust-tools:1.99.1 image"]
fn rust_direct_operation_is_compiled_executed_and_reused_with_exact_protocol_identity() {
    exercise(
        "rust",
        "engarg-rust-tools:1.99.1",
        "main.rs",
        RUST_COLLECTOR,
    );
}

#[test]
#[ignore = "requires Docker and the preloaded engarg-go-tools:1.25.14 image"]
fn go_direct_operation_is_compiled_executed_and_reused_with_exact_protocol_identity() {
    exercise("go", "engarg-go-tools:1.25.14", "main.go", GO_COLLECTOR);
}

#[test]
#[ignore = "requires Docker and the preloaded engarg-typescript-tools:5.9.3 image"]
fn typescript_direct_operation_is_compiled_executed_and_reused_with_exact_protocol_identity() {
    exercise(
        "typescript",
        "engarg-typescript-tools:5.9.3",
        "collector.ts",
        TYPESCRIPT_COLLECTOR,
    );
}

#[tokio::test]
#[ignore = "requires Docker, preloaded Rust SDK image and installed ENGARG_EAL_HOST with eal-tool-run"]
async fn capability_contract_generates_rust_and_reuses_it_for_different_claims() {
    use eal_collector_contracts::capability::{
        LogicalStrength, ProvenanceRequirement, SUPPORT_CONTRACT_SCHEMA, SubjectScope,
        SupportContract, SupportType, TimeRequirement,
    };
    use engarg::{
        provider::{Completion, CompletionRequest, LanguageModel},
        tooling::{
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
                require_original_observation_time: true,
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
        library_policy: None,
    };
    let configuration = engarg::tooling::profile::ToolchainConfiguration::load(&toolchain).unwrap();
    let mut selected = CapabilityImplementationConfig {
        language: "rust".into(),
        libraries: configuration.libraries,
        profile: configuration.profile,
        store: directory.path().join("store"),
        max_calls: 2,
        max_input_bytes: 1_048_576,
        max_response_bytes: 1_048_576,
        supervisor: PathBuf::from(env!("CARGO_BIN_EXE_engarg-process-supervisor")),
        engarg_executable: std::env::current_exe().unwrap(),
    };
    // An old, fixed source clock exposes accidental refresh during export or import.
    const ORIGINAL_TIME: &str = "2026-01-01T00:00:00Z";
    let portable_code = RUST_COLLECTOR.replace(
        "observation_time(std::time::SystemTime::now())?",
        &format!("{ORIGINAL_TIME:?}.to_owned()"),
    );
    assert_ne!(portable_code, RUST_COLLECTOR);
    let model = Model(Mutex::new(std::collections::VecDeque::from([
        draft.to_string(),
        json!({"language":"rust","files":[{"path":"main.rs","content":portable_code}]}).to_string(),
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
        assert_eq!(observation["observed_at"], ORIGINAL_TIME);
    }
    assert!(model.0.lock().unwrap().is_empty());
    execute_moved_tool_export(&second.package_manifest, directory.path(), ORIGINAL_TIME);
}

/// Exercise the installed standalone launcher through native EAL collection.
/// The caller has already paid for source generation and real Docker verification.
fn execute_moved_tool_export(manifest: &Path, root: &Path, original_time: &str) {
    let host = PathBuf::from(
        std::env::var_os("ENGARG_EAL_HOST")
            .or_else(|| std::env::var_os("EAL_HOST"))
            .expect("portable Docker regression requires an installed EAL host"),
    )
    .canonicalize()
    .unwrap();
    let prefix = host.parent().unwrap();
    for name in ["eal-tool-run", "eal-process-supervisor"] {
        assert!(prefix.join(name).is_file(), "missing installed {name}");
    }
    let exported = root.join("portable-export");
    let export = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "tool-export", "--package"])
        .arg(manifest)
        .arg("--output-dir")
        .arg(&exported)
        .args(["--tool-id", "portable_fixture"])
        .output()
        .unwrap();
    successful(&export);
    let exported_report: Value = serde_json::from_slice(&export.stdout).unwrap();
    assert_eq!(exported_report["tool_id"], "portable_fixture");
    let tools = fs::read_to_string(exported.join("tools.toml")).unwrap();
    let binding: toml::Value = toml::from_str(&tools).unwrap();
    assert_eq!(
        binding["tools"]["portable_fixture"]["argv"][0].as_str(),
        Some("eal-tool-run")
    );
    assert!(!tools.contains("engarg"));
    assert!(!tools.contains(manifest.to_str().unwrap()));
    fs::write(
        exported.join("tools.toml"),
        format!("{tools}\n[context]\nsuite = 'portable'\n"),
    )
    .unwrap();
    fs::write(
        exported.join("case.eal"),
        r#"language "EAL/4"
environment fixture {
require suite == "portable"
}
tool portable_fixture {
version "1"
}
evidence sample {
tool portable_fixture
kind observation
environment fixture
max_age 3600
input {"mode":"positive"}
require collected == true
require present == true
}
reasoning observation {
method "structured/1"
rationale "The verified fixture returns its supplied positive mode."
}
claim checked {
statement "The supplied synthetic mode was positive."
environment fixture
}
argument support = [evidence sample] via observation => checked
"#,
    )
    .unwrap();
    let moved = root.join("relocated-portable");
    fs::rename(&exported, &moved).unwrap();
    assert!(!exported.exists());
    // Neither the old package location nor its authoring manifest is available.
    fs::rename(
        manifest.parent().unwrap(),
        root.join("retired-authoring-input"),
    )
    .unwrap();
    assert!(!manifest.exists());
    let path = std::env::join_paths(std::iter::once(prefix.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let run = |workspace: &str| {
        Command::new(&host)
            .current_dir(&moved)
            .env("PATH", &path)
            .env_remove("OPENAI_API_KEY")
            .args([
                "run",
                "case.eal",
                "--tools",
                "tools.toml",
                "--workspace",
                workspace,
                "--claim",
                "checked",
                "--now",
                "2026-01-01T00:00:01Z",
            ])
            .output()
            .unwrap()
    };
    let acquired = run("successful-run");
    successful(&acquired);
    let response: Value = serde_json::from_slice(&acquired.stdout).unwrap();
    assert_eq!(response["is_error"], false, "{response}");
    let result = &response["result"];
    assert_eq!(
        result["assessment"]["claims"]["checked"]["status"], "supported",
        "{response}"
    );
    let record = &result["collection"]["records"]["sample"];
    assert_eq!(record["status"], "ok", "{record}");
    assert_eq!(
        record["value"],
        json!({"collected":true,"present":true,"mode":"positive"})
    );
    assert_eq!(record["collected_at"], original_time);
    assert_ne!(record["ingested_at"], record["collected_at"]);
    assert_eq!(
        record["acquisition_request"],
        json!({"tool":"portable_fixture","tool_version":"1","input":{"mode":"positive"},"context":{"suite":"portable"}})
    );
    assert_eq!(record["context"], json!({"suite":"portable"}));
    assert!(record["stdout_bytes"].as_u64().unwrap() > 0);

    // The moved package must enforce its own content pins without the authoring store.
    fs::write(moved.join("source/main.rs"), "unverified replacement").unwrap();
    let refused = run("refused-run");
    let report: Value = serde_json::from_slice(&refused.stdout).unwrap_or(Value::Null);
    assert!(
        !refused.status.success()
            || report["is_error"] == true
            || report["result"]["collection"]["records"]["sample"]["status"] == "error",
        "tampered source acquired successfully: {report}"
    );
    assert_ne!(
        report["result"]["assessment"]["claims"]["checked"]["status"],
        "supported"
    );
    let diagnostic = format!(
        "{} {}",
        String::from_utf8_lossy(&refused.stderr),
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(
        diagnostic.to_ascii_lowercase().contains("pin") || diagnostic.contains("SHA-256"),
        "{diagnostic}"
    );
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
    let (request, case, mut elaboration, _, _) = support_fixture::fixture();
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
    let report = json!({
        "schema_version":engarg::report::RUN_REPORT_SCHEMA,
        "execution_mode":"offline_synthetic_fixture",
        "input_sha256":engarg::authoring_checkpoint::digest(serde_json::to_vec(&request).unwrap()),
        "mode":"single", "request":request, "argument":case, "transcript":[],
        "limitations":["Synthetic operation discovery and interface qualification fixture."],
        "evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":routes}
    });
    engarg::report::read_direct(&report).unwrap();
    (report, config, draft)
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
    // Require an already-present, non-secret reference that the host does not
    // inherit by default. Native admission checks every declared reference even
    // when the synthetic executable itself does not read it. This reproduces
    // the Docker provider's environment boundary without requiring Docker.
    native["transport"]["inherit_env"] = json!(["EAL_COLLECTOR"]);
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
        // The export helper supplies USER with a deterministic public fixture
        // value; the native provider itself needs only EAL_COLLECTOR.
        planning.environment = vec!["EAL_COLLECTOR".into(), "USER".into()];
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
        }
    }
}

fn assert_native_acquisition_and_reuse(
    collector: &Path,
    root: &Path,
    selected: &Value,
    registered: &Path,
) {
    use engarg::{
        domain::{ArgumentCase, Request},
        export_policy::ExportPolicy,
        support_export,
        support_requirements::SupportElaboration,
    };
    let execution: eal_collector_contracts::execution::EvidenceExecutionPlan =
        serde_json::from_value(selected["evidence_planning"]["routes"]["execution"].clone())
            .unwrap();
    let request: Request = serde_json::from_value(selected["request"].clone()).unwrap();
    let case: ArgumentCase = serde_json::from_value(selected["argument"].clone()).unwrap();
    let elaboration: SupportElaboration =
        serde_json::from_value(selected["evidence_planning"]["elaboration"].clone()).unwrap();
    let compiled = support_export::compile(
        &request,
        &case,
        &elaboration,
        &execution,
        &ExportPolicy::default(),
    )
    .unwrap();
    assert!(!compiled.bundle.source.contains("engarg_support_graph"));
    let planning: Value = serde_json::from_slice(&fs::read(registered).unwrap()).unwrap();
    let configuration: Value =
        serde_json::from_slice(&fs::read(planning["connections"].as_str().unwrap()).unwrap())
            .unwrap();
    let mut manifest = eal_collector_contracts::RegistryDocument::default();
    for step in &execution.acquisitions {
        let provider = configuration["native_providers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|provider| provider["capability"]["id"] == step.source.capability_id)
            .unwrap();
        let transport = &provider["transport"];
        let mut argv = vec![transport["program"].as_str().unwrap().to_owned()];
        argv.extend(
            transport["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap().to_owned()),
        );
        manifest.tools.insert(
            support_export::operation_tool_name(&step.id),
            eal_collector_contracts::RegistryBinding {
                kind: eal_collector_contracts::ToolKind::Command,
                version: step.source.capability_version.clone(),
                argv,
                path: None,
                timeout_seconds: 120.0,
                max_output_bytes: 131072,
                env: Default::default(),
                pinned_files: vec![],
                parallel_safe: false,
                inherit_env: Some(
                    transport["inherit_env"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|name| name.as_str().unwrap().to_owned())
                        .collect(),
                ),
                direct: None,
            },
        );
    }
    manifest.context = serde_json::from_value(compiled.bundle.context.clone()).unwrap();
    let source = root.join("direct.eal");
    let tools = root.join("direct.tools.toml");
    fs::write(&source, &compiled.bundle.source).unwrap();
    fs::write(&tools, toml::to_string(&manifest).unwrap()).unwrap();
    let host = collector.with_file_name(format!("eal-host{}", std::env::consts::EXE_SUFFIX));
    let workspace = root.join("direct-run");
    let output = Command::new(&host)
        .args(["run"])
        .arg(&source)
        .arg("--tools")
        .arg(&tools)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--claim")
        .arg(&compiled.bundle.claim_names["current_claim"])
        .output()
        .unwrap();
    successful(&output);
    let executed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(executed["is_error"], false, "{executed}");
    let collection = &executed["result"]["collection"];
    let records = collection["records"].as_object().unwrap();
    let acquired: Vec<_> = records
        .values()
        .filter(|record| record["status"] == "ok" && record["tool"].as_str().is_some())
        .collect();
    assert_eq!(acquired.len(), 1, "{collection}");
    assert_eq!(
        acquired[0]["acquisition_request"]["input"],
        execution.acquisitions[0].arguments
    );
    assert!(acquired[0]["collected_at"].as_str().is_some());
    let assessment = &executed["result"]["assessment"];
    assert_eq!(assessment["valid"], true, "{assessment}");
    assert_eq!(
        assessment["claims"][&compiled.bundle.claim_names["current_claim"]]["status"],
        "unsupported",
        "source acquisition cannot settle independent judgement obligations"
    );
    // Reassessment uses the retained collection, with no second source invocation.
    let request = json!({"operation":"reason","source":compiled.bundle.source,"context":compiled.bundle.context,"collection_id":collection["collection_id"]});
    let mut child = Command::new(host)
        .arg("--workspace")
        .arg(&workspace)
        .arg("--registry")
        .arg(&tools)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    successful(&output);
    let reassessed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        reassessed["result"]["collection_id"],
        collection["collection_id"]
    );
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

#[test]
#[ignore = "requires Docker and the preloaded python:3.13-slim image"]
fn coding_failure_preserves_the_original_analysis_output() {
    let directory = tempfile::tempdir().unwrap();
    let analysis = directory.path().join("analysis.json");
    // Tool implementation consumes a separately authored report. Its failure
    // cannot replace that report or publish an augmented success artifact.
    saved_report(&analysis);
    let original_bytes = fs::read(&analysis).unwrap();
    let original: Value = serde_json::from_slice(&original_bytes).unwrap();
    let toolchain = directory.path().join("toolchain.json");
    profile(&toolchain, "python", "python:3.13-slim", "collector.py");
    let tool_store = directory.path().join("store");
    let tool_output = directory.path().join("implementation");
    let server = MockServer::new(vec![
        direct_draft(),
        json!({
            "language": "python",
            "files": [{"path": "collector.py", "content": "def invalid_python(:\n"}]
        }),
    ]);
    let output = implement(
        &analysis,
        &toolchain,
        &tool_store,
        &tool_output,
        &server.endpoint,
        "python",
    );
    assert!(
        !output.status.success(),
        "invalid Python unexpectedly passed collector admission"
    );
    assert_eq!(
        server.requests().len(),
        2,
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&analysis).unwrap(), original_bytes);
    assert!(original.get("tooling").is_none());
    let checkpoint: Value =
        serde_json::from_slice(&fs::read(tool_output.join("argument.analysis.json")).unwrap())
            .unwrap();
    assert_eq!(checkpoint["request"], original["request"]);
    assert_eq!(checkpoint["argument"], original["argument"]);
    assert_eq!(checkpoint["transcript"], original["transcript"]);
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
    let output_dir = directory.path().join("implementation");
    saved_report(&input);
    profile(&toolchain, "python", "python:3.13-slim", "collector.py");
    let mut settings: Value = serde_json::from_slice(&fs::read(&toolchain).unwrap()).unwrap();
    settings["timeout_seconds"] = json!(120);
    fs::write(&toolchain, serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let server = MockServer::new(vec![
        direct_draft(),
        json!({
            "language": "python",
            "files": [{"path": "collector.py", "content": "import time\ntime.sleep(120)\n"}]
        }),
    ]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "implement", "--input"])
        .arg(&input)
        .arg("--tools")
        .arg(input.parent().unwrap().join("tools.toml"))
        .arg("--eal-host")
        .arg(host())
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
        // The trusted preflight also starts a container. Wait for both model
        // responses so cancellation targets generated-source verification.
        if server.requests().len() == 2
            && let Some(name) = String::from_utf8_lossy(&running.stdout)
                .lines()
                .find(|name| name.starts_with(&prefix))
        {
            break Some(name.to_owned());
        }
        if child.try_wait().unwrap().is_some() || Instant::now() >= startup_deadline {
            break None;
        }
        thread::sleep(Duration::from_millis(200));
    };

    // Capture the daemon's mount identities before cancellation. Defer their
    // assertions until after termination so a failed inspection still stops
    // the test's own collector and attempts its resource cleanup.
    let snapshot_inspection = observed_running.as_ref().map(|name| {
        Command::new(&docker)
            .args(["container", "inspect", "--format", "{{json .Mounts}}", name])
            .output()
            .unwrap()
    });

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
    let snapshot_volumes = snapshot_inspection
        .as_ref()
        .map(|inspection| {
            successful(inspection);
            snapshot_volume_names(&serde_json::from_slice(&inspection.stdout).unwrap())
        })
        .unwrap_or_default();
    assert_snapshot_volumes_removed(&docker, &snapshot_volumes);
    assert!(
        exited_in_time,
        "launcher did not finish within 20 seconds of SIGTERM"
    );
    assert!(
        observed_running.is_some(),
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
        event["event"] == "implementation_cancelled" && event["passed"] == false
    }));
    for file in ["analysis.with-tools.json", "implementation.json"] {
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
