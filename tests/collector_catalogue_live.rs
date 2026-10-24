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
