//! Private preparation preserves the argument, chosen collectors and pending reviews.
use engarg::{
    acquisition::{AcquisitionPlan, ToolArgument},
    collector_catalogue::{CollectorCatalogue, ToolsRegistry},
    domain::{ArgumentCase, Request},
    eal_acquisition::plan_from_bundle,
    eal_export::{EalExport, ReviewRequest},
    eal_prepare::{prepare, prepare_with_catalogue},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command};

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn argument(name: &str, value: &str) -> ToolArgument {
    ToolArgument {
        name: name.into(),
        value: value.into(),
    }
}

fn prepared_plan(files: &BTreeMap<String, Vec<u8>>) -> Vec<AcquisitionPlan> {
    serde_json::from_slice(&files["acquisition-inputs.json"]).unwrap()
}

fn registry(files: &BTreeMap<String, Vec<u8>>) -> ToolsRegistry {
    toml::from_str(std::str::from_utf8(&files["argument.tools.toml"]).unwrap()).unwrap()
}

fn native_catalogue() -> CollectorCatalogue {
    // A selected producer contract, independent of EngArg's preparation logic.
    CollectorCatalogue::from_json(
        &serde_json::to_vec(&json!({
            "schema":"eal.collector-catalogue/1",
            "acquisition_contract":"eal.acquisition/1",
            "collectors":[{
                "id":"github","version":"1","purpose":"Read an exact commit identity",
                "parameters":["repository","commit"],
                "input_schema":{"type":"object","additionalProperties":false,
                    "properties":{"repository":{"type":"string"},"commit":{"type":"string"}},
                    "required":["repository","commit"]},
                "result_schema":{"type":"object","properties":{"found":{"type":"boolean"}}},
                "command":{"argv":["eal-collector","github","--repository","{repository}",
                    "--commit","{commit}","--executable","{operator.gh}",
                    "--supervisor","{operator.supervisor}"],
                    "inherit_env":["GH_TOKEN"],"timeout_seconds":65,
                    "max_output_bytes":4194304,"parallel_safe":false},
                "acquisition_predicate":"found == true","constraints":[]
            }]
        }))
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn preparation_preserves_distinct_http_queries_and_exact_review_requests() {
    let (request, mut case) = fixture();
    case.evidence_plan[0].method = "Read the full query result".into();
    case.evidence_plan[1].method = "Read the full database export query".into();
    let retained = vec![
        AcquisitionPlan {
            input_preparation: None,
            need_id: case.evidence_plan[0].id.clone(),
            tool_id: "http_json_get".into(),
            arguments: vec![argument(
                "url",
                "https://laboratory.example/first?query=full",
            )],
            unresolved_arguments: vec![],
        },
        AcquisitionPlan {
            input_preparation: None,
            need_id: case.evidence_plan[1].id.clone(),
            tool_id: "http_json_get".into(),
            arguments: vec![argument("url", "https://archive.example/second?query=full")],
            unresolved_arguments: vec![],
        },
    ];
    let directory = tempfile::tempdir().unwrap();
    let files = prepare_with_catalogue(
        &request,
        &case,
        &retained,
        directory.path(),
        Path::new("/operator/bin/engarg"),
        None,
    )
    .unwrap();
    assert_eq!(files.len(), 7);
    assert!(!files.contains_key("collector-inputs.json"));
    assert!(!files.contains_key("artifact-inputs.json"));
    let plans = prepared_plan(&files);
    assert_eq!(plans.len(), case.evidence_plan.len());
    for original in &retained {
        let prepared = plans
            .iter()
            .find(|plan| plan.need_id == original.need_id)
            .unwrap();
        assert_eq!(
            serde_json::to_value(prepared).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }
    assert!(plans.iter().skip(2).all(|plan| plan.tool_id == "custom"
        && plan.arguments.is_empty()
        && plan.unresolved_arguments == ["adapter"]));
    let bundle: EalExport = serde_json::from_slice(&files["argument.bundle.json"]).unwrap();
    assert_eq!(
        serde_json::to_value(&bundle.original_case).unwrap(),
        serde_json::to_value(&case).unwrap()
    );
    assert_eq!(
        std::str::from_utf8(&files["argument.eal"]).unwrap(),
        bundle.source
    );
    assert_eq!(
        plan_from_bundle(&bundle).unwrap().unwrap().len(),
        case.evidence_plan.len()
    );
    let bindings = registry(&files);
    let review = &bindings.tools["engarg_review_contract"];
    assert_eq!(
        review.argv,
        vec![
            "eal-collector",
            "review-json",
            "--inputs",
            "review-inputs.json"
        ]
    );
    assert_eq!(review.inherit_env, vec!["PATH"]);
    assert!(!review.parallel_safe);
    let collector_bindings: Vec<_> = bindings
        .tools
        .iter()
        .filter(|(name, _)| name.as_str() != "engarg_review_contract")
        .collect();
    assert!(
        collector_bindings
            .iter()
            .filter(|(_, binding)| binding.argv.iter().any(|arg| arg == "http_json_get"))
            .all(|(_, binding)| binding.argv[0] == "/operator/bin/engarg")
    );
    let pending: Value = serde_json::from_slice(&files["review-inputs.json"]).unwrap();
    assert_eq!(pending["schema"], "eal.review-inputs/2");
    assert_eq!(pending["reviews"], json!([]));
    let reviews: Vec<ReviewRequest> =
        serde_json::from_slice(&files["review-requests.json"]).unwrap();
    assert_eq!(reviews.len(), bundle.obligations.len());
    for (review, obligation) in reviews.iter().zip(&bundle.obligations) {
        assert_eq!(review.context, bundle.context);
        assert_eq!(review.evidence_id, obligation.eal_evidence_name);
        assert_eq!(review.input["case_digest"], bundle.revision);
        assert_eq!(
            review.input["obligation"],
            serde_json::to_value(obligation).unwrap()
        );
        assert!(serde_json::to_value(review).unwrap().get("value").is_none());
        let input_line = format!("  input {}", serde_json::to_string(&review.input).unwrap());
        assert!(bundle.source.lines().any(|line| line == input_line));
    }
}

#[test]
fn preparation_preserves_selected_native_contract_and_inputs() {
    let (request, case) = fixture();
    let catalogue = native_catalogue();
    let retained = vec![AcquisitionPlan {
        input_preparation: None,
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "github".into(),
        arguments: vec![
            argument("commit", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            argument("repository", "example/research"),
        ],
        unresolved_arguments: vec![],
    }];
    let directory = tempfile::tempdir().unwrap();
    let files = prepare_with_catalogue(
        &request,
        &case,
        &retained,
        directory.path(),
        Path::new("/operator/bin/engarg"),
        Some(&catalogue),
    )
    .unwrap();
    let plans = prepared_plan(&files);
    assert_eq!(
        serde_json::to_value(&plans[0]).unwrap(),
        serde_json::to_value(&retained[0]).unwrap()
    );
    let bindings = registry(&files);
    let native = bindings
        .tools
        .values()
        .find(|binding| {
            binding
                .argv
                .first()
                .is_some_and(|arg| arg == "eal-collector")
        })
        .unwrap();
    assert!(native.argv.iter().any(|value| value == "example/research"));
    assert!(
        native
            .argv
            .iter()
            .any(|value| value == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(native.argv.iter().any(|value| value == "/REPLACE_GH"));
    assert_eq!(native.inherit_env, ["GH_TOKEN"]);
    assert!(!files.contains_key("collector-inputs.json"));
}

#[test]
fn preparation_keeps_operator_artifact_paths_and_rejects_ambiguous_bindings() {
    let (request, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("operator-artifacts.json");
    std::fs::write(&artifact, b"operator-owned original").unwrap();
    let plan = AcquisitionPlan {
        input_preparation: None,
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "artifact_json".into(),
        arguments: vec![argument("inputs", artifact.to_str().unwrap())],
        unresolved_arguments: vec![],
    };
    let files = prepare_with_catalogue(
        &request,
        &case,
        std::slice::from_ref(&plan),
        directory.path(),
        Path::new("/operator/bin/engarg"),
        None,
    )
    .unwrap();
    assert_eq!(
        prepared_plan(&files)[0].arguments[0].value,
        artifact.to_str().unwrap()
    );
    assert_eq!(
        std::fs::read(&artifact).unwrap(),
        b"operator-owned original"
    );
    assert!(!files.contains_key("artifact-inputs.json"));
    assert!(
        prepare_with_catalogue(
            &request,
            &case,
            &[plan.clone(), plan.clone()],
            directory.path(),
            Path::new("/operator/bin/engarg"),
            None
        )
        .is_err()
    );
    let mut unknown = plan;
    unknown.need_id = "unrepresented".into();
    assert!(
        prepare_with_catalogue(
            &request,
            &case,
            &[unknown],
            directory.path(),
            Path::new("/operator/bin/engarg"),
            None
        )
        .is_err()
    );
}

#[test]
fn empty_plans_stay_explicitly_unresolved_and_legacy_global_settings_are_rejected() {
    let (request, case) = fixture();
    let directory = tempfile::tempdir().unwrap();
    let files = prepare(
        &request,
        &case,
        &[],
        directory.path(),
        Path::new("/operator/bin/engarg"),
        None,
        None,
    )
    .unwrap();
    assert!(
        prepared_plan(&files)
            .iter()
            .all(|plan| plan.tool_id == "custom"
                && plan.arguments.is_empty()
                && plan.unresolved_arguments == ["adapter"])
    );
    let error = prepare(
        &request,
        &case,
        &[],
        directory.path(),
        Path::new("/operator/bin/engarg"),
        Some("dev"),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("explicit per-need"));
    assert!(
        prepare(
            &request,
            &case,
            &[],
            directory.path(),
            Path::new("/operator/bin/engarg"),
            None,
            Some("https://grafana.example")
        )
        .is_err()
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
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 7);
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
