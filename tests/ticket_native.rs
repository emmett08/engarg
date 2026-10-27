//! Native ticket lifecycle uses direct TOML, standalone EAL and the execute MCP operation.
#![cfg(unix)]
use engarg::engine::topology::{DebateConfig, Topology};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
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
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn executable(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("Set {name}")))
        .canonicalize()
        .unwrap()
}
fn selected_topologies(selection: Option<&str>) -> Result<Vec<&'static str>, String> {
    match selection {
        None | Some("all") => Ok(TOPOLOGIES.to_vec()),
        Some(name) => TOPOLOGIES
            .iter()
            .copied()
            .find(|value| *value == name)
            .map(|value| vec![value])
            .ok_or_else(|| format!("Unknown topology {name}")),
    }
}
fn request(stream: &mut TcpStream) -> Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut bytes = Vec::new();
    let (end, length) = loop {
        let mut buffer = [0; 8192];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "HTTP input ended inside its headers");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 4 * 1024 * 1024);
        if let Some(start) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..start]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (start + 4, length);
        }
    };
    while bytes.len() - end < length {
        let mut buffer = [0; 8192];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "HTTP input ended inside its body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    serde_json::from_slice(&bytes[end..end + length]).unwrap()
}

struct Provider {
    endpoint: String,
    stop: Arc<AtomicBool>,
    calls: Arc<Mutex<Vec<Value>>>,
    thread: Option<thread::JoinHandle<()>>,
}

fn authoring_response(case: &Value, elaboration: &Value) -> Value {
    json!({
        "argument": live_output::generation_value(case),
        "support_elaboration_json": elaboration.to_string(),
    })
}

impl Provider {
    fn new(case: Value, elaboration: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_calls = Arc::clone(&calls);
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                let body = request(&mut stream);
                let data: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
                let schema = &body["text"]["format"]["schema"];
                let properties = schema["properties"].as_object().unwrap();
                let result = if properties.contains_key("support_elaboration_json") {
                    authoring_response(&case, &elaboration)
                } else {
                    json!({"findings":[],"objections":[],"evidence_gaps":[]})
                };
                let validator = jsonschema::validator_for(schema).unwrap();
                let errors = validator
                    .iter_errors(&result)
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>();
                assert!(errors.is_empty(), "{}", errors.join("; "));
                thread_calls.lock().unwrap().push(data);
                let response = json!({"id":"ticket-fixture","model":"fixture","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":result.to_string()}]}]}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            }
        });
        Self {
            endpoint,
            stop,
            calls,
            thread: Some(worker),
        }
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Preserve the original assertion while unwinding instead of aborting
        // the whole test process with a second panic in this destructor.
        if let Some(worker) = self.thread.take()
            && let Err(error) = worker.join()
            && !thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}

fn prepare(root: &Path, negative: bool, unavailable: bool) -> (Value, Value, Value) {
    let (request, case, elaboration) = executable_case::fixture();
    fs::create_dir(root.join("tools")).unwrap();
    fs::write(root.join("tools/source.py"),r#"import json,sys
from datetime import datetime, timezone
from pathlib import Path
request=json.load(sys.stdin)
fixture=json.loads(Path(__file__).with_name('snapshot.json').read_text())
if fixture['unavailable']:
    raise RuntimeError('fixture source unavailable')
if request['input']['operation']=='premise_source':
    ids=fixture['items']
    value={'measurement':{'count':len(ids),'distinct':len(set(ids))==len(ids)}}
else:
    value={'applies':True}
print(json.dumps({'value':value,'observed_at':datetime.now(timezone.utc).isoformat().replace('+00:00','Z')}))
"#).unwrap();
    write(
        &root.join("tools/snapshot.json"),
        &json!({"items":if negative {vec!["a","b","a"]} else {vec!["a","b","c"]},"unavailable":unavailable}),
    );
    fs::write(root.join("tools.toml"),"[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n").unwrap();
    let mut case = serde_json::to_value(case).unwrap();
    case["warrants"][0]["reasoning_method"]["conclusion"]["valid_from"] =
        json!("2020-01-01T00:00:00Z");
    case["warrants"][0]["reasoning_method"]["conclusion"]["valid_until"] =
        json!("2100-01-01T00:00:00Z");
    (
        serde_json::to_value(request).unwrap(),
        case,
        serde_json::to_value(elaboration).unwrap(),
    )
}
fn configuration(root: &Path, provider: &Provider, request: Value, topology: &str) -> Value {
    json!({"schema_version":"engarg.ticket/2","ticket_id":format!("ticket_{topology}"),"request":request,
        "debate":DebateConfig{participants:3,rounds:1,topology:Topology::preset(topology,3).unwrap(),adaptation:Default::default()},
        "tools":root.join("tools.toml"),"authoring_args":["--model","fixture","--endpoint",provider.endpoint,"--http-retries","0"],
        "eal_host":executable("ENGARG_EAL_HOST"),"environment":["PATH","HOME"],"collector_environment":["PATH"],
        "timeout_seconds":900,"stage_timeout_seconds":180})
}
fn run_ticket(config: &Value, root: &Path) -> (Output, Value, PathBuf) {
    write(&root.join("ticket.json"), config);
    let installed = root.join("installed");
    fs::create_dir(&installed).unwrap();
    for (name, source) in [
        ("engarg", env!("CARGO_BIN_EXE_engarg")),
        (
            "engarg-process-supervisor",
            env!("CARGO_BIN_EXE_engarg-process-supervisor"),
        ),
    ] {
        fs::copy(source, installed.join(name)).unwrap();
    }
    let workspace = root.join("run");
    let output = Command::new(installed.join("engarg"))
        .args(["ticket", "--spec"])
        .arg(root.join("ticket.json"))
        .arg("--run")
        .arg(&workspace)
        .arg("--eal-coordinate")
        .arg(executable("ENGARG_EAL_COORDINATE"))
        .output()
        .unwrap();
    let path = workspace.join("ticket-result.json");
    let report = if path.is_file() {
        read(&path)
    } else {
        Value::Null
    };
    (output, report, workspace)
}
fn assert_output(output: &Output, report: &Value, root: &Path) {
    assert!(
        output.status.success(),
        "ticket {} failed\n{report}\nstdout {}\nstderr {}",
        root.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn direct_ticket_fixture_is_valid_and_contains_no_acquired_formal_query() {
    let root = tempfile::tempdir().unwrap();
    let (request, case, elaboration) = prepare(root.path(), false, false);
    let response = authoring_response(&case, &elaboration);
    let validator =
        jsonschema::validator_for(&engarg::support_requirements::envelope_schema()).unwrap();
    assert!(validator.is_valid(&response));
    let request = serde_json::from_value(request).unwrap();
    let case = serde_json::from_value(case).unwrap();
    let elaboration = serde_json::from_value(elaboration).unwrap();
    engarg::domain::validate_case(&request, &case).unwrap();
    assert!(engarg::support_requirements::validate_elaboration(&case, &elaboration).is_empty());
    assert!(
        !serde_json::to_string(&elaboration)
            .unwrap()
            .contains("formal_source")
    );
}
#[test]
fn topology_selection_is_explicit() {
    assert_eq!(selected_topologies(Some("ring")).unwrap(), vec!["ring"]);
    assert_eq!(selected_topologies(None).unwrap().len(), 9);
    assert!(selected_topologies(Some("bogus")).is_err());
}
#[test]
#[ignore = "requires EAL companions; set ENGARG_EAL_HOST and ENGARG_EAL_COORDINATE"]
fn direct_ticket_executes_through_every_selected_native_topology() {
    let selection = std::env::var("ENGARG_TICKET_TOPOLOGIES").ok();
    for topology in selected_topologies(selection.as_deref()).unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (request, case, elaboration) = prepare(root, false, false);
        let provider = Provider::new(case, elaboration);
        let config = configuration(root, &provider, request, topology);
        let (output, report, workspace) = run_ticket(&config, root);
        if !output.status.success() {
            let retained = directory.keep();
            assert_output(&output, &report, &retained);
            unreachable!();
        }
        assert_eq!(report["status"], "completed");
        assert_eq!(report["outcome"]["claim_status"], "supported");
        let execution = read(&workspace.join("execute/execution.json"));
        assert_eq!(execution["result"]["schema"], "EAL/execution/1");
        assert_eq!(
            execution["result"]["source_digest"],
            report["outcome"]["source_digest"]
        );
        assert!(!workspace.join("execute/source-registration.json").exists());
        assert!(!workspace.join("integrate/connections.ready.json").exists());
        assert!(workspace.join("export/tools/source.py").is_file());
        let calls = provider.calls.lock().unwrap();
        let debate: DebateConfig = serde_json::from_value(config["debate"].clone()).unwrap();
        let graph = debate.graphs().unwrap().remove(0);
        let peers = calls
            .iter()
            .filter(|data| data["debate"]["round"] == 1)
            .collect::<Vec<_>>();
        assert_eq!(peers.len(), 3);
        for data in peers {
            let participant = data["debate"]["participant"].as_u64().unwrap() as usize;
            let incoming = graph
                .edges
                .iter()
                .filter(|edge| edge.to == participant)
                .map(|edge| edge.from)
                .collect::<Vec<_>>();
            assert_eq!(data["debate"]["incoming"], json!(incoming));
        }
        assert_eq!(calls.len(), 9);
    }
}
#[test]
#[ignore = "requires EAL companions; set ENGARG_EAL_HOST and ENGARG_EAL_COORDINATE"]
fn measured_negative_completes_and_unavailable_source_blocks() {
    for unavailable in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (request, case, elaboration) = prepare(root, true, unavailable);
        let provider = Provider::new(case, elaboration);
        let config = configuration(root, &provider, request, "ring");
        let (output, report, workspace) = run_ticket(&config, root);
        assert_eq!(
            output.status.success(),
            !unavailable,
            "{report}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            report["status"],
            if unavailable { "blocked" } else { "completed" }
        );
        assert_eq!(report["outcome"]["complete"], !unavailable);
        assert_eq!(report["outcome"]["claim_status"], "unsupported");
        assert!(workspace.join("execute/execution.json").is_file());
    }
}
#[test]
#[ignore = "requires EAL companions; set ENGARG_EAL_HOST and ENGARG_EAL_COORDINATE"]
fn unbound_operation_blocks_before_export_or_evidence_collection() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (request, case, elaboration) = prepare(root, false, false);
    fs::write(root.join("tools.toml"), "").unwrap();
    let provider = Provider::new(case, elaboration);
    let config = configuration(root, &provider, request, "ring");
    let (output, report, workspace) = run_ticket(&config, root);
    assert!(!output.status.success());
    assert_eq!(report["status"], "blocked", "{report}");
    let integration = read(&workspace.join("integrate/integration.json"));
    assert_eq!(integration["reason"], "required_operation_unbound");
    assert!(!workspace.join("export").exists());
    assert!(!workspace.join("execute").exists());
}
