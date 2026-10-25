//! Exercise an unrelated scientific collector through generated source and the real EAL runtime.
#![cfg(unix)]
use engarg::{
    acquisition::{AcquisitionPlan, ToolArgument},
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request},
    eal_acquisition::export_with_catalogue,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
fn scientific_collector_preserves_typed_inputs_and_measured_negative() {
    let binary =
        std::env::var_os("ENGARG_EAL_HOST").expect("compatibility test requires ENGARG_EAL_HOST");
    let directory = tempfile::tempdir().unwrap();
    let collector = directory.path().join("calibration-collector");
    std::fs::write(&collector, "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"value\":{\"collected\":true,\"calibrated\":false,\"samples\":5,\"channels\":[1,3]}}'\n").unwrap();
    std::fs::set_permissions(&collector, std::fs::Permissions::from_mode(0o700)).unwrap();
    let catalogue = CollectorCatalogue::from_json(&serde_json::to_vec(&json!({
        "schema":"eal.collector-catalogue/1", "acquisition_contract":"eal.acquisition/1",
        "collectors":[{"id":"lab.calibration","version":"7","purpose":"Measure calibration for selected laboratory channels",
            "parameters":["samples","channels"],
            "input_schema":{"type":"object","additionalProperties":false,"required":["samples","channels"],
                "properties":{"samples":{"type":"integer","minimum":1},"channels":{"type":"array","items":{"type":"integer"}}}},
            "result_schema":{"type":"object","required":["collected","calibrated"],"properties":{"collected":{"type":"boolean"},"calibrated":{"type":"boolean"}}},
            "command":{"argv":[collector],"inherit_env":[],"timeout_seconds":5,"max_output_bytes":4096,"parallel_safe":false},
            "acquisition_predicate":"collected == true", "constraints":[]
        }]
    })).unwrap()).unwrap();
    let request: Request = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let plans = case
        .evidence_plan
        .iter()
        .enumerate()
        .map(|(index, need)| {
            if index == 0 {
                AcquisitionPlan {
                    input_preparation: None,
                    need_id: need.id.clone(),
                    tool_id: "lab.calibration".into(),
                    arguments: vec![
                        ToolArgument {
                            name: "samples".into(),
                            value: "5".into(),
                        },
                        ToolArgument {
                            name: "channels".into(),
                            value: "[1,3]".into(),
                        },
                    ],
                    unresolved_arguments: vec![],
                }
            } else {
                AcquisitionPlan {
                    input_preparation: None,
                    need_id: need.id.clone(),
                    tool_id: "custom".into(),
                    arguments: vec![],
                    unresolved_arguments: vec!["adapter".into()],
                }
            }
        })
        .collect::<Vec<_>>();
    let exported = export_with_catalogue(&request, &case, &plans, Some(&catalogue)).unwrap();
    let tool = exported.bundle.source_map[&case.evidence_plan[0].id]
        .declarations
        .iter()
        .find(|entry| entry.kind == "tool")
        .unwrap()
        .name
        .clone();
    let registry = eal_collector_contracts::CommandRegistry {
        tools: std::collections::BTreeMap::from([(
            tool.clone(),
            exported.registry.tools[&tool].clone(),
        )]),
    };
    std::fs::write(
        directory.path().join("tools.toml"),
        registry.to_toml().unwrap(),
    )
    .unwrap();
    let bundle = exported.bundle;
    let evidence = &bundle.source_map[&case.evidence_plan[0].id]
        .declarations
        .iter()
        .find(|d| d.kind == "evidence")
        .unwrap()
        .name;
    for operation in [
        json!({"operation":"validate","source":bundle.source}),
        json!({"operation":"collect","source":bundle.source,"context":bundle.context,"evidence_ids":[evidence]}),
    ] {
        let mut child = Command::new(&binary)
            .arg("--workspace")
            .arg(directory.path())
            .arg("--registry")
            .arg(directory.path().join("tools.toml"))
            .args(["--timeout", "15"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&operation).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{response}");
        assert_ne!(response["is_error"], true, "{response}");
        if operation["operation"] == "validate" {
            assert_eq!(response["result"]["valid"], true);
        } else {
            let record = &response["result"]["records"][evidence];
            assert_eq!(record["status"], "ok", "{record}");
            assert_eq!(record["value"]["collected"], true);
            assert_eq!(record["value"]["calibrated"], false);
            assert_eq!(record["value"]["channels"], json!([1, 3]));
            assert_eq!(record["value"]["samples"], 5);
        }
    }
}

#[test]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
fn openapi_descriptor_exports_and_collects_a_measured_negative_through_eal() {
    use sha2::{Digest, Sha256};
    use std::{io::Read, net::TcpListener, thread};
    let host =
        std::env::var_os("ENGARG_EAL_HOST").expect("compatibility test requires ENGARG_EAL_HOST");
    let collector =
        std::env::var_os("EAL_COLLECTOR").expect("compatibility test requires EAL_COLLECTOR");
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..n]);
        }
        assert!(String::from_utf8_lossy(&request).starts_with("GET /health "));
        let body = r#"{"healthy":false}"#;
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let pin = |path: &std::path::Path, value: Value| {
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(path, &bytes).unwrap();
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let spec = directory.path().join("openapi.json");
    let digest = pin(
        &spec,
        json!({"openapi":"3.1.0","paths":{"/health":{"get":{"operationId":"health","responses":{"200":{"content":{"application/json":{"schema":{"type":"object","required":["healthy"],"properties":{"healthy":{"type":"boolean"}}}}}}}}}}}),
    );
    let config = directory.path().join("source.json");
    let digest = pin(
        &config,
        json!({"schema":"eal.api-source/1","origin":origin,"source":{"kind":"openapi","specification":spec,"specification_sha256":digest,"operation_id":"health","read_only":true}}),
    );
    let described = Command::new(&collector)
        .args([
            "describe-api",
            "openapi",
            "--contract-version",
            "2",
            "--measurement",
            "openapi_operation",
            "--config",
            config.to_str().unwrap(),
            "--config-sha256",
            &digest,
        ])
        .env_clear()
        .output()
        .unwrap();
    assert!(
        described.status.success(),
        "{}",
        String::from_utf8_lossy(&described.stderr)
    );
    let catalogue = CollectorCatalogue::from_json(&described.stdout).unwrap();
    let descriptor = &catalogue.collectors[0];
    let request: Request = serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let plans = case
        .evidence_plan
        .iter()
        .enumerate()
        .map(|(index, need)| AcquisitionPlan {
            input_preparation: None,
            need_id: need.id.clone(),
            tool_id: if index == 0 {
                descriptor.id.clone()
            } else {
                "custom".into()
            },
            arguments: vec![],
            unresolved_arguments: if index == 0 {
                vec![]
            } else {
                vec!["adapter".into()]
            },
        })
        .collect::<Vec<_>>();
    let exported = export_with_catalogue(&request, &case, &plans, Some(&catalogue)).unwrap();
    let entries = &exported.bundle.source_map[&case.evidence_plan[0].id].declarations;
    let tool = &entries
        .iter()
        .find(|entry| entry.kind == "tool")
        .unwrap()
        .name;
    let evidence = &entries
        .iter()
        .find(|entry| entry.kind == "evidence")
        .unwrap()
        .name;
    let binding = descriptor
        .binding(
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::from([(
                "executable".into(),
                collector.to_str().unwrap().into(),
            )]),
        )
        .unwrap();
    let registry = eal_collector_contracts::CommandRegistry {
        tools: std::collections::BTreeMap::from([(tool.clone(), binding)]),
    };
    let registry_path = directory.path().join("tools.toml");
    std::fs::write(&registry_path, registry.to_toml().unwrap()).unwrap();
    for operation in [
        json!({"operation":"validate","source":exported.bundle.source}),
        json!({"operation":"collect","source":exported.bundle.source,"context":exported.bundle.context,"evidence_ids":[evidence]}),
    ] {
        let mut child = Command::new(&host)
            .arg("--workspace")
            .arg(directory.path())
            .arg("--registry")
            .arg(&registry_path)
            .args(["--timeout", "30"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&operation).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(output.status.success(), "{response}");
        assert_ne!(response["is_error"], true, "{response}");
        if operation["operation"] == "validate" {
            assert_eq!(response["result"]["valid"], true);
        } else {
            let record = &response["result"]["records"][evidence];
            assert_eq!(record["status"], "ok", "{record}");
            assert_eq!(record["value"]["collected"], true);
            assert_eq!(record["value"]["observation"]["healthy"], false);
        }
    }
    server.join().unwrap();
}
