use std::{
    collections::BTreeSet,
    io::Write,
    process::{Command, Stdio},
};

use engarg::{
    domain::{ArgumentCase, Request, SupportRelationship},
    eal_export::EalExport,
};

#[path = "common/scoped_review_gates.rs"]
mod scoped_review_gates;

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
    let first = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    let second = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
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
    assert!(first.source.starts_with("language \"EAL/4\"\n"));
    assert_eq!(first.target_language, "EAL/4");
    assert!(first.source.contains("method \"structured/1\""));
    assert!(!first.source.contains("method \"deductive/1\""));
}

#[test]
fn structured_review_requests_retain_exact_inputs_without_reparsing_source() {
    let (request, mut case) = fixtures();
    let authored_text = "Retain this text\nevidence unrelated {\n  input {\"obligation\":{}}";
    case.grounds[0].statement = authored_text.into();
    let projection = scoped_review_gates::compile(&request, &case).unwrap();
    let bundle = &projection.bundle;
    let elaboration = scoped_review_gates::elaboration(&case);
    assert_eq!(
        projection.review_requests.len(),
        elaboration.requirements.len()
    );
    for review in &projection.review_requests {
        let requirement = elaboration
            .requirements
            .iter()
            .find(|r| review.input["requirement"] == r.id)
            .unwrap();
        assert_eq!(review.environment, "engarg_case");
        assert_eq!(review.tool, "engarg_review_contract");
        assert_eq!(review.tool_version, "1");
        assert_eq!(review.context, bundle.context);
        assert_eq!(review.input["target"], requirement.target_id);
        assert_eq!(
            review.input["role"],
            serde_json::to_value(requirement.role).unwrap()
        );
        assert_eq!(
            review.input["acceptance"],
            serde_json::to_value(&requirement.acceptance).unwrap()
        );
        assert!(serde_json::to_value(review).unwrap().get("value").is_none());
        let encoded_input = format!("  input {}", serde_json::to_string(&review.input).unwrap());
        assert!(bundle.source.lines().any(|line| line == encoded_input));
    }
    assert_eq!(bundle.original_case.grounds[0].statement, authored_text);
    scoped_review_gates::assert_scoped_review_gates(&projection);
}

#[test]
fn checked_flag_and_model_status_never_become_accepted_observations() {
    let (mut request, case) = fixtures();
    request.evidence[0].checked_by_user = true;
    let projection = scoped_review_gates::compile(&request, &case).unwrap();
    assert!(projection.bundle.original_request.evidence[0].checked_by_user);
    assert!(!projection.bundle.source.contains("\"accepted\":true"));
    scoped_review_gates::assert_scoped_review_gates(&projection);
    for target in ["G2", "W1", "A1"] {
        assert!(
            projection
                .review_requests
                .iter()
                .any(|r| r.input["target"] == target && r.input["role"] == "support")
        );
    }
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
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
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
            names.insert(&entry.name);
        }
    }
    // Usage edges may repeat one declaration in several source-map entries.
    // Each named evaluator is still emitted exactly once in the source.
    for name in names {
        assert_eq!(
            bundle
                .source
                .lines()
                .filter(|line| [
                    "evidence",
                    "claim",
                    "argument",
                    "assumption",
                    "reasoning",
                    "objection"
                ]
                .iter()
                .any(|kind| line.starts_with(&format!("{kind} {name} "))))
                .count(),
            1,
            "{name}"
        );
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
        let bundle = scoped_review_gates::compile(&request, &case)
            .unwrap()
            .bundle;
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
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
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
    let none = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    assert!(
        !none.source.lines().any(|line| line.starts_with("argument ")
            && line.ends_with(&format!("=> {}", none.claim_names["current_claim"])))
    );
}

#[test]
fn unresolved_material_challenges_gate_the_exact_support_and_preserve_attack_targets() {
    let (request, case) = fixtures();
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    let d1 = &scoped_review_gates::gate(&bundle, "D1", "challenge_resolution");
    let d2 = &scoped_review_gates::gate(&bundle, "D2", "challenge_resolution");
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
    let claim_attack = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    assert!(
        argument_line(&claim_attack, "current_claim", "selected_composition").contains(
            &scoped_review_gates::gate(&claim_attack, "D1", "challenge_resolution")
        )
    );
}

#[test]
fn stronger_target_has_its_own_bounded_claim_and_dependency_path() {
    let (request, case) = fixtures();
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
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
    assert!(!scoped_review_gates::gate(&bundle, "G2", "support").is_empty());
}

#[test]
fn invalid_source_is_rejected_instead_of_silently_repaired() {
    let (request, mut case) = fixtures();
    case.warrants[0].premise_ids = vec!["fabricated-ground".into()];
    assert!(scoped_review_gates::compile(&request, &case).is_err());
}

/// Set EAL_CORE to the upstream binary to exercise its real compiler and checker.
#[test]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
fn upstream_core_validates_export_when_available() {
    let binary = std::env::var_os("EAL_CORE").expect("compatibility test requires EAL_CORE");
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
        let bundle = scoped_review_gates::compile(&request, &case)
            .unwrap()
            .bundle;
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
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    let assumption = declaration(&bundle, "A1", "authored_assumption");
    let condition = declaration(&bundle, "A1", "assumption_condition_claim");
    let bridge = argument_line(&bundle, "A1", "assumption_condition_support");
    assert!(bridge.contains(&format!("assumptions {assumption}")));
    assert!(bridge.contains(&scoped_review_gates::gate(&bundle, "A1", "support")));
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
    assert!(line.contains(&scoped_review_gates::gate(
        &bundle,
        "D1",
        "challenge_activation"
    )));
}

/// The fixture acquires review records through the production runtime so derived
/// review checks retain their real dependency identities and measurement ages.
#[tokio::test]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
async fn upstream_assumption_attack_invalidates_dependent_objection_when_available() {
    use engarg::eal_host::{HostConfig, invoke};
    use serde_json::json;
    let core = std::path::PathBuf::from(
        std::env::var_os("EAL_CORE").expect("compatibility test requires EAL_CORE"),
    );
    let host = core.with_file_name(format!("eal-host{}", std::env::consts::EXE_SUFFIX));
    let (request, case) = assumption_dependent_challenge();
    let bundle = scoped_review_gates::compile(&request, &case)
        .unwrap()
        .bundle;
    let workspace = tempfile::tempdir().unwrap();
    let script = r#"import json,sys
acquisition=json.load(sys.stdin)
request=acquisition['input']
verdict='supported'
if sys.argv[1]=='inactive' and request['target']=='D2' and request['role']=='challenge_activation': verdict='inconclusive'
value=dict(criterion=request['criterion'],target=request['target'],scope=request['scope'],reviewer='synthetic-assumption-reviewer',verdict=verdict,reasons=['Synthetic assumption propagation regression only.'],reviewed_at='2026-10-23T12:00:00Z')
print(json.dumps({'value':value,'observed_at':value['reviewed_at'],'context':acquisition['context'],'request':{key:acquisition[key] for key in ['tool','tool_version','input','context']}}))
"#;
    let mut assessments = Vec::new();
    for mode in ["inactive", "active"] {
        let directory = workspace.path().join(mode);
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("review.py"), script).unwrap();
        let manifest = directory.join("tools.toml");
        std::fs::write(&manifest,format!("[tools.engarg_review_contract]\nkind='command'\nversion='1'\nargv=['python3','review.py','{mode}']\ninherit_env=['PATH']\n")).unwrap();
        let config = HostConfig {
            executable: host.clone(),
            workspace: directory,
            database: None,
            registry: Some(manifest),
            collector_env: vec!["PATH".into()],
            public_collector_env: vec!["PATH".into()],
            url: None,
            token_env: None,
            timeout: std::time::Duration::from_secs(30),
        };
        let response=invoke(&config,&json!({"operation":"execute","source":bundle.source,"context":bundle.context,"now":"2026-10-23T12:00:00Z"})).await.unwrap();
        assert_eq!(response["is_error"], false, "{response}");
        let records = response["result"]["collection"]["records"]
            .as_object()
            .unwrap();
        let program = eal_core::composition::compile(
            &bundle.source,
            None,
            "",
            eal_core::ExecutionLimits::default(),
        )
        .unwrap();
        for (id, evidence) in &program.evidence {
            let record = &records[id];
            assert_eq!(record["status"], "ok", "{id}: {record}");
            assert_eq!(
                record["collected_at"], "2026-10-23T12:00:00Z",
                "{id}: {record}"
            );
            for binding in &evidence.bindings {
                let parent = &records[&binding.evidence];
                let retained = record["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["evidence_id"] == binding.evidence)
                    .unwrap();
                assert_eq!(retained["run_id"], parent["run_id"]);
                assert_eq!(retained["data_digest"], parent["data_digest"]);
                assert_eq!(retained["collected_at"], parent["collected_at"]);
            }
        }
        assessments.push(response["result"]["assessment"].clone());
    }
    let before = &assessments[0];
    let after = &assessments[1];
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
    assert_eq!(after["objections"][attacker]["status"], "active", "{after}");
    assert_eq!(after["assumptions"][assumption]["status"], "contested");
    assert_eq!(after["claims"][condition]["grounded_label"], "rejected");
    assert_eq!(after["objections"][dependent]["status"], "defeated");
    assert_eq!(
        after["objections"][dependent]["premise_labels"][condition],
        "rejected"
    );
}
