//! Public optimisation retains independently executed candidate sources and evidence.
use eal_collector_contracts::execution::EvaluationCriterion;
use engarg::{
    direct_planning::DirectPlan,
    domain::{Ground, SupportRelationship},
    engine::{Mode, RunReport},
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn fixture(root: &Path, third: bool) -> RunReport {
    let (request, mut argument, mut support) = executable_case::fixture();
    // Keep real checked deduction and separate empirical/applicability gates.
    // The live source has a wider explicit test interval than the frozen
    // historical fixture; collected observations keep their actual timestamps.
    if let Some(engarg::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut argument.warrants[0].reasoning_method
    {
        bound.conclusion.valid_from = "2026-01-01T00:00:00Z".into();
        bound.conclusion.valid_until = "2100-01-01T00:00:00Z".into();
    }
    argument.compositions[0].relationship = SupportRelationship::AlternativeRoutes;
    let mut second = argument.warrants[0].clone();
    second.id = "W2".into();
    second.premise_ids.push("G2".into());
    if let Some(engarg::domain::ReasoningMethodBinding::Bound(bound)) = &mut second.reasoning_method
    {
        bound.method.query["premises"]
            .as_array_mut()
            .unwrap()
            .push(json!("r"));
        let mut premise = bound.formal_premises[0].clone();
        premise.premise_id = "G2".into();
        premise.input_pointer = "/premises/2".into();
        premise.evidence_need_id = "N8".into();
        bound.formal_premises.push(premise);
        let mut requirement = bound.evidence_requirements[0].clone();
        requirement.evidence_need_id = "N8".into();
        bound.evidence_requirements.push(requirement);
    }
    argument.assumptions[0].applies_to.push("W2".into());
    argument.warrants.push(second);
    argument.grounds.push(Ground {
        id: "G2".into(),
        statement:
            "A second authored use of the same identified snapshot; not an independent sample."
                .into(),
        ..argument.grounds[0].clone()
    });
    let mut need = argument
        .evidence_plan
        .iter()
        .find(|need| need.target_id == "G1")
        .unwrap()
        .clone();
    need.id = "N8".into();
    need.target_id = "G2".into();
    argument.evidence_plan.push(need);
    if third {
        let mut route = argument.warrants[0].clone();
        route.id = "W3".into();
        argument.warrants.push(route);
        argument.assumptions[0].applies_to.push("W3".into());
    }
    for (before, after) in [("G1", "G2"), ("W1", "W2")]
        .into_iter()
        .chain(third.then_some(("W1", "W3")))
    {
        let mut requirement = support
            .requirements
            .iter()
            .find(|r| r.target_id == before)
            .unwrap()
            .clone();
        let old = requirement.id.clone();
        requirement.id = format!("support_{after}");
        requirement.target_id = after.into();
        requirement.evidence_need_ids.clear();
        requirement.original_acceptance_criteria.clear();
        if after == "G2" {
            let need = argument
                .evidence_plan
                .iter()
                .find(|need| need.id == "N8")
                .unwrap();
            requirement.evidence_need_ids.push(need.id.clone());
            requirement
                .original_acceptance_criteria
                .push(need.acceptance_criterion.clone());
        }
        let mut check = support
            .evaluations
            .iter()
            .find(|e| e.requirement_id == old)
            .unwrap()
            .clone();
        check.id = format!("check_{after}");
        check.requirement_id = requirement.id.clone();
        support.requirements.push(requirement);
        support.evaluations.push(check);
    }
    // The shortest original route fails its fixed applicability criterion.
    let short = support
        .requirements
        .iter_mut()
        .find(|r| r.target_id == "W1")
        .unwrap();
    short.acceptance = EvaluationCriterion::Equals {
        expected: json!(false),
    };
    support
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == short.id)
        .unwrap()
        .criterion = short.acceptance.clone();
    fs::create_dir(root.join("tools")).unwrap();
    fs::write(root.join("tools/read.py"), r#"import datetime,json,sys
request=json.load(sys.stdin)
assert request['input']['fixture']=='frozen-fixture-v1'
value={'measurement':{'count':3,'distinct':True}} if request['input']['operation']=='premise_source' else {'applies':True}
print(json.dumps({'value':value,'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z')}))
"#).unwrap();
    fs::write(root.join("tools.toml"), "[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/read.py']\ninherit_env=['PATH']\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/read.py']\ninherit_env=['PATH']\n").unwrap();
    let report = RunReport {
        schema_version: "engarg.run.v3".into(),
        execution_mode: "offline_synthetic_fixture".into(),
        input_sha256: eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
        direct_bindings: Some(DirectPlan::unbound(&support)),
        support_elaboration: Some(support),
        design_review: None,
        argument_revisions: vec![],
        target_runtime: None,
        authoring_recovery: None,
        evidence_planning: None,
        replanning: None,
        support_recovery: None,
        limitations: vec![],
    };
    engarg::report::validate(&report).unwrap();
    fs::write(
        root.join("input.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    report
}

fn run(root: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(root)
        .env_remove("ENGARG_MODEL")
        .args([
            "optimise",
            "--input",
            "input.json",
            "--tools",
            "tools.toml",
            "--output-dir",
            "result",
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn bounds_and_existing_output_fail_before_host_or_collection() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path(), false);
    for extra in [["--beam-width", "0"], ["--max-candidates", "257"]] {
        assert!(!run(root.path(), &extra).status.success());
        assert!(!root.path().join("result").exists());
    }
    fs::create_dir(root.path().join("result")).unwrap();
    fs::write(root.path().join("result/sentinel"), "original").unwrap();
    let result = run(root.path(), &["--eal-host", "/nonexistent/host"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("new directory"));
    assert_eq!(
        fs::read_to_string(root.path().join("result/sentinel")).unwrap(),
        "original"
    );
}

#[test]
fn output_cannot_enter_the_package_resource_tree() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path(), false);
    let result = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(root.path())
        .args([
            "optimise",
            "--input",
            "input.json",
            "--tools",
            "tools.toml",
            "--output-dir",
            "tools/result",
            "--eal-host",
            "/nonexistent/host",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("resource tree"));
    assert!(!root.path().join("tools/result").exists());
}

#[test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired native host"]
fn native_search_prefers_support_then_size_and_retains_explicit_revisions() {
    for third in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let original = fixture(root.path(), third);
        let before = fs::read(root.path().join("input.json")).unwrap();
        let result = run(root.path(), &[]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let receipt: Value = serde_json::from_slice(
            &fs::read(root.path().join("result/optimisation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            receipt["optimality"], "minimum_within_declared_routes",
            "{receipt}"
        );
        let candidates = receipt["search"]["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), if third { 3 } else { 2 });
        assert_eq!(candidates[0]["assessment"]["supported"], false);
        let selected = receipt["selected_report"].as_str().unwrap();
        let revised: Value =
            serde_json::from_slice(&fs::read(root.path().join("result").join(selected)).unwrap())
                .unwrap();
        let revised = engarg::report::read_direct(&revised).unwrap();
        assert_eq!(
            revised.argument.compositions[0].selected_warrant_ids,
            [if third { "W3" } else { "W2" }]
        );
        assert_eq!(revised.argument_revisions.len(), 1);
        assert_eq!(
            serde_json::to_value(&revised.argument.defeaters).unwrap(),
            serde_json::to_value(&original.argument.defeaters).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&revised.support_elaboration).unwrap(),
            serde_json::to_value(&original.support_elaboration).unwrap()
        );
        assert_eq!(before, fs::read(root.path().join("input.json")).unwrap());
        let executions: Vec<Value> = candidates
            .iter()
            .map(|candidate| {
                let dir = candidate["assessment"]["artifact"].as_str().unwrap();
                serde_json::from_slice(
                    &fs::read(root.path().join("result").join(dir).join("execution.json")).unwrap(),
                )
                .unwrap()
            })
            .collect();
        assert_ne!(
            executions[0]["result"]["source_digest"],
            executions[1]["result"]["source_digest"]
        );
        assert_ne!(
            executions[0]["result"]["collection"]["collection_id"],
            executions[1]["result"]["collection"]["collection_id"]
        );
    }
}

#[test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired native host"]
fn native_budget_or_unmet_global_condition_never_invents_a_winner() {
    for blocked in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut report = fixture(root.path(), false);
        if blocked {
            let support = report.support_elaboration.as_mut().unwrap();
            let global = support
                .requirements
                .iter_mut()
                .find(|r| r.id == "composition_condition")
                .unwrap();
            global.acceptance = EvaluationCriterion::Equals {
                expected: json!(false),
            };
            support
                .evaluations
                .iter_mut()
                .find(|e| e.requirement_id == global.id)
                .unwrap()
                .criterion = global.acceptance.clone();
            fs::write(
                root.path().join("input.json"),
                serde_json::to_vec(&report).unwrap(),
            )
            .unwrap();
        }
        let extra: &[&str] = if blocked {
            &[]
        } else {
            &["--max-candidates", "1"]
        };
        let result = run(root.path(), extra);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let receipt: Value = serde_json::from_slice(
            &fs::read(root.path().join("result/optimisation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["optimality"], "no_supported_candidate", "{receipt}");
        assert!(receipt["selected_report"].is_null());
        assert!(receipt["selected_package"].is_null());
        if !blocked {
            assert_eq!(receipt["search"]["search_complete"], false);
        }
    }
}
