//! End-to-end export of authored synthetic retained observations.
use eal_core::{ExecutionLimits, canonical, composition, evaluation, methods};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

const NOW: &str = "2026-10-29T12:00:00Z";
const SOURCE: &str = r#"language "EAL/4"
environment bench {
 require site == "synthetic"
}
tool reader {
 version "1"
}
evidence observed {
 tool reader
 kind test
 environment bench
 max_age 60
 require passed == true
}
reasoning measured {
 method "structured/1"
 rationale "This synthetic observation supports the declared test result."
}
claim working {
 statement "The synthetic bench test passed."
 environment bench
}
argument support = [evidence observed] via measured => working
"#;

fn execution(source: &str, positive: bool) -> Value {
    let program = composition::compile(source, None, "", ExecutionLimits::default()).unwrap();
    let context = json!({"site":"synthetic"});
    let mut records = json!({});
    for (name, evidence) in &program.evidence {
        let tool = &program.tools[evidence.tool.as_ref().unwrap()];
        let acquisition = json!({"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
        let request = json!({"evidence_id":name,"environment":evidence.environment,"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
        let value = json!({"passed":positive});
        records[name] = json!({
            "schema":"EAL/observation-record/1", "evidence_id":name,
            "source_digest":program.source_digest, "tool":tool.name,
            "tool_version":tool.version, "tool_binding_digest":"0".repeat(64),
            "evidence_kind":evidence.kind,"environment":evidence.environment,
            "environment_fingerprint":evaluation::environment_fingerprint(&evidence.environment,&context,&program.limits).unwrap(),
            "input_digest":canonical::canonical_digest(&evidence.input,&program.limits).unwrap(),
            "input":evidence.input,"context":context,"acquisition_request":acquisition,
            "acquisition_request_digest":canonical::canonical_digest(&acquisition,&program.limits).unwrap(),
            "request_digest":canonical::canonical_digest(&request,&program.limits).unwrap(),
            "collected_at":NOW,"run_id":format!("synthetic-{name}"),"status":"ok",
            "data_digest":canonical::canonical_digest(&value,&program.limits).unwrap(),"value":value
        });
    }
    let mut assessment = evaluation::evaluate(
        &program,
        &records,
        NOW,
        &context,
        methods::default_registry(),
        None,
    )
    .unwrap();
    assert_eq!(assessment["valid"], true);
    assert_eq!(assessment["complete"], true);
    assessment["collection_id"] = json!("synthetic-collection");
    assessment["assessment_id"] = json!("synthetic-assessment");
    json!({"is_error":false,"result":{
        "schema":"EAL/execution/1", "source_digest":program.source_digest,
        "claim":"working", "assessment":assessment,
        "collection":{"source_digest":program.source_digest,"context":context,
            "records":records,"collection_id":"synthetic-collection"}
    }})
}

fn run(root: &Path, source: &str, retained: &Value, extra: &[&str]) -> std::process::Output {
    std::fs::write(root.join("case.eal"), source).unwrap();
    std::fs::write(
        root.join("execution.json"),
        serde_json::to_vec(retained).unwrap(),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(root)
        .env_remove("ENGARG_MODEL")
        .args([
            "eal",
            "export-aspic",
            "--input",
            "case.eal",
            "--execution",
            "execution.json",
            "--output",
            "view.json",
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
#[ignore = "requires ENGARG_EAL_CORE selecting the paired native core"]
fn native_export_retains_observation_time_and_complete_negative_results() {
    for positive in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let retained = execution(SOURCE, positive);
        let result = run(root.path(), SOURCE, &retained, &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        let view: Value =
            serde_json::from_slice(&std::fs::read(root.path().join("view.json")).unwrap()).unwrap();
        assert_eq!(view["schema"], "aspic-view/3");
        assert_eq!(view["evaluated_at"], NOW);
        assert_eq!(view["goal_claim"], "working");
        if positive {
            assert_eq!(view["formal_status"], "accepted");
        }
        assert_eq!(
            view["authored_claim_status"],
            if positive { "supported" } else { "unsupported" }
        );
        assert_eq!(view["validation"]["formal_result"], "recomputed");
        if positive {
            assert!(
                view["arguments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["origin"]["observation"]["collected_at"] == NOW)
            );
        } else {
            assert_eq!(
                view["unavailable_evidence"][0]["availability_issues"],
                json!(["predicate_not_met"])
            );
        }
    }
}

#[test]
#[ignore = "requires ENGARG_EAL_CORE selecting the paired native core"]
fn changed_snapshot_inputs_and_assessment_only_json_leave_no_output() {
    let good = execution(SOURCE, true);
    let mut cases = Vec::new();
    let mut wrong = good.clone();
    wrong["result"]["collection"]["context"]["site"] = json!("other");
    cases.push((SOURCE.to_owned(), wrong));
    let mut wrong = good.clone();
    wrong["result"]["collection"]["records"]["observed"]["tool_binding_digest"] =
        json!("1".repeat(64));
    cases.push((SOURCE.to_owned(), wrong));
    let mut wrong = good.clone();
    wrong["result"]["collection"]["records"]["observed"]["value"]["passed"] = json!(false);
    cases.push((SOURCE.to_owned(), wrong));
    let mut wrong = good.clone();
    wrong["result"]["assessment"]["claims"]["working"]["status"] = json!("unsupported");
    cases.push((SOURCE.to_owned(), wrong));
    cases.push((
        SOURCE.replace("bench test passed", "bench test differs"),
        good.clone(),
    ));
    cases.push((SOURCE.to_owned(), good["result"]["assessment"].clone()));
    for (source, retained) in cases {
        let root = tempfile::tempdir().unwrap();
        let result = run(root.path(), &source, &retained, &[]);
        assert!(!result.status.success());
        assert!(!root.path().join("view.json").exists());
        assert!(result.stdout.is_empty());
    }
}

#[test]
#[ignore = "requires ENGARG_EAL_CORE selecting the paired native core"]
fn incomplete_formal_construction_requires_explicit_limits_and_preserves_existing_files() {
    let declarations = (0..9).map(|n| format!("evidence e{n} {{\n tool reader\n kind test\n environment bench\n max_age 60\n require passed == true\n}}\n")).collect::<String>();
    let source = SOURCE.replace(
        "argument support = [evidence observed]",
        &format!(
            "{declarations}\nargument support = [evidence {}]",
            (0..9)
                .map(|n| format!("e{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    let retained = execution(&source, true);
    let root = tempfile::tempdir().unwrap();
    let failed = run(root.path(), &source, &retained, &[]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("incomplete"));
    assert!(!root.path().join("view.json").exists());
    std::fs::write(
        root.path().join("limits.toml"),
        "[limits]\nformal_antecedents = 16\n",
    )
    .unwrap();
    let result = run(
        root.path(),
        &source,
        &retained,
        &["--limits", "limits.toml"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original = std::fs::read(root.path().join("view.json")).unwrap();
    assert!(
        !run(
            root.path(),
            &source,
            &retained,
            &["--limits", "limits.toml"]
        )
        .status
        .success()
    );
    assert_eq!(
        std::fs::read(root.path().join("view.json")).unwrap(),
        original
    );
}

#[test]
#[ignore = "requires ENGARG_EAL_CORE selecting the paired native core"]
fn method_profile_and_goal_are_checked_against_retained_assessment() {
    let retained = execution(SOURCE, true);
    for extra in [["--methods", "engineering"], ["--claim", "absent"]] {
        let root = tempfile::tempdir().unwrap();
        assert!(!run(root.path(), SOURCE, &retained, &extra).status.success());
        assert!(!root.path().join("view.json").exists());
    }
    let mut retained = retained;
    retained["result"]["claim"] = Value::Null;
    let root = tempfile::tempdir().unwrap();
    assert!(!run(root.path(), SOURCE, &retained, &[]).status.success());
    let result = run(root.path(), SOURCE, &retained, &["--claim", "working"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn assessment_only_and_failed_envelopes_are_rejected_without_a_native_process() {
    let good = execution(SOURCE, true);
    for retained in [
        good["result"]["assessment"].clone(),
        json!({"is_error":true,"result":good["result"]}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let result = run(
            root.path(),
            SOURCE,
            &retained,
            &["--eal-core", "/must-not-execute"],
        );
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("execution envelope"));
        assert!(!root.path().join("view.json").exists());
    }
}

#[cfg(unix)]
#[test]
fn failed_or_non_json_native_processes_leave_no_view() {
    for executable in ["/bin/false", "/bin/echo"] {
        let root = tempfile::tempdir().unwrap();
        let result = run(
            root.path(),
            SOURCE,
            &execution(SOURCE, true),
            &["--eal-core", executable],
        );
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!root.path().join("view.json").exists());
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires ENGARG_EAL_CORE selecting the paired native core"]
fn relative_operator_executable_is_resolved_before_the_private_workspace() {
    use std::os::unix::fs::symlink;
    let core =
        std::env::var_os("ENGARG_EAL_CORE").expect("set ENGARG_EAL_CORE for installed tests");
    let core = std::fs::canonicalize(core).unwrap();
    let root = tempfile::tempdir().unwrap();
    symlink(core, root.path().join("native core")).unwrap();
    let result = run(
        root.path(),
        SOURCE,
        &execution(SOURCE, true),
        &["--eal-core", "./native core"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
