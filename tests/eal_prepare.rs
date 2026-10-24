//! Private preparation preserves the argument and every independent review gate.
use engarg::{
    acquisition::{AcquisitionPlan, ToolArgument},
    domain::{ArgumentCase, Request},
    eal_acquisition::plan_from_bundle,
    eal_export::EalExport,
    eal_prepare::prepare,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

#[test]
fn preparation_binds_complete_commands_and_preserves_pending_reviews() {
    let (request, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let mut retained = vec![AcquisitionPlan {
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "kubernetes".into(),
        arguments: vec![
            ToolArgument {
                name: "kind".into(),
                value: "Widget".into(),
            },
            ToolArgument {
                name: "name".into(),
                value: "example".into(),
            },
        ],
        unresolved_arguments: vec!["inventory".into(), "resource".into(), "namespace".into()],
    }];
    retained.extend(
        case.evidence_plan
            .iter()
            .skip(1)
            .map(|need| AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: "custom".into(),
                arguments: vec![],
                unresolved_arguments: vec!["adapter".into()],
            }),
    );
    let files = prepare(
        &request,
        &case,
        &retained,
        directory.path(),
        Path::new("/operator/bin/engarg"),
        Some("dev"),
        Some("https://grafana.example.invalid"),
    )
    .unwrap();
    assert_eq!(files.len(), 9);
    let bundle: EalExport = serde_json::from_slice(&files["argument.bundle.json"]).unwrap();
    assert_eq!(
        serde_json::to_value(&bundle.original_case).unwrap(),
        serde_json::to_value(&case).unwrap()
    );
    let plans = plan_from_bundle(&bundle).unwrap().unwrap();
    assert_eq!(plans.len(), case.evidence_plan.len());
    assert!(engarg::acquisition::validate(&case, &plans).is_empty());
    assert!(
        plans
            .iter()
            .all(|plan| plan.unresolved_arguments.is_empty())
    );
    assert_eq!(plans[0].tool_id, "scoped_json");
    let registry = std::str::from_utf8(&files["argument.tools.toml"]).unwrap();
    assert!(!registry.contains("REPLACE"));
    assert!(registry.contains("/operator/bin/engarg"));
    assert!(registry.contains("review_json"));
    assert!(registry.contains("inherit_env = []"));
    let scoped: Value = serde_json::from_slice(&files["collector-inputs.json"]).unwrap();
    assert_eq!(scoped["case_digest"], bundle.revision);
    assert_eq!(scoped["kubernetes"]["context"], "dev");
    assert!(scoped["kubernetes"]["resource"].is_null());
    assert!(scoped["kubernetes"]["namespace"].is_null());
    assert!(scoped["grafana"]["datasource_uid"].is_null());
    assert!(scoped["grafana"]["token_env"].is_null());
    assert!(scoped["grafana"]["tenant_env"].is_null());
    let reviews: Value = serde_json::from_slice(&files["review-inputs.json"]).unwrap();
    assert_eq!(reviews["reviews"], json!([]));
    let requests: Vec<Value> = serde_json::from_slice(&files["review-requests.json"]).unwrap();
    assert_eq!(requests.len(), bundle.obligations.len());
    for request in requests {
        assert_eq!(request["context"], bundle.context);
        assert_eq!(request["input"]["case_digest"], bundle.revision);
        assert!(request["input"]["obligation"].is_object());
        assert!(request.get("value").is_none());
    }
    assert_eq!(
        std::str::from_utf8(&files["argument.eal"]).unwrap(),
        bundle.source
    );
}

#[test]
fn prepare_cli_creates_private_files_and_preserves_existing_directory() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("analysis.json");
    let output = directory.path().join("private");
    let (request, argument) = fixture();
    std::fs::write(
        &input,
        serde_json::to_vec(&json!({"request":request,"argument":argument})).unwrap(),
    )
    .unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["eal", "prepare", "--input"])
            .arg(&input)
            .arg("--output-dir")
            .arg(&output)
            .args(["--kube-context", "dev"])
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    let original = std::fs::read(output.join("argument.bundle.json")).unwrap();
    let result = run();
    assert!(!result.status.success());
    assert_eq!(
        std::fs::read(output.join("argument.bundle.json")).unwrap(),
        original
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for entry in std::fs::read_dir(&output).unwrap() {
            assert_eq!(
                entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
