use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
}

#[test]
fn demo_is_explicitly_offline_and_structurally_valid() {
    let result = cli().args(["demo", "--format", "json"]).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["execution_mode"], "offline_synthetic_fixture");
    let request = serde_json::from_value(value["request"].clone()).unwrap();
    let case = serde_json::from_value(value["argument"].clone()).unwrap();
    assert!(engarg::domain::validate_case(&request, &case).is_ok());
}

#[test]
fn outputs_are_private_and_existing_files_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("case.json");
    assert!(
        cli()
            .args(["demo", "--format", "json", "--output"])
            .arg(&output)
            .status()
            .unwrap()
            .success()
    );
    let before = std::fs::read(&output).unwrap();
    let second = cli()
        .args(["demo", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(std::fs::read(&output).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn rejects_ambiguous_input_and_has_no_implicit_model_selection() {
    let result = cli()
        .args([
            "analyse", "--claim", "C", "--input", "x.json", "--model", "m",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let result = cli()
        .env_remove("ENGARG_MODEL")
        .args(["analyse", "--claim", "C"])
        .output()
        .unwrap();
    assert!(!result.status.success());
}

#[test]
fn analysis_exposes_direct_and_discovery_planning_without_flat_frontend() {
    let help = cli().args(["analyse", "--help"]).output().unwrap();
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("[possible values: direct, graph]"));
    assert!(!text.contains("--collector-catalogue"));
    for options in [
        vec!["--evidence-planning", "flat"],
        vec!["--collector-catalogue", "catalogue.json"],
    ] {
        let output = cli()
            .args(["analyse", "--claim", "C", "--model", "fixture"])
            .args(options)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn conflicting_output_and_audit_paths_fail_before_transport_setup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.json");
    let result = cli()
        .args(["analyse", "--claim", "C", "--model", "M", "--output"])
        .arg(&path)
        .arg("--audit")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("different paths"));
    assert!(!path.exists());
}

#[test]
fn exported_schema_closes_every_object_and_has_required_fields() {
    let result = cli().args(["schema", "case"]).output().unwrap();
    assert!(result.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    fn check(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                if object.get("type").and_then(|v| v.as_str()) == Some("object") {
                    assert_eq!(
                        object.get("additionalProperties"),
                        Some(&serde_json::json!(false))
                    );
                    let props = object["properties"].as_object().unwrap();
                    let required = object["required"].as_array().unwrap();
                    assert_eq!(props.len(), required.len());
                }
                for v in object.values() {
                    check(v);
                }
            }
            serde_json::Value::Array(values) => {
                for v in values {
                    check(v)
                }
            }
            _ => {}
        }
    }
    check(&schema);
    assert!(schema["properties"].get("subclaims").is_some());
    assert!(schema["properties"].get("compositions").is_some());
    assert!(schema["properties"].get("reading").is_some());
}

#[test]
fn default_output_uses_the_attached_template_and_original_is_available() {
    let result = cli().arg("demo").output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("Argument context"));
    assert!(text.contains("14. Decision basis and revision triggers"));
    assert!(text.contains("Evidence register"));
    assert!(text.contains("Synthetic demonstration"));
    let original = cli().arg("template").output().unwrap();
    assert!(original.status.success());
    assert_eq!(
        original.stdout,
        include_bytes!("../templates/engineering-toulmin.md")
    );
}

#[test]
fn one_saved_graph_can_render_every_presentation_without_a_provider_key() {
    let directory = tempfile::tempdir().unwrap();
    let canonical = directory.path().join("run.json");
    let result = cli()
        .args(["demo", "--format", "json", "--output"])
        .arg(&canonical)
        .output()
        .unwrap();
    assert!(result.status.success());
    let before = std::fs::read(&canonical).unwrap();
    for (format, marker) in [
        ("markdown", "Argument context"),
        ("md", "Argument context"),
        ("outline", "current\\_claim"),
        ("argument", "current_claim"),
        ("plain", "current_claim"),
        ("txt", "current_claim"),
        ("prose", "controlled parallelisation"),
        ("html", "<details"),
        ("expandable", "<details"),
    ] {
        let result = cli()
            .env_remove("ENGARG_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .args(["render", "--input"])
            .arg(&canonical)
            .args(["--format", format])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains(marker), "{format} lacks {marker}");
        assert!(text.contains("Synthetic demonstration"));
    }
    assert_eq!(std::fs::read(canonical).unwrap(), before);
}

#[test]
fn omitting_the_audit_appendix_preserves_every_argument_record_and_saved_json() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    assert!(
        cli()
            .args(["demo", "--format", "json", "--output"])
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    let before = std::fs::read(&path).unwrap();
    let full = cli()
        .args(["render", "--input"])
        .arg(&path)
        .output()
        .unwrap();
    let compact = cli()
        .args(["render", "--input"])
        .arg(&path)
        .arg("--no-audit-appendix")
        .output()
        .unwrap();
    assert!(full.status.success() && compact.status.success());
    let full = String::from_utf8(full.stdout).unwrap();
    let compact = String::from_utf8(compact.stdout).unwrap();
    assert_eq!(
        full.split_once("## Complete audit record")
            .unwrap()
            .0
            .trim_end(),
        compact.trim_end()
    );
    for marker in [
        "14. Decision basis and revision triggers",
        "Evidence register",
        "Challenge register",
        "Composition register",
    ] {
        assert!(compact.contains(marker));
    }
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn invalid_saved_graph_is_rejected_before_it_can_be_presented() {
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("bad.json");
    let mut value: serde_json::Value = serde_json::from_slice(
        &cli()
            .args(["demo", "--format", "json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    value["argument"]["reading"][0]["paragraphs"][0]["reference_ids"] =
        serde_json::json!(["invented-reference"]);
    std::fs::write(&report, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = cli()
        .args(["render", "--input"])
        .arg(report)
        .args(["--format", "prose"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invented-reference"));
}

#[test]
fn outline_depth_is_bounded_at_the_cli() {
    for depth in ["0", "7"] {
        assert!(
            !cli()
                .args(["demo", "--format", "outline", "--outline-depth", depth])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn generation_and_display_depth_are_distinct_cli_controls() {
    for option in ["--argument-depth", "--display-depth"] {
        for invalid in ["0", "17"] {
            let result = cli()
                .args(["analyse", "--claim", "C", "--model", "M", option, invalid])
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("invalid value"));
        }
    }
    for invalid in ["4", "-1"] {
        assert!(
            !cli()
                .args([
                    "analyse",
                    "--claim",
                    "C",
                    "--model",
                    "M",
                    "--max-repairs",
                    invalid
                ])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("run.json");
    assert!(
        cli()
            .args(["demo", "--format", "json", "--output"])
            .arg(&path)
            .output()
            .unwrap()
            .status
            .success()
    );
    let original = std::fs::read(&path).unwrap();
    for format in ["outline", "prose", "html"] {
        let result = cli()
            .args(["render", "--input"])
            .arg(&path)
            .args(["--format", format, "--display-depth", "1"])
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("Detailed claim levels: 1"));
    }
    let json = cli()
        .args(["render", "--input"])
        .arg(&path)
        .args(["--format", "json", "--display-depth", "1"])
        .output()
        .unwrap();
    assert!(json.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json.stdout).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&original).unwrap()
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn minimal_request_schema_requires_only_the_claim() {
    let output = cli().args(["schema", "request"]).output().unwrap();
    assert!(output.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(schema["required"], serde_json::json!(["claim"]));
    assert_eq!(schema["properties"]["context"]["default"], "");
    assert_eq!(
        schema["properties"]["evidence"]["default"],
        serde_json::json!([])
    );
}

#[test]
fn claim_only_demo_contains_a_conditional_argument_and_required_evidence() {
    let output = cli()
        .args(["demo", "--claim-only", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["execution_mode"], "offline_synthetic_fixture");
    assert_eq!(report["request"]["evidence"], serde_json::json!([]));
    assert_eq!(report["argument"]["status"], "conditional");
    assert!(
        !report["argument"]["warrants"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !report["argument"]["evidence_plan"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let text = cli().args(["demo", "--claim-only"]).output().unwrap();
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("14. Decision basis and revision triggers"));
    assert!(text.contains("Evidence obligations for the stronger target"));
}
