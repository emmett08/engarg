//! Complete composed mode matrix through installed EAL and movable artefacts.
use engarg::{export_policy::ExportPolicy, package};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
use tempfile::TempDir;
#[path = "common/executable_case.rs"]
mod executable_case;
#[path = "common/mode_acceptance.rs"]
mod mode_acceptance;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn execute(index: usize, scenario: &str) -> (Value, String) {
    let original = TempDir::new().unwrap();
    let fixture = mode_acceptance::fixture(index, scenario);
    fs::create_dir(original.path().join("tools")).unwrap();
    let script = include_str!("fixtures/mode_observations.py").replace(
        "__PAYLOAD__",
        &serde_json::to_string(&fixture.observation.to_string()).unwrap(),
    );
    fs::write(original.path().join("tools/source.py"), script).unwrap();
    fs::write(
        original.path().join("tools/policy.py"),
        include_str!("fixtures/decision_policy.py"),
    )
    .unwrap();
    let manifest = original.path().join("tools.toml");
    let mut tools = String::new();
    for acquisition in &fixture.elaboration.acquisitions {
        let script = if acquisition.id == "policy_source" {
            "policy"
        } else {
            "source"
        };
        tools.push_str(&format!("[tools.{}]\nkind='command'\nversion='1'\nargv=['python3','tools/{script}.py']\ninherit_env=['PATH']\n",acquisition.id));
    }
    fs::write(&manifest, tools).unwrap();
    let built = original.path().join("built");
    let receipt = package::build(
        &fixture.request,
        &fixture.case,
        &fixture.elaboration,
        &manifest,
        &built,
        &ExportPolicy::default(),
    )
    .unwrap();
    let moved = TempDir::new().unwrap();
    fs::create_dir(moved.path().join("tools")).unwrap();
    for file in ["case.eal", "tools.toml", "tools/source.py"] {
        fs::copy(built.join(file), moved.path().join(file)).unwrap();
    }
    if index == 6 {
        fs::copy(
            built.join("tools/policy.py"),
            moved.path().join("tools/policy.py"),
        )
        .unwrap();
    }
    let source = fs::read_to_string(moved.path().join("case.eal")).unwrap();
    assert!(source.starts_with("language \"EAL/4\""));
    assert!(!source.contains("formal_source"));
    assert!(!source.contains("engarg_support_graph"));
    drop(original);
    let claim = receipt["claims"]["current_claim"]
        .as_str()
        .unwrap()
        .to_owned();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let mut command = Command::new(host);
    command.current_dir(moved.path()).args([
        "run",
        "case.eal",
        "--tools",
        "tools.toml",
        "--workspace",
        "run",
        "--claim",
        &claim,
        "--now",
        reasoning::NOW,
    ]);
    if scenario == "exhausted" && index < 6 {
        fs::write(
            moved.path().join("limits.toml"),
            "[limits]\nmethod_work = 1\n",
        )
        .unwrap();
        command.args(["--limits", "limits.toml"]);
    }
    let run = command.output().unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let response: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(response["is_error"], false, "{response}");
    (response["result"].clone(), claim)
}

fn computation(result: &Value, index: usize) -> &Value {
    let method = format!("{}/1", mode_acceptance::MODES[index]);
    &result["assessment"]["arguments"]
        .as_object()
        .unwrap()
        .values()
        .find(|a| a["reasoning_result"]["method"] == method)
        .unwrap()["reasoning_result"]
}

#[test]
fn every_matrix_fixture_is_a_valid_argument_with_complete_operation_dependencies() {
    for index in 0..7 {
        for scenario in [
            "positive",
            "negative",
            "unavailable",
            "stale",
            "inapplicable",
            "challenged",
            "resolved-challenge",
            "challenge-unavailable",
            "exhausted",
        ] {
            mode_acceptance::fixture(index, scenario);
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn seven_compiled_modes_execute_from_moved_source_tools_and_toml_only() {
    for index in 0..7 {
        let (result, claim) = execute(index, "positive");
        assert_eq!(
            result["assessment"]["claims"][&claim]["status"],
            "supported",
            "{}: {result}",
            mode_acceptance::MODES[index]
        );
        let acquisitions = result["collection"]["records"]
            .as_object()
            .unwrap()
            .values()
            .filter(|r| {
                r["tool"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("source_"))
            })
            .count();
        assert_eq!(
            acquisitions,
            if index == 6 {
                3
            } else if index == 5 {
                2
            } else {
                1
            }
        );
        if index < 6 {
            let details = &computation(&result, index)["details"];
            match index {
                0 => assert_eq!(details["entailed"], true),
                1 => {
                    assert!((details["lower"].as_f64().unwrap() - 0.8256343384950865).abs() < 1e-8)
                }
                2 => assert!((details["posterior"]["A"].as_f64().unwrap() - 0.9).abs() < 1e-12),
                3 => assert_eq!(details["estimate"], 6.0),
                4 => assert_eq!(details["difference"], 6.0),
                5 => assert_eq!(details["match_fraction"], 1.0),
                _ => unreachable!(),
            }
        } else {
            let policy = result["collection"]["records"]
                .as_object()
                .unwrap()
                .values()
                .find(|r| r["value"]["calculation"]["selected"] == "B")
                .unwrap();
            assert_eq!(policy["value"]["feasible_options"], json!(["B", "A"]));
            assert_eq!(policy["value"]["calculation"]["status"], "complete");
            assert_eq!(policy["value"]["work"]["performed"], 3);
            assert_eq!(policy["value"]["work"]["required"], 3);
            assert_eq!(
                policy["acquisition_request"]["input"]["data"]["options"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn every_mode_distinguishes_measured_negative_from_unavailable_stale_and_inapplicable() {
    for index in 0..7 {
        for scenario in ["negative", "unavailable", "stale", "inapplicable"] {
            let (result, claim) = execute(index, scenario);
            assert_ne!(
                result["assessment"]["claims"][&claim]["status"],
                "supported",
                "{} / {scenario}: {result}",
                mode_acceptance::MODES[index]
            );
            if scenario == "negative" {
                assert!(
                    result["collection"]["records"]
                        .as_object()
                        .unwrap()
                        .values()
                        .all(|r| r["status"] == "ok"),
                    "{result}"
                );
                if index < 6 {
                    let computed = computation(&result, index);
                    assert_eq!(computed["status"], "supported", "{result}");
                    let details = &computed["details"];
                    match index {
                        0 => assert_eq!(details["entailed"], false),
                        1 => assert!(details["lower"].as_f64().unwrap() < 0.5),
                        2 => assert!(
                            (details["posterior"]["A"].as_f64().unwrap() - 0.1).abs() < 1e-12
                        ),
                        3 => assert_eq!(details["estimate"], -6.0),
                        4 => assert_eq!(details["difference"], 0.0),
                        5 => assert_eq!(details["match_fraction"], 0.5),
                        _ => unreachable!(),
                    }
                } else {
                    assert!(
                        result["collection"]["records"]
                            .as_object()
                            .unwrap()
                            .values()
                            .any(|r| r["value"]["calculation"]["selected"] == "A")
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn every_mode_acquires_material_assumption_attacks_and_their_resolution() {
    for index in 0..7 {
        for scenario in ["challenged", "resolved-challenge", "challenge-unavailable"] {
            let (result, claim) = execute(index, scenario);
            assert_eq!(
                result["assessment"]["claims"][&claim]["status"] == "supported",
                scenario == "resolved-challenge",
                "{} / {scenario}: {result}",
                mode_acceptance::MODES[index]
            );
            assert!(
                !result["assessment"]["objections"]
                    .as_object()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                result["collection"]["records"]
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|r| r["acquisition_request"]["input"]["operation"] == "challenge_source")
            );
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn native_calculation_and_declared_decision_policy_exhaustion_never_claim_success() {
    for index in 0..7 {
        let (result, claim) = execute(index, "exhausted");
        assert_ne!(
            result["assessment"]["claims"][&claim]["status"],
            "supported",
            "{}: {result}",
            mode_acceptance::MODES[index]
        );
        if index < 6 {
            let method = computation(&result, index);
            assert_eq!(method["status"], "incomplete", "{result}");
            assert_eq!(method["details"], json!({}));
            assert_eq!(result["assessment"]["complete"], false);
        } else {
            let policy = result["collection"]["records"]
                .as_object()
                .unwrap()
                .values()
                .find(|r| r["value"]["calculation"]["status"] == "incomplete")
                .unwrap_or_else(|| panic!("Missing retained incomplete calculation: {result}"));
            assert_eq!(
                policy["status"], "ok",
                "The incomplete calculation record was acquired"
            );
            assert_eq!(
                policy["value"]["work"],
                json!({
                    "unit":"option_visit","allowance":1,"performed":1,"required":3
                })
            );
            assert!(policy["value"]["calculation"].get("selected").is_none());
            assert!(policy["value"].get("feasible_options").is_none());
            assert_eq!(
                policy["acquisition_request"]["input"]["data"]["options"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
        }
    }
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn transfer_and_decision_reviews_require_a_current_identified_applicable_verdict() {
    for index in [5, 6] {
        for scenario in [
            "wrong-scope",
            "rejected",
            "inconclusive",
            "missing-review-time",
            "missing-reviewer",
            "old-nested-review",
            "malformed-review-reasons",
            "blank-reviewer",
        ] {
            let (result, claim) = execute(index, scenario);
            assert_ne!(
                result["assessment"]["claims"][&claim]["status"],
                "supported",
                "{} / {scenario}: {result}",
                mode_acceptance::MODES[index]
            );
        }
    }
    let (result, claim) = execute(5, "disanalogy");
    assert_ne!(
        result["assessment"]["claims"][&claim]["status"], "supported",
        "{result}"
    );
}
