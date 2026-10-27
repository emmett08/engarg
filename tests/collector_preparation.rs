//! Current-case configuration is independent of implementation and evidence availability.
use engarg::{
    acquisition::{AcquisitionPlan, InputPreparation, ScopedOperation, ToolArgument},
    collector_preparation::prepare_plans,
    domain::{ArgumentCase, Request},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
};

const FINAL_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const QUERY: &str = "{\n  span.resource.name = \"catalog-sync-operator\"\n}\r\n";

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn inputs() -> Value {
    json!({
        "schema":"engarg.scoped-inputs/1", "case_digest":"old-case-digest",
        "needs":{"previous_need":"grafana_trace"},
        "kubernetes":{"context":"dev","resource":"applications.example.invalid",
            "kind":"Application","name":"catalog-sync-operator","namespace":"development"},
        "grafana":{"origin":"https://grafana.example.invalid","datasource_uid":"tempo",
            "query":QUERY,"start_unix":1000,"end_unix":2000},
        "private_setting":"DO_NOT_COPY_OPERATOR_SECRET",
    })
}

fn plan(id: &str, tool: &str, input_preparation: Option<InputPreparation>) -> AcquisitionPlan {
    AcquisitionPlan {
        need_id: id.into(),
        tool_id: tool.into(),
        arguments: Vec::new(),
        unresolved_arguments: vec![
            if tool == "custom" {
                "adapter"
            } else {
                "inputs"
            }
            .into(),
        ],
        input_preparation,
    }
}

fn scoped(id: &str, operation: ScopedOperation) -> AcquisitionPlan {
    plan(
        id,
        "scoped_json",
        Some(InputPreparation::Scoped { operation }),
    )
}

fn artifact(id: &str, reference: Option<&str>) -> AcquisitionPlan {
    plan(
        id,
        "artifact_json",
        reference.map(|reference| InputPreparation::Artifact {
            reference: reference.into(),
        }),
    )
}

fn value(files: &BTreeMap<String, Vec<u8>>, name: &str) -> Value {
    serde_json::from_slice(&files[name]).unwrap()
}

fn report_entry<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["need_id"] == id)
        .unwrap()
}

fn write_files(directory: &Path, files: &BTreeMap<String, Vec<u8>>) {
    for (name, bytes) in files {
        fs::write(directory.join(name), bytes).unwrap();
    }
}

fn request(id: &str, digest: &str) -> Value {
    json!({"evidence_id":format!("acquisition_{id}"),"environment":"engarg_case",
        "tool":format!("acquisition_tool_{id}"),"tool_version":"1",
        "input":{"schema":"engarg.eal-acquisition/1","case_digest":digest,"need":{"id":id}},
        "context":{"engarg_export_digest":digest}})
}

#[test]
fn current_typed_bindings_ignore_old_digests_maps_and_unrelated_private_settings() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("new-run");
    let plans = vec![
        scoped("N1", ScopedOperation::GrafanaSearch),
        scoped("N2", ScopedOperation::GrafanaTrace),
        artifact("N3", None),
    ];
    let prepared = prepare_plans(&case, &plans, &inputs(), directory.path(), &output).unwrap();
    assert_eq!(
        prepared.plans[0].arguments[0].value,
        output.join("scoped-inputs.json").to_str().unwrap()
    );
    assert!(prepared.plans[0].unresolved_arguments.is_empty());
    assert_eq!(prepared.plans[1].unresolved_arguments, ["inputs"]);
    assert_eq!(prepared.plans[2].unresolved_arguments, ["inputs"]);
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    let scoped = value(&files, "scoped-inputs.json");
    assert_eq!(scoped["case_digest"], FINAL_DIGEST);
    assert_eq!(
        scoped["needs"],
        json!({"N1":"grafana_search","N2":"grafana_trace"})
    );
    assert_eq!(scoped["grafana"]["query"], QUERY);
    assert_eq!(scoped["grafana"]["rollout_kind"], "Application");
    assert_eq!(scoped["grafana"]["trace_ids"], json!([]));
    assert!(
        !String::from_utf8(files["scoped-inputs.json"].clone())
            .unwrap()
            .contains("DO_NOT_COPY_OPERATOR_SECRET")
    );
    let report = value(&files, "collector-preparation.json");
    assert_eq!(report["case_digest"], FINAL_DIGEST);
    assert_eq!(
        report["ignored_legacy_binding_fields"],
        json!(["case_digest", "needs"])
    );
    assert_eq!(report["evidence_collected"], false);
    assert_eq!(report["independent_reviews_created"], false);
    assert_eq!(report_entry(&report, "N1")["configuration"], "bound");
    assert_eq!(report_entry(&report, "N1")["evidence"], "not_collected");
    assert!(
        report_entry(&report, "N2")["gaps"]
            .to_string()
            .contains("trace_ids")
    );
    assert!(
        report_entry(&report, "N3")["gaps"]
            .to_string()
            .contains("producer unidentified")
    );
    assert_eq!(report_entry(&report, "N4")["implementation"], "missing");
}

#[tokio::test]
async fn current_case_identity_and_need_mapping_execute_the_literal_search() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut configuration = inputs();
    configuration.as_object_mut().unwrap().remove("kubernetes");
    configuration["grafana"]["origin"] =
        json!(format!("http://{}", listener.local_addr().unwrap()));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        let wire = String::from_utf8(bytes).unwrap();
        let target = wire
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://localhost{target}")).unwrap();
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "q").unwrap().1,
            QUERY
        );
        let body = r#"{"traces":[]}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let prepared = prepare_plans(
        &case,
        &[scoped("N1", ScopedOperation::GrafanaSearch)],
        &configuration,
        directory.path(),
        directory.path(),
    )
    .unwrap();
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    assert_eq!(
        value(&files, "scoped-inputs.json")["kubernetes"],
        Value::Null
    );
    write_files(directory.path(), &files);
    let observed = engarg::scoped_collectors::observe(
        request("N1", FINAL_DIGEST),
        &directory.path().join("scoped-inputs.json"),
    )
    .await
    .unwrap();
    assert_eq!(observed["value"]["collected"], true);
    assert_eq!(
        observed["value"]["observation"]["data"]["traces"],
        json!([])
    );
    assert_eq!(observed["context"]["engarg_export_digest"], FINAL_DIGEST);
    assert!(
        engarg::scoped_collectors::observe(
            request("previous_need", FINAL_DIGEST),
            &directory.path().join("scoped-inputs.json")
        )
        .await
        .is_err()
    );
    server.join().unwrap();
}

#[test]
fn explicit_retained_artifacts_are_verified_rebased_and_keep_original_provenance_and_age() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("operator");
    let output = directory.path().join("run");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&output).unwrap();
    let bytes = br#"{"present":false,"value":123456789012345678901234567890}"#;
    fs::write(source.join("measurement.json"), bytes).unwrap();
    let sha: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut configuration = inputs();
    configuration["artifacts"] = json!({"instrument_capture":{"artifacts":[{"path":"measurement.json","sha256":sha,"observed_at":"2025-01-01T12:00:00.100Z","source":"retained negative instrument measurement"}]}});
    let prepared = prepare_plans(
        &case,
        &[artifact("N1", Some("instrument_capture"))],
        &configuration,
        &source,
        &output,
    )
    .unwrap();
    assert!(prepared.plans[0].unresolved_arguments.is_empty());
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    write_files(&output, &files);
    let retained = value(&files, "artifact-inputs.json");
    assert_eq!(
        retained["needs"]["N1"]["artifacts"][0]["path"],
        source
            .join("measurement.json")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    let observed = engarg::retained_collectors::observe(
        "artifact_json",
        request("N1", FINAL_DIGEST),
        &output.join("artifact-inputs.json"),
    )
    .unwrap();
    assert_eq!(observed["observed_at"], "2025-01-01T12:00:00.100Z");
    assert_eq!(
        observed["value"]["artifacts"][0]["observation"]["present"],
        false
    );
    assert_eq!(
        observed["value"]["artifacts"][0]["source"],
        "retained negative instrument measurement"
    );
    assert!(observed["value"].get("accepted").is_none());
    let report = value(&files, "collector-preparation.json");
    assert_eq!(
        report_entry(&report, "N1")["evidence"],
        "retained_artifacts_verified"
    );
}

#[test]
fn missing_or_invalid_retained_records_keep_the_reader_unresolved() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("capture.json"), b"{}").unwrap();
    let sha: String = Sha256::digest(b"{}")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let valid = json!({"path":"capture.json","sha256":sha,"observed_at":"2025-01-01T00:00:00Z","source":"retained instrument capture"});
    let mut cases = vec![json!([])];
    for (field, value) in [
        ("path", json!("missing.json")),
        ("sha256", json!("0".repeat(64))),
        ("observed_at", json!("2025-02-29T00:00:00Z")),
        ("source", json!("")),
    ] {
        let mut pin = valid.clone();
        pin[field] = value;
        cases.push(json!([pin]));
    }
    for artifacts in cases {
        let configuration = json!({"artifacts":{"capture":{"artifacts":artifacts}}});
        let mut selected = artifact("N1", Some("capture"));
        selected.arguments.push(ToolArgument {
            name: "inputs".into(),
            value: "/old/inputs.json".into(),
        });
        selected.unresolved_arguments.clear();
        let prepared = prepare_plans(
            &case,
            &[selected],
            &configuration,
            directory.path(),
            directory.path(),
        )
        .unwrap();
        assert!(prepared.plans[0].arguments.is_empty());
        assert_eq!(prepared.plans[0].unresolved_arguments, ["inputs"]);
        let files = prepared.finish(FINAL_DIGEST).unwrap();
        assert_eq!(
            value(&files, "artifact-inputs.json")["needs"]["N1"]["artifacts"],
            json!([])
        );
        assert_eq!(
            report_entry(&value(&files, "collector-preparation.json"), "N1")["evidence"],
            "unavailable"
        );
    }
}

#[test]
fn resolved_old_paths_and_suggestive_prose_do_not_supply_current_operation_authority() {
    let (_, mut case) = fixture();
    case.evidence_plan[0].method = "Read full Grafana trace and execute exact search".into();
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.json");
    fs::write(&old, serde_json::to_vec(&inputs()).unwrap()).unwrap();
    let mut selected = plan("N1", "scoped_json", None);
    selected.arguments.push(ToolArgument {
        name: "inputs".into(),
        value: old.to_str().unwrap().into(),
    });
    selected.unresolved_arguments.clear();
    let prepared = prepare_plans(
        &case,
        &[selected],
        &inputs(),
        directory.path(),
        directory.path(),
    )
    .unwrap();
    assert!(prepared.plans[0].arguments.is_empty());
    assert_eq!(prepared.plans[0].unresolved_arguments, ["inputs"]);
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    assert_eq!(value(&files, "scoped-inputs.json")["needs"], json!({}));
}

#[test]
fn explicit_trace_ids_bind_only_with_matching_identity_and_forwardable_credentials() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let mut configuration = inputs();
    configuration["grafana"]["trace_ids"] = json!(["1234567890abcdef1234567890abcdef"]);
    let selected = scoped("N1", ScopedOperation::GrafanaTrace);
    let prepared = prepare_plans(
        &case,
        std::slice::from_ref(&selected),
        &configuration,
        directory.path(),
        directory.path(),
    )
    .unwrap();
    assert!(prepared.plans[0].unresolved_arguments.is_empty());
    for (field, value) in [
        ("trace_ids", json!(["0000000000000000"])),
        ("rollout_name", json!("different-resource")),
        ("token_env", json!("UNFORWARDED_TOKEN")),
    ] {
        let mut invalid = configuration.clone();
        invalid["grafana"][field] = value;
        let prepared = prepare_plans(
            &case,
            std::slice::from_ref(&selected),
            &invalid,
            directory.path(),
            directory.path(),
        )
        .unwrap();
        assert_eq!(
            prepared.plans[0].unresolved_arguments,
            ["inputs"],
            "{field}"
        );
    }
}

#[test]
fn malformed_unused_fields_cannot_be_reported_as_bound() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    for (operation, field, wrong_type) in [
        (
            ScopedOperation::GrafanaSearch,
            "trace_ids",
            json!("not-an-array"),
        ),
        (
            ScopedOperation::GrafanaTrace,
            "start_unix",
            json!("not-an-integer"),
        ),
    ] {
        let mut configuration = inputs();
        configuration["grafana"]["trace_ids"] = json!(["1234567890abcdef"]);
        configuration["grafana"][field] = wrong_type;
        let prepared = prepare_plans(
            &case,
            &[scoped("N1", operation)],
            &configuration,
            directory.path(),
            directory.path(),
        )
        .unwrap();
        assert_eq!(prepared.plans[0].unresolved_arguments, ["inputs"]);
        let files = prepared.finish(FINAL_DIGEST).unwrap();
        assert_eq!(value(&files, "scoped-inputs.json")["grafana"], Value::Null);
        assert!(
            report_entry(&value(&files, "collector-preparation.json"), "N1")["gaps"]
                .to_string()
                .contains("input types")
        );
    }
    let mut configuration = inputs();
    configuration["grafana"]["trace_ids"] = json!("not-an-array");
    let prepared = prepare_plans(
        &case,
        &[scoped("N1", ScopedOperation::Kubernetes)],
        &configuration,
        directory.path(),
        directory.path(),
    )
    .unwrap();
    assert!(prepared.plans[0].unresolved_arguments.is_empty());
    assert_eq!(
        value(
            &prepared.finish(FINAL_DIGEST).unwrap(),
            "scoped-inputs.json"
        )["grafana"],
        Value::Null
    );
}

#[test]
fn selected_native_descriptions_do_not_assert_installed_implementations() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let selected = AcquisitionPlan {
        need_id: "N1".into(),
        tool_id: "datadog_query".into(),
        arguments: Vec::new(),
        unresolved_arguments: vec!["query".into()],
        input_preparation: None,
    };
    let prepared = prepare_plans(
        &case,
        &[selected],
        &inputs(),
        directory.path(),
        directory.path(),
    )
    .unwrap();
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    assert_eq!(
        report_entry(&value(&files, "collector-preparation.json"), "N1")["implementation"],
        "catalogue_selected"
    );
}

#[test]
fn the_reported_one_seven_thirty_four_case_configures_scope_and_retains_producer_gaps() {
    let (_, mut case) = fixture();
    while case.evidence_plan.len() < 42 {
        let mut need = case.evidence_plan[0].clone();
        need.id = format!("N{}", case.evidence_plan.len() + 1);
        case.evidence_plan.push(need);
    }
    let directory = tempfile::tempdir().unwrap();
    let mut plans = Vec::new();
    for (index, need) in case.evidence_plan.iter().enumerate() {
        if index == 0 {
            plans.push(AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: "kubectl_get".into(),
                arguments: [
                    ("context", "dev"),
                    ("namespace", "development"),
                    ("resource", "applications.example.invalid"),
                    ("name", "catalog-sync-operator"),
                ]
                .into_iter()
                .map(|(name, value)| ToolArgument {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
                unresolved_arguments: Vec::new(),
                input_preparation: None,
            });
        } else if index < 8 {
            plans.push(scoped(&need.id, ScopedOperation::GrafanaSearch));
        } else {
            plans.push(artifact(&need.id, None));
        }
    }
    let prepared =
        prepare_plans(&case, &plans, &inputs(), directory.path(), directory.path()).unwrap();
    assert_eq!(
        prepared
            .plans
            .iter()
            .filter(|plan| plan.unresolved_arguments.is_empty())
            .count(),
        8
    );
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    let report = value(&files, "collector-preparation.json");
    assert_eq!(
        report["summary"],
        json!({"bound":8,"unresolved":34,"retained_artifacts_verified":0})
    );
    assert_eq!(
        value(&files, "scoped-inputs.json")["needs"]
            .as_object()
            .unwrap()
            .len(),
        7
    );
    let missing = report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["tool_id"] == "artifact_json")
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 34);
    assert!(
        missing
            .iter()
            .all(|entry| entry["implementation"] == "available"
                && entry["configuration"] == "unresolved"
                && entry["evidence"] == "unavailable"
                && entry["gaps"].to_string().contains("producer unidentified"))
    );
}

#[test]
fn direct_adapter_bindings_report_static_configuration_without_claiming_collection() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let selected = AcquisitionPlan {
        need_id: "N1".into(),
        tool_id: "kubectl_get".into(),
        arguments: [
            ("context", "dev"),
            ("namespace", "development"),
            ("resource", "applications.example.invalid"),
            ("name", "catalog-sync-operator"),
        ]
        .into_iter()
        .map(|(name, value)| ToolArgument {
            name: name.into(),
            value: value.into(),
        })
        .collect(),
        unresolved_arguments: Vec::new(),
        input_preparation: None,
    };
    let prepared = prepare_plans(
        &case,
        &[selected],
        &inputs(),
        directory.path(),
        directory.path(),
    )
    .unwrap();
    let files = prepared.finish(FINAL_DIGEST).unwrap();
    let report = value(&files, "collector-preparation.json");
    assert_eq!(report_entry(&report, "N1")["configuration"], "bound");
    assert_eq!(report_entry(&report, "N1")["evidence"], "not_collected");
    assert!(!files.contains_key("scoped-inputs.json"));
    assert!(!files.contains_key("artifact-inputs.json"));
    assert_eq!(report["runtime_access_and_credentials_checked"], false);
}

#[test]
fn invalid_case_digests_unknown_needs_and_duplicate_plans_fail_preparation() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    assert!(
        prepare_plans(
            &case,
            &[scoped("unknown", ScopedOperation::Kubernetes)],
            &inputs(),
            directory.path(),
            directory.path()
        )
        .is_err()
    );
    let selected = scoped("N1", ScopedOperation::Kubernetes);
    assert!(
        prepare_plans(
            &case,
            &[selected.clone(), selected],
            &inputs(),
            directory.path(),
            directory.path()
        )
        .is_err()
    );
    let prepared =
        prepare_plans(&case, &[], &inputs(), directory.path(), directory.path()).unwrap();
    assert!(prepared.finish("old-case-digest").is_err());
}

#[cfg(unix)]
#[test]
fn retained_symlinks_and_special_files_are_not_admitted() {
    let (_, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("real.json"), b"{}").unwrap();
    std::os::unix::fs::symlink(
        directory.path().join("real.json"),
        directory.path().join("alias.json"),
    )
    .unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(directory.path().join("capture.fifo"))
            .status()
            .unwrap()
            .success()
    );
    let sha: String = Sha256::digest(b"{}")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    for path in ["alias.json", "capture.fifo"] {
        let configuration = json!({"artifacts":{"capture":{"artifacts":[{"path":path,"sha256":sha,"observed_at":"2025-01-01T00:00:00Z","source":"operator pin"}]}}});
        let prepared = prepare_plans(
            &case,
            &[artifact("N1", Some("capture"))],
            &configuration,
            directory.path(),
            directory.path(),
        )
        .unwrap();
        assert_eq!(prepared.plans[0].unresolved_arguments, ["inputs"]);
    }
}
