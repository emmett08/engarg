//! Current-case preparation feeds the normal exporter and the real adapter readers.
use engarg::{
    acquisition::{AcquisitionPlan, InputPreparation, ScopedOperation},
    domain::{ArgumentCase, Request},
    eal_acquisition::reconstruct_bundle,
    eal_export::EalExport,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

fn write(path: &Path, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn passed(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture(path: &Path) -> ArgumentCase {
    let request: Request = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    write(
        path,
        &json!({"request":request,"argument":case,"transcript":[]}),
    );
    case
}

fn plan(id: &str, tool: &str, preparation: Option<InputPreparation>) -> AcquisitionPlan {
    AcquisitionPlan {
        need_id: id.into(),
        tool_id: tool.into(),
        arguments: vec![],
        unresolved_arguments: vec!["inputs".into()],
        input_preparation: preparation,
    }
}

fn acquisition(bundle: &EalExport, id: &str) -> Value {
    json!({
        "evidence_id":format!("acquisition_{id}"),"environment":"engarg_case",
        "tool":format!("acquisition_tool_{id}"),"tool_version":"1",
        "input":{"schema":"engarg.eal-acquisition/1","case_digest":bundle.revision,
            "need":{"id":id},"arguments":{}},
        "context":bundle.context,
    })
}

#[tokio::test]
async fn new_export_materialises_current_bindings_without_creating_missing_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let report = root.join("analysis.json");
    let case = fixture(&report);
    let plans = root.join("plans.json");
    write(
        &plans,
        &vec![
            plan(
                &case.evidence_plan[0].id,
                "scoped_json",
                Some(InputPreparation::Scoped {
                    operation: ScopedOperation::GrafanaSearch,
                }),
            ),
            plan(
                &case.evidence_plan[1].id,
                "scoped_json",
                Some(InputPreparation::Scoped {
                    operation: ScopedOperation::GrafanaTrace,
                }),
            ),
            plan(
                &case.evidence_plan[2].id,
                "artifact_json",
                Some(InputPreparation::Artifact {
                    reference: "retained_measurement".into(),
                }),
            ),
            plan(&case.evidence_plan[3].id, "artifact_json", None),
        ],
    );
    let source = root.join("operator");
    fs::create_dir(&source).unwrap();
    let bytes = b"{\"measured\":false}";
    fs::write(source.join("capture.json"), bytes).unwrap();
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let query = "{ span.service.name = \"catalog  sync\" }\n";
    let inputs = root.join("operator.snapshot.json");
    write(
        &inputs,
        &json!({
            "case_digest":"old-case","needs":{"old-need":"grafana_trace"},
            "kubernetes":{"context":"dev","resource":"applications.example.invalid","kind":"Application","namespace":"development","name":"catalog-sync-operator"},
            "grafana":{"origin":origin,"datasource_uid":"tempo","query":query,"start_unix":1000,"end_unix":2000},
            "artifacts":{"retained_measurement":{"artifacts":[{"path":"capture.json","sha256":hash,
                "observed_at":"2026-10-24T12:00:00Z","source":"separately retained instrument capture"}]}}
        }),
    );
    let directory = root.join("collector-inputs");
    let bundle_path = root.join("argument.bundle.json");
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["eal", "export", "--input"])
        .arg(&report)
        .arg("--acquisition-plan")
        .arg(&plans)
        .arg("--operator-inputs")
        .arg(&inputs)
        .arg("--operator-input-base")
        .arg(&source)
        .arg("--preparation-output-dir")
        .arg(&directory)
        .arg("--output")
        .arg(root.join("argument.eal"))
        .arg("--manifest")
        .arg(&bundle_path)
        .arg("--tools-output")
        .arg(root.join("argument.tools.toml"))
        .output()
        .unwrap();
    passed(&output);
    let bundle: EalExport = serde_json::from_value(read(&bundle_path)).unwrap();
    assert_eq!(
        serde_json::to_value(reconstruct_bundle(&bundle).unwrap()).unwrap(),
        read(&bundle_path)
    );
    let scoped = read(&directory.join("scoped-inputs.json"));
    assert_eq!(scoped["case_digest"], bundle.revision);
    assert_eq!(scoped["grafana"]["query"], query);
    assert!(scoped["needs"].get("old-need").is_none());
    assert_eq!(scoped["needs"][&case.evidence_plan[0].id], "grafana_search");
    let preparation = read(&directory.join("collector-preparation.json"));
    assert_eq!(preparation["case_digest"], bundle.revision);
    assert_eq!(
        preparation["summary"],
        json!({"bound":2,"unresolved":2,"retained_artifacts_verified":1})
    );
    assert_eq!(preparation["evidence_collected"], false);
    assert_eq!(
        read(&directory.join("review-inputs.json"))["reviews"],
        json!([])
    );
    let bound = bundle.acquisition_plan.as_ref().unwrap();
    assert!(bound.entries[0].unresolved_arguments.is_empty());
    assert_eq!(bound.entries[1].unresolved_arguments, ["inputs"]);
    assert_eq!(bound.entries[3].unresolved_arguments, ["inputs"]);
    let artifact = engarg::retained_collectors::observe(
        "artifact_json",
        acquisition(&bundle, &case.evidence_plan[2].id),
        &directory.join("artifact-inputs.json"),
    )
    .unwrap();
    assert_eq!(artifact["value"]["collected"], true);
    fs::write(source.join("capture.json"), b"{\"measured\":true}").unwrap();
    assert!(
        engarg::retained_collectors::observe(
            "artifact_json",
            acquisition(&bundle, &case.evidence_plan[2].id),
            &directory.join("artifact-inputs.json"),
        )
        .is_err()
    );
    let mut old = acquisition(&bundle, &case.evidence_plan[0].id);
    old["input"]["case_digest"] = json!("old-case");
    assert!(
        engarg::scoped_collectors::observe(old, &directory.join("scoped-inputs.json"))
            .await
            .is_err()
    );
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("mock Grafana did not receive request: {error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut wire = Vec::new();
        while !wire.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut chunk = [0u8; 1024];
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0);
            wire.extend_from_slice(&chunk[..count]);
            assert!(wire.len() < 16_384);
        }
        let body = "{\"traces\":[]}";
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        String::from_utf8(wire).unwrap()
    });
    let measurement = engarg::scoped_collectors::observe(
        acquisition(&bundle, &case.evidence_plan[0].id),
        &directory.join("scoped-inputs.json"),
    )
    .await
    .unwrap();
    assert_eq!(
        measurement["value"]["observation"]["data"],
        json!({"traces":[]})
    );
    let wire = server.join().unwrap();
    let target = wire
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let url = reqwest::Url::parse(&format!("{origin}{target}")).unwrap();
    assert_eq!(
        url.query_pairs().find(|(name, _)| name == "q").unwrap().1,
        query
    );
    assert!(read(&report).get("tooling").is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(directory.join("scoped-inputs.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_preparation_preserves_operator_files_and_leaves_no_partial_export() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let report = root.join("analysis.json");
    fixture(&report);
    let inputs = root.join("inputs.json");
    write(&inputs, &json!([]));
    let directory = root.join("new-inputs");
    let source = root.join("argument.eal");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "export", "--input"])
            .arg(&report)
            .arg("--operator-inputs")
            .arg(&inputs)
            .arg("--preparation-output-dir")
            .arg(&directory)
            .arg("--output")
            .arg(&source)
            .output()
            .unwrap()
    };
    assert!(!invoke().status.success());
    assert!(!directory.exists());
    assert!(!source.exists());
    assert!(!root.join("argument.tools.toml").exists());
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("operator-owned"), b"preserve").unwrap();
    assert!(!invoke().status.success());
    assert_eq!(
        fs::read(directory.join("operator-owned")).unwrap(),
        b"preserve"
    );
}
