//! A retained ticket crosses native graph, MCP, Rust collector and EAL boundaries.
//! The generated-tool variant requires the installed Rust Docker toolchain.
#![cfg(unix)]

use engarg::engine::topology::{DebateConfig, Topology};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[path = "common/reasoning.rs"]
#[allow(dead_code)]
mod reasoning;

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
const SOURCE: &str = include_str!("fixtures/ticket-collector/main.rs");

fn selected_topologies(selection: Option<&str>) -> Result<Vec<&'static str>, String> {
    match selection {
        None | Some("all") => Ok(TOPOLOGIES.to_vec()),
        Some(selected) => TOPOLOGIES
            .iter()
            .copied()
            .find(|topology| *topology == selected)
            .map(|topology| vec![topology])
            .ok_or_else(|| {
                format!(
                    "ENGARG_TICKET_TOPOLOGIES must be all or one exact topology preset ({}); received {selected:?}",
                    TOPOLOGIES.join(", ")
                )
            }),
    }
}

fn successful(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
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
fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn fixture_script() -> PathBuf {
    repository().join("tests/fixtures/ticket_pipeline.py")
}

fn compile_collector() -> PathBuf {
    let target = repository().join("target/native-ticket-fixture");
    successful(
        &Command::new(env!("CARGO"))
            .args(["build", "--locked", "--offline", "--manifest-path"])
            .arg(repository().join("tests/fixtures/ticket-collector/Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0")
            .output()
            .unwrap(),
    );
    target.join("debug/engarg-native-collector-fixture")
}

fn prepare(
    root: &Path,
    collector: &Path,
    server: &Path,
    native: Option<&Path>,
) -> (Value, Value, Value) {
    let (_, base) = reasoning::fixture(&reasoning::cases().remove(0));
    write(
        &root.join("case.base.json"),
        &serde_json::to_value(base).unwrap(),
    );
    successful(
        &Command::new("python3")
            .arg(fixture_script())
            .arg(root)
            .arg(collector)
            .arg(server)
            .arg(native.unwrap_or(Path::new("")))
            .output()
            .unwrap(),
    );
    (
        read(&root.join("request.json")),
        read(&root.join("case.json")),
        read(&root.join("elaboration.json")),
    )
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

fn draft(data: &Value, formal: &Value) -> Value {
    let context: Value =
        serde_json::from_str(data["request"]["context"].as_str().unwrap()).unwrap();
    let mut fixtures = Vec::new();
    for (name, ids, complete) in [
        ("empty", vec![], true),
        ("singleton", vec!["one"], true),
        ("three_distinct", vec!["one", "two", "three"], true),
        ("duplicates", vec!["same", "same"], true),
        ("case_sensitive", vec!["a", "A"], true),
        ("incomplete", vec!["one", "two"], false),
    ] {
        let items = ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>();
        let distinct = ids.iter().collect::<std::collections::BTreeSet<_>>().len() == ids.len();
        fixtures.push(json!({"name":name,"kind":if name=="duplicates"{"negative"}else{"positive"},"input_json":json!({"snapshot":"demonstration-v1","items":items,"complete":complete,"formal_input":formal}).to_string(),"expected_value_json":json!({"collected":true,"snapshot":"demonstration-v1","count":ids.len(),"distinct":distinct,"complete":complete,"typed_input":formal}).to_string(),"files":[]}));
    }
    fixtures.push(json!({"name":"invalid_input","kind":"acquisition_error","input_json":json!({"items":[],"complete":true,"formal_input":formal}).to_string(),"expected_value_json":null,"files":[]}));
    let parameters = context["input_schema"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .collect::<Vec<_>>();
    json!({"purpose":context["operation_purpose"],"parameters":parameters,"input_schema_json":context["input_schema"].to_string(),"result_schema_json":context["output_schema"].to_string(),"arguments":[],"unresolved_arguments":parameters,"fixtures":fixtures})
}

struct Provider {
    endpoint: String,
    stop: Arc<AtomicBool>,
    calls: Arc<Mutex<Vec<Value>>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Provider {
    fn new(case: Value, elaboration: Value, broken_code: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_calls = Arc::clone(&calls);
        let formal = elaboration["acquisitions"][1]["arguments"]["formal_input"].clone();
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
                    json!({"argument":case,"support_elaboration_json":elaboration.to_string()})
                } else if properties.contains_key("fixtures") {
                    draft(&data, &formal)
                } else if properties.contains_key("files") {
                    json!({"language":"rust","files":[{"path":"main.rs","content":if broken_code{"this is invalid Rust"}else{SOURCE}}]})
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
        if let Some(worker) = self.thread.take() {
            worker.join().unwrap();
        }
    }
}

fn configuration(
    root: &Path,
    provider: &Provider,
    request: Value,
    topology: &str,
    generated: bool,
) -> Value {
    let host = executable("ENGARG_EAL_HOST");
    let mut implementation_args = vec![
        "--model".to_owned(),
        "fixture".into(),
        "--endpoint".into(),
        provider.endpoint.clone(),
        "--tool-language".into(),
        "rust".into(),
        "--toolchain".into(),
        repository()
            .join("examples/toolchains/rust.json")
            .to_string_lossy()
            .into_owned(),
        "--tool-store".into(),
        root.join("tools").to_string_lossy().into_owned(),
        "--tool-max-calls".into(),
        "2".into(),
        "--tool-timeout-secs".into(),
        "600".into(),
        "--http-retries".into(),
        "0".into(),
    ];
    if !generated {
        implementation_args.clear();
    }
    let environment = [
        "PATH",
        "HOME",
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_CONFIG",
        "USER",
    ]
    .into_iter()
    .filter(|name| std::env::var_os(name).is_some())
    .collect::<Vec<_>>();
    json!({"schema_version":"engarg.ticket/1","ticket_id":format!("ticket_{topology}"),"request":request,
        "debate":DebateConfig{participants:3,rounds:1,topology:Topology::preset(topology,3).unwrap(),adaptation:Default::default()},
        "evidence_config":root.join("planning.json"),"authoring_args":["--model","fixture","--endpoint",provider.endpoint,"--http-retries","0"],
        "implementation_args":implementation_args,"mcp_registrations":root.join("mcp-registrations.json"),"qualification_argv":["/usr/bin/python3",fixture_script(),"qualify"],
        "eal_host":host,"eal_application":host.with_file_name("eal"),"environment":environment,"collector_environment":[],"review_binding":root.join("review-binding.json"),
        "timeout_seconds":3600,"stage_timeout_seconds":900,"input_files":[fixture_script(),repository().join("scripts/catalogue_evidence_example.py"),root.join("case.json"),root.join("catalogue.json")]})
}

fn run_ticket(config: &Value, root: &Path) -> (Output, Value, PathBuf) {
    let spec = root.join("ticket.json");
    write(&spec, config);
    let workspace = root.join("run");
    let installed = root.join("installed");
    fs::create_dir(&installed).unwrap();
    for name in ["engarg", "engarg-process-supervisor"] {
        fs::copy(
            repository().join("target/debug").join(name),
            installed.join(name),
        )
        .unwrap();
    }
    let result = Command::new(installed.join("engarg"))
        .args(["ticket", "--spec"])
        .arg(&spec)
        .arg("--run")
        .arg(&workspace)
        .arg("--eal-coordinate")
        .arg(executable("ENGARG_EAL_COORDINATE"))
        .output()
        .unwrap();
    let saved = workspace.join("ticket-result.json");
    let report = if saved.exists() {
        read(&saved)
    } else {
        Value::Null
    };
    (result, report, workspace)
}

fn acceptance(generated: bool) {
    let selection = std::env::var_os("ENGARG_TICKET_TOPOLOGIES").map(|value| {
        value
            .into_string()
            .expect("ENGARG_TICKET_TOPOLOGIES must contain valid Unicode")
    });
    let topologies =
        selected_topologies(selection.as_deref()).unwrap_or_else(|message| panic!("{message}"));
    let collector = executable("EAL_COLLECTOR");
    let mcp = executable("ENGARG_CATALOGUE_MCP");
    let native = (!generated).then(compile_collector);
    for topology in topologies {
        eprintln!("Executing complete ticket for {topology} (generated tool: {generated})");
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (request, case, elaboration) = prepare(root, &collector, &mcp, native.as_deref());
        assert!(
            read(&root.join("connections.json"))["connections"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let provider = Provider::new(case, elaboration, false);
        let config = configuration(root, &provider, request, topology, generated);
        let (output, report, workspace) = run_ticket(&config, root);
        if !output.status.success() {
            let retained = directory.keep();
            panic!(
                "Ticket failed at {}\nstdout:{}\nstderr:{}",
                retained.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(report["status"], "completed", "{report}");
        assert_eq!(report["outcome"]["claim_status"], "supported", "{report}");
        assert_eq!(report["outcome"]["complete"], true, "{report}");
        let analysis = read(&workspace.join("author/analysis.json"));
        assert_eq!(
            analysis["run_config"]["debate"],
            serde_json::to_value(config["debate"].clone()).unwrap()
        );
        let ready = read(&workspace.join("integrate/analysis.ready.json"));
        assert!(
            ready["evidence_planning"]["routes"]["gaps"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{ready}"
        );
        let registered = read(&workspace.join("integrate/connections.ready.json"));
        assert!(!registered["connections"].as_array().unwrap().is_empty());
        assert!(
            !registered["native_providers"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            root.join("mcp-requests.jsonl").is_file(),
            "real MCP source was not invoked"
        );
        let reviews = fs::read_to_string(root.join("review-decisions.jsonl")).unwrap();
        let reviews = reviews
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reviews.len(), 5);
        assert!(reviews.iter().all(|review| review["accepted"] == true));
        assert!(workspace.join("execute/source-registration.json").is_file());
        assert!(workspace.join("execute/evidence-collection.json").is_file());
        assert!(
            workspace
                .join("execute/registered-assessment.json")
                .is_file()
        );
        let known = read(&workspace.join("execute/registered-assessment.json"));
        assert_eq!(
            known["result"]["source_digest"],
            report["outcome"]["source_digest"]
        );
        let calls = provider.calls.lock().unwrap();
        let debate: DebateConfig = serde_json::from_value(config["debate"].clone()).unwrap();
        let graph = debate.graphs().unwrap().remove(0);
        let peer_calls = calls
            .iter()
            .filter(|data| data["debate"]["round"] == 1)
            .collect::<Vec<_>>();
        assert_eq!(peer_calls.len(), 3);
        for data in peer_calls {
            let participant = data["debate"]["participant"].as_u64().unwrap() as usize;
            let incoming = graph
                .edges
                .iter()
                .filter(|edge| edge.to == participant)
                .map(|edge| edge.from)
                .collect::<Vec<_>>();
            assert_eq!(data["debate"]["incoming"], json!(incoming));
            assert_eq!(
                data["peer_assessments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|peer| peer["participant"].as_u64().unwrap() as usize)
                    .collect::<Vec<_>>(),
                incoming
            );
        }
        if generated {
            let implemented = read(&workspace.join("integrate/tooling/analysis.with-tools.json"));
            assert_eq!(implemented["tooling"]["model_calls"], 2);
            assert_eq!(implemented["tooling"]["capabilities"][0]["reused"], false);
            assert_eq!(
                ready["replanning"]["previous_extensions"]["tooling"], implemented["tooling"],
                "final route replanning must retain the verified tooling provenance"
            );
            assert_eq!(calls.len(), 11, "9 authoring + draft + code");
        } else {
            assert_eq!(calls.len(), 9);
        }
        eprintln!("Completed {topology}: measured evidence supports the registered claim");
    }
}

#[test]
fn topology_selection_keeps_the_requested_acceptance_coverage() {
    assert_eq!(selected_topologies(None).unwrap(), TOPOLOGIES);
    assert_eq!(selected_topologies(Some("all")).unwrap(), TOPOLOGIES);
    for topology in TOPOLOGIES {
        assert_eq!(selected_topologies(Some(topology)).unwrap(), vec![topology]);
    }
    for invalid in ["", " ", "Ring", "ring,tree", "unknown"] {
        assert!(selected_topologies(Some(invalid)).is_err(), "{invalid:?}");
    }
}

#[test]
#[ignore = "requires native EAL companions and catalogue_mcp; compiles a real Rust collector"]
fn every_topology_registers_a_new_mcp_and_executes_a_compiled_collector_in_one_ticket() {
    acceptance(false);
}

#[test]
#[ignore = "requires native EAL companions, catalogue_mcp and preloaded Rust Docker toolchain"]
fn every_topology_generates_compiles_registers_and_executes_its_missing_rust_tool() {
    acceptance(true);
}

#[test]
#[ignore = "requires native EAL companions and catalogue_mcp"]
fn unavailable_new_mcp_blocks_before_tool_generation_or_evidence_execution() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (request, case, elaboration) = prepare(
        root,
        &executable("EAL_COLLECTOR"),
        &executable("ENGARG_CATALOGUE_MCP"),
        None,
    );
    // Keep the exact approved transport and descriptor fingerprint, but make the
    // actual newly activated MCP process fail while opening its source data.
    fs::write(root.join("catalogue.json"), b"invalid catalogue JSON").unwrap();
    let provider = Provider::new(case, elaboration, false);
    let config = configuration(root, &provider, request, "ring", true);
    let (output, report, workspace) = run_ticket(&config, root);
    let calls = provider.calls.lock().unwrap().len();
    let collected = workspace.join("execute/evidence-collection.json").exists();
    if output.status.success() || report["status"] != "blocked" || calls != 9 || collected {
        let retained = directory.keep();
        panic!(
            "Unavailable MCP did not block before tool generation and evidence execution at {}\nstatus: {}\nprovider calls: {calls}\nevidence collected: {collected}\nreport: {report}\nstdout: {}\nstderr: {}",
            retained.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires native EAL companions and catalogue_mcp; compiles a real Rust collector"]
fn a_measured_negative_completes_the_ticket_with_an_unsupported_claim() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let native = compile_collector();
    let (request, case, elaboration) = prepare(
        root,
        &executable("EAL_COLLECTOR"),
        &executable("ENGARG_CATALOGUE_MCP"),
        Some(&native),
    );
    let mut snapshot = read(&root.join("catalogue.json"));
    snapshot["items"][2]["id"] = json!("alpha");
    write(&root.join("catalogue.json"), &snapshot);
    let provider = Provider::new(case, elaboration, false);
    let config = configuration(root, &provider, request, "ring", false);
    let (output, report, workspace) = run_ticket(&config, root);
    if !output.status.success() {
        let retained = directory.keep();
        panic!(
            "Negative ticket failed at {}\n{}\n{}",
            retained.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(report["outcome"]["complete"], true, "{report}");
    assert_eq!(report["outcome"]["claim_status"], "unsupported", "{report}");
    let assessment = read(&workspace.join("execute/assessment.json"));
    assert_eq!(
        assessment["result"]["arguments"]["native_method_argument_5731"]["reasoning_result"]["status"],
        "not_run"
    );
    let collection = read(&workspace.join("execute/evidence-collection.json"));
    let refusal = &collection["result"]["records"]["reasoning_input_5731"];
    assert_eq!(refusal["status"], "ok");
    assert_eq!(
        refusal["value"]["schema"],
        "engarg.reasoning-input-denied/1"
    );
    assert!(refusal["value"].get("payload").is_none());
}

#[test]
#[ignore = "requires native EAL companions, catalogue_mcp and preloaded Rust Docker toolchain"]
fn a_rejected_compilation_blocks_before_evidence_execution() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (request, case, elaboration) = prepare(
        root,
        &executable("EAL_COLLECTOR"),
        &executable("ENGARG_CATALOGUE_MCP"),
        None,
    );
    let provider = Provider::new(case, elaboration, true);
    let config = configuration(root, &provider, request, "ring", true);
    let (output, report, workspace) = run_ticket(&config, root);
    assert!(!output.status.success());
    assert_eq!(report["status"], "blocked", "{report}");
    assert_eq!(provider.calls.lock().unwrap().len(), 11);
    assert!(!workspace.join("execute/evidence-collection.json").exists());
}

#[test]
fn ticket_fixture_has_a_bound_method_and_a_complete_support_design() {
    let directory = tempfile::tempdir().unwrap();
    let (request, case, elaboration) = prepare(
        directory.path(),
        Path::new("/fixture/eal-collector"),
        Path::new("/fixture/catalogue-mcp"),
        None,
    );
    let request: engarg::domain::Request = serde_json::from_value(request).unwrap();
    let case: engarg::domain::ArgumentCase = serde_json::from_value(case).unwrap();
    engarg::domain::validate_case(&request, &case).unwrap();
    let elaboration: engarg::support_requirements::SupportElaboration =
        serde_json::from_value(elaboration).unwrap();
    let errors = engarg::support_requirements::validate_elaboration(&case, &elaboration);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn finite_review_policy_rejects_changed_interpretations_and_adverse_source_facts() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (_, case, _) = prepare(
        root,
        Path::new("/fixture/eal-collector"),
        Path::new("/fixture/catalogue-mcp"),
        None,
    );
    let digest = "a".repeat(64);
    let request = json!({
        "evidence_id":"ground_review_4732", "environment":"engarg_case",
        "tool":"engarg_review_contract", "tool_version":"1",
        "input":{"schema":"engarg.eal-export/3", "case_digest":digest,
            "obligation":{"id":"ground_interpretation:G2", "kind":"ground_interpretation",
                "target_id":"G2", "eal_evidence_name":"ground_review_4732", "source_evidence_ids":[]},
            "authored_detail":case["grounds"][1]},
        "context":{"engarg_export_digest":digest},
    });
    let review = |request: &Value| {
        let mut child = Command::new("python3")
            .arg(fixture_script())
            .arg("review")
            .arg(root)
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
        let output = child.wait_with_output().unwrap();
        successful(&output);
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let positive = review(&request);
    assert_eq!(positive["value"]["accepted"], true);
    assert_eq!(positive["context"], request["context"]);
    assert_eq!(positive["request"]["input"], request["input"]);
    let mut changed = request.clone();
    changed["input"]["authored_detail"]["statement"] = json!("An unrelated interpretation.");
    assert_eq!(review(&changed)["value"]["accepted"], false);
    let mut snapshot = read(&root.join("catalogue.json"));
    snapshot["items"][2]["id"] = json!("alpha");
    write(&root.join("catalogue.json"), &snapshot);
    assert_eq!(review(&request)["value"]["accepted"], false);
}
