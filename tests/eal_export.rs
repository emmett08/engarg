use std::{
    collections::BTreeSet,
    io::Write,
    process::{Command, Stdio},
};

use engarg::{
    domain::{ArgumentCase, Request, SupportRelationship},
    eal_export::{EalExport, export},
};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn declaration<'a>(bundle: &'a EalExport, id: &str, role: &str) -> &'a str {
    &bundle.source_map[id]
        .declarations
        .iter()
        .find(|entry| entry.role == role)
        .unwrap()
        .name
}

fn argument_line<'a>(bundle: &'a EalExport, id: &str, role: &str) -> &'a str {
    let name = declaration(bundle, id, role);
    bundle
        .source
        .lines()
        .find(|line| line.starts_with(&format!("argument {name} = ")))
        .unwrap()
}

#[test]
fn export_is_deterministic_and_preserves_complete_canonical_source() {
    let (request, case) = fixtures();
    let first = export(&request, &case).unwrap();
    let second = export(&request, &case).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&first.original_request).unwrap(),
        serde_json::to_value(request).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&first.original_case).unwrap(),
        serde_json::to_value(case).unwrap()
    );
    assert_eq!(first.context["engarg_export_digest"], first.revision);
    assert_eq!(first.claim_names.len(), 3);
    assert!(first.claim_names.contains_key("current_claim"));
    assert!(first.claim_names.contains_key("stronger_target"));
    assert!(first.source.starts_with("language \"EAL/3\"\n"));
    assert_eq!(first.target_language, "EAL/3");
    assert!(first.source.contains("method \"structured/1\""));
    assert!(!first.source.contains("method \"deductive/1\""));
}

#[test]
fn checked_flag_and_model_status_never_become_accepted_observations() {
    let (mut request, case) = fixtures();
    request.evidence[0].checked_by_user = true;
    let bundle = export(&request, &case).unwrap();
    assert!(bundle.source.contains("\"checked_by_user\":true"));
    assert!(!bundle.source.contains("\"accepted\":true"));
    assert_eq!(
        bundle.source.matches("require accepted == true").count(),
        bundle.obligations.len()
    );
    assert!(bundle.obligations.iter().any(
        |obligation| obligation.kind == "ground_interpretation" && obligation.target_id == "G2"
    ));
    assert!(bundle.obligations.iter().any(|obligation| obligation.kind == "warrant_adequacy" && obligation.target_id == "W1"));
    assert!(bundle.obligations.iter().any(
        |obligation| obligation.kind == "assumption_validation" && obligation.target_id == "A1"
    ));
    assert!(
        bundle
            .limitations
            .iter()
            .any(|limitation| limitation.contains("not an installed collector"))
    );
}

fn replace_id(value: &mut serde_json::Value, from: &str, to: &str) {
    match value {
        serde_json::Value::String(text) if text == from => *text = to.into(),
        serde_json::Value::Array(values) => values
            .iter_mut()
            .for_each(|value| replace_id(value, from, to)),
        serde_json::Value::Object(values) => values
            .values_mut()
            .for_each(|value| replace_id(value, from, to)),
        _ => {}
    }
}

#[test]
fn identifiers_are_injective_and_untrusted_unicode_text_stays_quoted() {
    let (mut request, case) = fixtures();
    let mut value = serde_json::to_value(case).unwrap();
    replace_id(&mut value, "G1", "a-b");
    replace_id(&mut value, "G2", "a_b");
    let mut case: ArgumentCase = serde_json::from_value(value).unwrap();
    let injection = "工程 🧪 \"}\nclaim injected { // \\ end\r\n";
    case.grounds[0].statement = injection.into();
    request.evidence[0].excerpt = injection.into();
    let bundle = export(&request, &case).unwrap();
    assert_ne!(
        declaration(&bundle, "a-b", "bounded_ground"),
        declaration(&bundle, "a_b", "bounded_ground")
    );
    assert!(bundle.source.contains("工程 🧪"));
    assert!(
        bundle
            .source
            .contains("\\\"}\\nclaim injected { // \\\\ end\\r\\n")
    );
    assert!(
        !bundle
            .source
            .lines()
            .any(|line| line.starts_with("claim injected"))
    );
    let mut names = BTreeSet::new();
    for mapping in bundle.source_map.values() {
        for entry in &mapping.declarations {
            assert!(
                entry
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            );
            assert!(
                names.insert(&entry.name),
                "duplicate declaration {}",
                entry.name
            );
        }
    }
}

fn add_second_route(case: &mut ArgumentCase, relationship: SupportRelationship) {
    let mut warrant = case
        .warrants
        .iter()
        .find(|warrant| warrant.conclusion_id == "current_claim")
        .unwrap()
        .clone();
    warrant.id = "W-alt".into();
    case.warrants.push(warrant);
    let composition = case
        .compositions
        .iter_mut()
        .find(|composition| composition.conclusion_id == "current_claim")
        .unwrap();
    composition.relationship = relationship;
    composition.selected_warrant_ids = if relationship == SupportRelationship::AlternativeRoutes {
        vec!["W2".into()]
    } else {
        vec!["W2".into(), "W-alt".into()]
    };
}

#[test]
fn joint_and_cumulative_routes_are_and_composed() {
    for relationship in [
        SupportRelationship::JointlyRequired,
        SupportRelationship::Cumulative,
    ] {
        let (request, mut case) = fixtures();
        add_second_route(&mut case, relationship);
        let bundle = export(&request, &case).unwrap();
        let line = argument_line(&bundle, "current_claim", "selected_composition");
        assert!(line.contains(declaration(&bundle, "W2", "candidate_route")));
        assert!(line.contains(declaration(&bundle, "W-alt", "candidate_route")));
        // Only one concluding application means EAL cannot reinterpret AND as OR.
        assert_eq!(
            bundle
                .source
                .lines()
                .filter(|line| line.starts_with("argument ")
                    && line.ends_with(&format!("=> {}", bundle.claim_names["current_claim"])))
                .count(),
            1
        );
    }
}

#[test]
fn alternatives_only_connect_selected_reliance_but_retain_candidates() {
    let (request, mut case) = fixtures();
    add_second_route(&mut case, SupportRelationship::AlternativeRoutes);
    let bundle = export(&request, &case).unwrap();
    let line = argument_line(&bundle, "current_claim", "selected_composition");
    assert!(line.contains(declaration(&bundle, "W2", "candidate_route")));
    assert!(!line.contains(declaration(&bundle, "W-alt", "candidate_route")));
    assert!(argument_line(&bundle, "W-alt", "candidate_route_support").contains("=> route_"));
    case.compositions
        .iter_mut()
        .find(|composition| composition.conclusion_id == "current_claim")
        .unwrap()
        .selected_warrant_ids
        .clear();
    let none = export(&request, &case).unwrap();
    assert!(
        !none.source.lines().any(|line| line.starts_with("argument ")
            && line.ends_with(&format!("=> {}", none.claim_names["current_claim"])))
    );
}

#[test]
fn unresolved_material_challenges_gate_the_exact_support_and_preserve_attack_targets() {
    let (request, case) = fixtures();
    let bundle = export(&request, &case).unwrap();
    let d1 = declaration(&bundle, "D1", "material_challenge_resolution");
    let d2 = declaration(&bundle, "D2", "material_challenge_resolution");
    assert!(argument_line(&bundle, "W3", "candidate_route_support").contains(d1));
    assert!(!argument_line(&bundle, "W2", "candidate_route_support").contains(d1));
    assert!(argument_line(&bundle, "G2", "ground_support").contains(d2));
    assert!(bundle.source.contains(&format!(
        "-x> reasoning {}",
        declaration(&bundle, "W3", "authored_warrant")
    )));
    assert!(bundle.source.contains(&format!(
        "-x> claim {}",
        declaration(&bundle, "G2", "bounded_ground")
    )));
    assert!(
        argument_line(&bundle, "W3", "candidate_route_support").contains(declaration(
            &bundle,
            "A1",
            "authored_assumption"
        ))
    );
    let mut case = case;
    case.defeaters[0].target_id = "current_claim".into();
    case.evidence_plan
        .iter_mut()
        .find(|need| need.target_id == "D1")
        .unwrap()
        .enables_claim = "current_claim".into();
    let claim_attack = export(&request, &case).unwrap();
    assert!(
        argument_line(&claim_attack, "current_claim", "selected_composition").contains(
            declaration(&claim_attack, "D1", "material_challenge_resolution")
        )
    );
}

#[test]
fn stronger_target_has_its_own_bounded_claim_and_dependency_path() {
    let (request, case) = fixtures();
    let bundle = export(&request, &case).unwrap();
    assert_ne!(
        bundle.claim_names["current_claim"],
        bundle.claim_names["stronger_target"]
    );
    assert!(
        argument_line(&bundle, "stronger_target", "selected_composition").contains(declaration(
            &bundle,
            "W3",
            "candidate_route"
        ))
    );
    assert!(bundle.source.contains(&case.stronger_target.qualifier));
    assert!(bundle.source.contains(&case.scope.time_horizon));
    assert!(
        bundle
            .obligations
            .iter()
            .any(|obligation| obligation.target_id == "G2")
    );
}

#[test]
fn invalid_source_is_rejected_instead_of_silently_repaired() {
    let (request, mut case) = fixtures();
    case.warrants[0].premise_ids = vec!["fabricated-ground".into()];
    assert!(export(&request, &case).is_err());
}

/// Set EAL_CORE to the upstream binary to exercise its real compiler and checker.
#[test]
fn upstream_core_validates_export_when_available() {
    let Some(binary) = std::env::var_os("EAL_CORE") else {
        return;
    };
    let (mut request, case) = fixtures();
    request.evidence[0].excerpt = "工程 🧪 \"}\nclaim injected { // \\".into();
    let mut cases = vec![case.clone()];
    for relationship in [
        SupportRelationship::JointlyRequired,
        SupportRelationship::Cumulative,
        SupportRelationship::AlternativeRoutes,
    ] {
        let mut variant = case.clone();
        add_second_route(&mut variant, relationship);
        cases.push(variant);
    }
    let mut assumption_target = case.clone();
    assumption_target.defeaters[0].target_id = "A1".into();
    cases.push(assumption_target);
    let mut objection_target = case;
    objection_target.defeaters[1].target_id = "D1".into();
    cases.push(objection_target);
    for case in cases {
        let bundle = export(&request, &case).unwrap();
        let mut child = Command::new(&binary)
            .arg("validate")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(bundle.source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "upstream EAL validation failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["valid"], true, "{result}");
    }
}

fn assumption_dependent_challenge() -> (Request, ArgumentCase) {
    let (request, mut case) = fixtures();
    case.assumptions[0].applies_to = vec!["D1".into()];
    case.defeaters[0].target_id = "current_claim".into();
    case.defeaters[0].material = false;
    case.defeaters[1].target_id = "A1".into();
    case.defeaters[1].status = engarg::domain::DefeaterStatus::Refuted;
    case.defeaters[1].material = false;
    case.defeaters[1].evidence_ids = vec!["E1".into()];
    for need in &mut case.evidence_plan {
        if ["D1", "D2"].contains(&need.target_id.as_str()) {
            need.enables_claim = "current_claim".into();
        }
    }
    (request, case)
}

#[test]
fn objection_conditions_retain_native_assumptions_through_premise_claims() {
    let (request, case) = assumption_dependent_challenge();
    let bundle = export(&request, &case).unwrap();
    let assumption = declaration(&bundle, "A1", "authored_assumption");
    let condition = declaration(&bundle, "A1", "assumption_condition_claim");
    let bridge = argument_line(&bundle, "A1", "assumption_condition_support");
    assert!(bridge.contains(&format!("assumptions {assumption}")));
    assert!(bridge.contains(declaration(&bundle, "A1", "assumption_validation")));
    assert!(
        !bridge.contains("premises "),
        "the bridge cannot introduce premise cycles"
    );
    let objection = declaration(&bundle, "D1", "evidence_gated_challenge");
    let line = bundle
        .source
        .lines()
        .find(|line| line.starts_with(&format!("objection {objection} = ")))
        .unwrap();
    assert!(line.contains(&format!("premises {condition}")));
    assert!(line.contains(declaration(&bundle, "D1", "challenge_activation")));
}

fn core_output(
    binary: &std::ffi::OsStr,
    workspace: &std::path::Path,
    args: &[&str],
) -> serde_json::Value {
    let output = Command::new(binary)
        .arg("--workspace")
        .arg(workspace)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "upstream core failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// This fixture contains only strings, booleans, objects, arrays and integers,
/// for which compact sorted serde JSON equals upstream canonical JSON. These
/// records are synthetic test data and never enter a production store/export.
fn synthetic_digest(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(serde_json::to_vec(value).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn synthetic_accepted_records(
    program: &serde_json::Value,
    context: &serde_json::Value,
) -> serde_json::Value {
    use serde_json::json;
    let mut records = serde_json::Map::new();
    for (id, evidence) in program["evidence"].as_object().unwrap() {
        let tool = &program["tools"][evidence["tool"].as_str().unwrap()];
        let input = &evidence["input"];
        let acquisition = json!({"tool":tool["name"],"tool_version":tool["version"],"input":input,"context":context});
        let request = json!({"evidence_id":id,"environment":evidence["environment"],"tool":tool["name"],"tool_version":tool["version"],"input":input,"context":context});
        let value =
            json!({"accepted":true,"fixture":"synthetic assumption propagation regression only"});
        records.insert(id.clone(), json!({
            "evidence_id":id,
            "source_digest":program["source_digest"],
            "tool":tool["name"],
            "tool_version":tool["version"],
            "tool_binding_digest":"0".repeat(64),
            "evidence_kind":evidence["kind"],
            "environment":evidence["environment"],
            "environment_fingerprint":synthetic_digest(&json!({"environment":evidence["environment"],"context":context})),
            "input_digest":synthetic_digest(input),
            "collected_at":"2026-10-23T12:00:00Z",
            "input":input,
            "context":context,
            "acquisition_request_digest":synthetic_digest(&acquisition),
            "acquisition_request":acquisition,
            "request_digest":synthetic_digest(&request),
            "run_id":"synthetic-assumption-propagation-regression",
            "status":"ok",
            "data_digest":synthetic_digest(&value),
            "value":value
        }));
    }
    serde_json::Value::Object(records)
}

/// Exercise the real EAL evaluator, including its grounded support/attack solver.
/// Set EAL_CORE to enable this regression in an environment with upstream built.
#[test]
fn upstream_assumption_attack_invalidates_dependent_objection_when_available() {
    let Some(binary) = std::env::var_os("EAL_CORE") else {
        return;
    };
    let (request, case) = assumption_dependent_challenge();
    let bundle = export(&request, &case).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("source.eal"), &bundle.source).unwrap();
    std::fs::write(
        workspace.path().join("context.json"),
        serde_json::to_vec(&bundle.context).unwrap(),
    )
    .unwrap();
    let program = core_output(&binary, workspace.path(), &["compile", "source.eal"]);
    let valid = core_output(&binary, workspace.path(), &["validate", "source.eal"]);
    assert_eq!(valid["valid"], true, "{valid}");
    let all_records = synthetic_accepted_records(&program, &bundle.context);
    let attack_gate = declaration(&bundle, "D2", "challenge_activation");
    let mut inactive_records = all_records.clone();
    inactive_records
        .as_object_mut()
        .unwrap()
        .remove(attack_gate);
    let evaluate = |records: &serde_json::Value| {
        std::fs::write(
            workspace.path().join("records.json"),
            serde_json::to_vec(records).unwrap(),
        )
        .unwrap();
        core_output(
            &binary,
            workspace.path(),
            &[
                "evaluate",
                "source.eal",
                "--now",
                "2026-10-23T12:00:00Z",
                "--context",
                "@context.json",
                "--records",
                "records.json",
            ],
        )
    };
    let before = evaluate(&inactive_records);
    let after = evaluate(&all_records);
    let assumption = declaration(&bundle, "A1", "authored_assumption");
    let condition = declaration(&bundle, "A1", "assumption_condition_claim");
    let dependent = declaration(&bundle, "D1", "evidence_gated_challenge");
    let attacker = declaration(&bundle, "D2", "evidence_gated_challenge");
    assert_eq!(
        before["assumptions"][assumption]["status"], "supported",
        "{before}"
    );
    assert_eq!(before["claims"][condition]["grounded_label"], "accepted");
    assert_eq!(before["objections"][dependent]["status"], "active");
    assert_eq!(before["objections"][attacker]["status"], "inactive");
    assert_eq!(
        after["evidence"][attack_gate]["status"], "available",
        "{after}"
    );
    assert_eq!(after["objections"][attacker]["status"], "active");
    assert_eq!(after["assumptions"][assumption]["status"], "contested");
    assert_eq!(after["claims"][condition]["grounded_label"], "rejected");
    assert_eq!(after["objections"][dependent]["status"], "defeated");
    assert_eq!(
        after["objections"][dependent]["premise_labels"][condition],
        "rejected"
    );
}
