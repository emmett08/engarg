//! Independent end-to-end acceptance and adverse source/semantic controls.
use engarg::{
    domain::ArgumentIndex,
    optimal_argument::{
        assurance::{self, CheckStatus},
        development::{
            ArtifactReference, CandidateInvestigator, CriticalIssue, IssueKind, candidate_digest,
            issue_digest,
        },
        execution::ExecutionConfig,
        investigation::VerifiedInvestigator,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[path = "common/executable_case.rs"]
mod executable_case;
#[path = "common/grounded_assurance_case.rs"]
mod grounded_assurance_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn check_fixture(fixture: &grounded_assurance_case::Fixture) {
    engarg::report::validate(&fixture.original).unwrap();
    engarg::domain::validate_case(&fixture.original.request, &fixture.proposal.argument).unwrap();
    let errors = engarg::support_requirements::validate_elaboration(
        &fixture.proposal.argument,
        &fixture.proposal.support_elaboration,
    );
    assert!(errors.is_empty(), "{}", errors.join("; "));
    fixture.plan.validate(&fixture.original).unwrap();
}

#[test]
fn empirical_case_has_explicit_atom_meanings_and_complete_material_contracts() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::empirical(root.path(), "positive");
    check_fixture(&fixture);
    let selected =
        ArgumentIndex::new(&fixture.proposal.argument).selected_dependency("current_claim");
    assert_eq!(selected.len(), 4);
    let declared: BTreeSet<_> = fixture
        .plan
        .verification
        .iter()
        .map(|c| c.target_id.as_str())
        .collect();
    assert_eq!(selected.into_iter().collect::<BTreeSet<_>>(), declared);
    let receipt = assurance::prepare(&fixture.original, &fixture.proposal, &fixture.plan).unwrap();
    assert!(receipt.ready_for_execution(), "{receipt:?}");
    assert!(
        !receipt.passed(),
        "Actual native source observations must still be pending"
    );
    let specification = serde_json::to_value(&fixture.plan).unwrap();
    let atoms = &specification["verification"][2]["atoms"];
    assert_eq!(atoms[0]["symbol"], "count_matches");
    assert_eq!(
        atoms[0]["predicate"]["expression"]["arguments"][0]["value"],
        "count"
    );
    assert_eq!(atoms[1]["symbol"], "identifiers_distinct");
    assert_eq!(atoms[1]["predicate"]["expression"]["value"], "distinct");
}

#[test]
fn native_expression_predicates_recompute_raw_ids_instead_of_trusting_summaries() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::empirical(root.path(), "positive");
    let contract = fixture
        .plan
        .verification
        .iter()
        .find(|c| c.target_id == "W1")
        .unwrap();
    // These are independently supplied algorithm-test inputs, not source
    // observations or a claim that any physical device was measured.
    let mut query = json!({
        "coverage":{"target_id":"W1","mode":"empirical",
            "formulas":[{"id":"count","formula":"count_matches"},{"id":"unique","formula":"identifiers_distinct"}],
            "required_obligations":[{"id":"requirement_W1","role":"warrant"}]},
        "contract":contract,
        "observations":{"premise_source":{"status":"available",
            "value":{"identifiers":["alpha","beta","gamma"],
                "measurement":{"count":3,"distinct":true},"accepted":true,
                "identity":grounded_assurance_case::DEVICE,"unit":"1",
                "scope":{"fixture":"frozen-fixture-v1"},"protocol":grounded_assurance_case::PROTOCOL,
                "valid_from":grounded_assurance_case::START,"valid_until":grounded_assurance_case::END},
            "observed_at":"2026-10-30T12:00:00Z","source_digest":"a".repeat(64)}},
        "assessed_at":"2026-10-30T12:00:01Z"
    });
    let checked =
        eal_core::verification::evaluate_query(&query, &eal_core::ExecutionLimits::default())
            .unwrap();
    assert_eq!(checked.status, eal_core::verification::Status::Pass);
    query["observations"]["premise_source"]["value"]["identifiers"] =
        json!(["alpha", "alpha", "gamma"]);
    let refuted =
        eal_core::verification::evaluate_query(&query, &eal_core::ExecutionLimits::default())
            .unwrap();
    assert_eq!(refuted.status, eal_core::verification::Status::Fail);
    let distinct = refuted
        .atoms
        .iter()
        .find(|a| a.symbol == "identifiers_distinct")
        .unwrap();
    assert_eq!(distinct.truth, Some(false));
    assert_eq!(
        distinct.details["read_fields"]["identifiers"],
        json!(["alpha", "alpha", "gamma"])
    );
    assert_eq!(
        query["observations"]["premise_source"]["value"]["measurement"]["distinct"],
        true
    );
    assert_eq!(
        query["observations"]["premise_source"]["value"]["accepted"],
        true
    );
}

#[test]
fn missing_material_contract_or_obligation_cannot_be_certified() {
    let root = tempfile::tempdir().unwrap();
    let mut fixture = grounded_assurance_case::empirical(root.path(), "positive");
    fixture.plan.verification.retain(|c| c.target_id != "A1");
    let receipt = assurance::prepare(&fixture.original, &fixture.proposal, &fixture.plan).unwrap();
    assert!(
        !receipt.ready_for_execution(),
        "A critical applicability target needs its own contract"
    );
    assert!(!receipt.passed());
    let root = tempfile::tempdir().unwrap();
    let mut fixture = grounded_assurance_case::empirical(root.path(), "positive");
    fixture
        .plan
        .verification
        .iter_mut()
        .find(|c| c.target_id == "W1")
        .unwrap()
        .obligations
        .clear();
    let receipt = assurance::prepare(&fixture.original, &fixture.proposal, &fixture.plan).unwrap();
    assert!(
        !receipt.ready_for_execution(),
        "A named target alone cannot cover its warrant obligation"
    );
}

#[test]
fn fixed_source_authority_rejects_policy_and_target_drift() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::empirical(root.path(), "positive");
    for mismatch in ["identity", "scope", "unit", "age"] {
        let mut policy = fixture.plan.clone();
        let applicability = &mut policy.verification[0].applicability[0];
        match mismatch {
            "identity" => applicability.identity = json!("another-device"),
            "scope" => applicability.scope = json!({"fixture":"another-fixture"}),
            "unit" => applicability.unit = "V".into(),
            "age" => applicability.max_age_seconds = 3601,
            _ => unreachable!(),
        }
        assert!(
            policy.validate(&fixture.original).is_err(),
            "{mismatch}: submitted policy cannot broaden or replace the original source contract"
        );
    }
    for field in ["subject", "valid_until"] {
        let mut value = serde_json::to_value(&fixture.proposal).unwrap();
        value["argument"]["warrants"][0]["reasoning_method"]["conclusion"][field] =
            if field == "subject" {
                json!("another-device")
            } else {
                json!("2099-01-01T00:00:00Z")
            };
        let changed = serde_json::from_value(value).unwrap();
        let receipt = assurance::prepare(&fixture.original, &changed, &fixture.plan).unwrap();
        assert!(
            !receipt.ready_for_execution(),
            "{field}: acquired source metadata cannot be reassigned to a different target"
        );
    }
    assert!(!root.path().join("result").exists());
}

#[test]
fn derived_claim_uses_verified_dependencies_without_a_duplicate_observation() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::empirical_composed(root.path(), "positive");
    check_fixture(&fixture);
    assert!(
        !fixture
            .plan
            .verification
            .iter()
            .any(|c| c.target_id == "current_claim")
    );
    assert!(
        !fixture
            .proposal
            .support_elaboration
            .requirements
            .iter()
            .any(|r| r.target_id == "current_claim")
    );
    let receipt = assurance::prepare(&fixture.original, &fixture.proposal, &fixture.plan).unwrap();
    assert!(receipt.ready_for_execution(), "{receipt:?}");
    let composed = receipt
        .checks
        .iter()
        .find(|c| c.id == "coverage:current_claim")
        .unwrap();
    assert_eq!(
        composed.details["composed_targets"],
        json!(["A1", "G1", "W1"])
    );
    let mut missing_warrant = fixture.plan.clone();
    missing_warrant.verification.retain(|c| c.target_id != "W1");
    let unresolved =
        assurance::prepare(&fixture.original, &fixture.proposal, &missing_warrant).unwrap();
    assert!(!unresolved.ready_for_execution());
}

#[test]
fn mathematical_theorem_has_no_empirical_support_design_or_atom_grounding() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::theorem(root.path());
    check_fixture(&fixture);
    assert!(engarg::support_requirements::material_targets(&fixture.proposal.argument).is_empty());
    assert!(fixture.proposal.support_elaboration.requirements.is_empty());
    assert!(fixture.proposal.support_elaboration.acquisitions.is_empty());
    assert!(fixture.proposal.argument.evidence_plan.is_empty());
    assert!(
        fixture
            .plan
            .verification
            .iter()
            .all(|c| c.atoms.is_empty() && c.applicability.is_empty() && c.obligations.is_empty())
    );
    assert_eq!(
        ArgumentIndex::new(&fixture.proposal.argument)
            .selected_dependency("current_claim")
            .len(),
        2
    );
    let receipt = assurance::prepare(&fixture.original, &fixture.proposal, &fixture.plan).unwrap();
    assert!(receipt.ready_for_execution(), "{receipt:?}");
    let proof = receipt
        .checks
        .iter()
        .find(|c| c.id == "closed_theorem")
        .unwrap();
    assert_eq!(proof.status, CheckStatus::Pass);
    assert!(proof.details.get("observed_at").is_none());
}

#[test]
fn mathematical_mode_cannot_hide_empirical_premises_or_a_false_theorem() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::theorem(root.path());
    for alteration in [
        "external_premise",
        "nonempty_query_premises",
        "false_theorem",
        "empirical_mode",
    ] {
        let mut value = serde_json::to_value(&fixture.proposal).unwrap();
        match alteration {
            "external_premise" => {
                value["argument"]["grounds"] = json!([{"id":"G1","statement":"An unobserved physical claim.","status":"missing","evidence_ids":[]}]);
                value["argument"]["warrants"][0]["premise_ids"] = json!(["G1"]);
            }
            "nonempty_query_premises" => {
                value["argument"]["warrants"][0]["reasoning_method"]["method"]["query"]["premises"] =
                    json!(["p"])
            }
            "false_theorem" => {
                value["argument"]["warrants"][0]["reasoning_method"]["method"]["query"]["conclusion"] =
                    json!("p")
            }
            "empirical_mode" => {
                value["argument"]["warrants"][0]["reasoning_method"]["claim_mode"] =
                    json!("empirical")
            }
            _ => unreachable!(),
        }
        let proposal = serde_json::from_value(value).unwrap();
        let outcome = assurance::prepare(&fixture.original, &proposal, &fixture.plan);
        assert!(
            outcome.is_err() || !outcome.unwrap().ready_for_execution(),
            "{alteration}"
        );
    }
}

fn investigator(root: &Path, fixture: &grounded_assurance_case::Fixture) -> VerifiedInvestigator {
    let output = root.join("result");
    fs::create_dir(&output).unwrap();
    VerifiedInvestigator {
        original: fixture.original.clone(),
        plan: fixture.plan.clone(),
        execution: ExecutionConfig {
            tools: root.join("tools.toml"),
            output,
            eal_host: PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("ENGARG_EAL_HOST")),
            timeout: Duration::from_secs(120),
            collector_env: vec![],
            public_collector_env: vec![],
            policy: Default::default(),
        },
    }
}

fn issue(
    id: &str,
    kind: IssueKind,
    target: &str,
    checks: &[&str],
    obligations: &[&str],
) -> CriticalIssue {
    CriticalIssue {
        id: id.into(),
        kind,
        target_id: target.into(),
        description: format!(
            "Independent test finding {id}: a named check, lifecycle state or correspondence requires inspection."
        ),
        obligation_ids: obligations.iter().map(|s| (*s).into()).collect(),
        check_ids: checks.iter().map(|s| (*s).into()).collect(),
        artifacts: vec![],
    }
}

#[tokio::test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired grounded-verification native host"]
async fn native_grounded_observations_close_all_material_checks_and_typed_state_findings() {
    let root = tempfile::tempdir().unwrap();
    let mut fixture = grounded_assurance_case::empirical(root.path(), "positive");
    // Force an explicit authored revision. The engine then installs its checked
    // dissent history, reproducing the real raw/admitted encoding difference.
    fixture.proposal.argument.reading[0].title = "Inspect the actual grounded snapshot".into();
    check_fixture(&fixture);
    let investigator = investigator(root.path(), &fixture);
    let outcome = investigator
        .assess(&fixture.original, fixture.proposal.clone(), 0)
        .await
        .unwrap();
    assert!(
        outcome.assessment.supported,
        "{}",
        serde_json::to_string_pretty(&outcome).unwrap()
    );
    let receipt: assurance::AssuranceReceipt =
        serde_json::from_value(outcome.verification.clone()).unwrap();
    assert!(receipt.passed());
    assert_eq!(receipt.verification_reports.len(), 4);
    let reports = serde_json::to_value(&receipt.verification_reports).unwrap();
    assert!(
        reports
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "pass")
    );
    let warrant = reports
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["target_id"] == "W1")
        .unwrap();
    assert_eq!(warrant["atoms"].as_array().unwrap().len(), 2);
    assert!(
        warrant["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["truth"] == true)
    );
    assert!(
        warrant["formulas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["truth"] == true)
    );
    assert_eq!(outcome.verification["lifecycle"]["phase"], "post_execution");
    assert_eq!(
        outcome.verification["lifecycle"]["runtime_status"],
        "supported"
    );
    let report = outcome.report.as_ref().unwrap();
    assert!(fixture.proposal.argument.dissent_revisions.is_none());
    assert!(report.argument.dissent_revisions.is_some());
    assert_eq!(report.argument_revisions.len(), 1);
    assert_ne!(
        assurance::digest(&fixture.proposal.argument).unwrap(),
        assurance::digest(&report.argument).unwrap()
    );
    assert_eq!(
        outcome.verification["lifecycle"]["bindings"]["raw_candidate_sha256"],
        assurance::digest(&fixture.proposal).unwrap()
    );
    assert_eq!(
        outcome.verification["lifecycle"]["bindings"]["normalization_checked"],
        true
    );
    assert_eq!(
        outcome.verification["lifecycle"]["bindings"]["admitted_candidate_sha256"],
        candidate_digest(report).unwrap()
    );
    assert_eq!(
        outcome.verification["lifecycle"]["bindings"]["admitted_report_sha256"],
        assurance::digest(report).unwrap()
    );
    let execution: Value = serde_json::from_slice(
        &fs::read(root.path().join("result/candidates/0000/execution.json")).unwrap(),
    )
    .unwrap();
    let source = execution["result"]["collection"]["records"]
        .as_object()
        .unwrap()
        .values()
        .find(|record| {
            record["value"]["identity"] == grounded_assurance_case::DEVICE
                && record["value"]["measurement"].is_object()
        })
        .expect("The independently measured snapshot record");
    assert_eq!(
        source["value"]["measurement"],
        json!({"count":3,"distinct":true})
    );
    assert_eq!(source["status"], "ok");
    assert!(source["evidence_id"].is_string());

    let mut issues = vec![
        issue(
            "missing_snapshot",
            IssueKind::MissingEvidence,
            "G1",
            &["source_requirement_G1"],
            &["requirement_G1"],
        ),
        issue(
            "planning_label",
            IssueKind::LifecycleConflict,
            "current_claim",
            &[],
            &[],
        ),
        issue(
            "binding",
            IssueKind::BindingMismatch,
            "current_claim",
            &[],
            &[],
        ),
        issue(
            "missing_warrant_verification",
            IssueKind::MissingVerification,
            "W1",
            &["coverage:W1"],
            &["requirement_W1"],
        ),
        issue("meaning", IssueKind::Semantic, "W1", &["coverage:W1"], &[]),
    ];
    let verification_digest = assurance::digest(&outcome.verification).unwrap();
    issues[1].artifacts = vec![ArtifactReference {
        path: "verification.json".into(),
        pointer: Some("/lifecycle".into()),
        sha256: Some(verification_digest.clone()),
    }];
    issues[2].artifacts = vec![ArtifactReference {
        path: "verification.json".into(),
        pointer: Some("/lifecycle/bindings".into()),
        sha256: Some(verification_digest),
    }];
    let resolutions = investigator
        .resolve_issues(&outcome, &issues)
        .await
        .unwrap();
    let resolved: BTreeSet<_> = resolutions.iter().map(|r| r.issue_sha256.clone()).collect();
    let expected: BTreeSet<_> = issues[..4]
        .iter()
        .map(|i| issue_digest(i).unwrap())
        .collect();
    assert_eq!(
        resolved, expected,
        "Semantic findings must never be cleared by passing state checks"
    );
    assert!(
        resolutions
            .iter()
            .all(|r| r.candidate_sha256 == candidate_digest(report).unwrap()
                && r.verification_sha256 == assurance::digest(&outcome.verification).unwrap())
    );

    let mut wrong_check = issues[0].clone();
    wrong_check.check_ids = vec!["unrelated_check".into()];
    let mut wrong_target = issues[0].clone();
    wrong_target.target_id = "W1".into();
    let mut wrong_obligation = issues[0].clone();
    wrong_obligation.obligation_ids = vec!["unrelated_obligation".into()];
    let mut wrong_digest = issues[0].clone();
    wrong_digest.artifacts = vec![ArtifactReference {
        path: "report.json".into(),
        pointer: None,
        sha256: Some("0".repeat(64)),
    }];
    assert!(
        investigator
            .resolve_issues(
                &outcome,
                &[wrong_check, wrong_target, wrong_obligation, wrong_digest]
            )
            .await
            .unwrap()
            .is_empty()
    );
    let mut no_lifecycle_artifact = issues[1].clone();
    no_lifecycle_artifact.artifacts.clear();
    let mut no_binding_artifact = issues[2].clone();
    no_binding_artifact.artifacts.clear();
    assert!(
        investigator
            .resolve_issues(&outcome, &[no_lifecycle_artifact, no_binding_artifact])
            .await
            .unwrap()
            .is_empty(),
        "Broad lifecycle/binding claims without inspected artifacts remain unresolved"
    );
    let mut wrong_binding = outcome.clone();
    wrong_binding.verification["lifecycle"]["bindings"]["admitted_candidate_sha256"] =
        json!("0".repeat(64));
    assert!(
        investigator
            .resolve_issues(&wrong_binding, &issues)
            .await
            .unwrap()
            .is_empty()
    );
    let mut changed_report = outcome.clone();
    changed_report
        .report
        .as_mut()
        .unwrap()
        .limitations
        .push("Unbound report metadata was changed after verification.".into());
    assert!(
        investigator
            .resolve_issues(&changed_report, &issues)
            .await
            .unwrap()
            .is_empty(),
        "A matching candidate graph cannot hide changed admitted-report metadata"
    );
    let mut before_execution = outcome.clone();
    before_execution.verification["lifecycle"]["phase"] = json!("pre_execution");
    assert!(
        investigator
            .resolve_issues(&before_execution, &[issues[1].clone()])
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired grounded-verification native host"]
async fn native_grounded_wrong_identity_unit_scope_time_and_missing_symbol_block_selection() {
    for fault in [
        "wrong_device",
        "wrong_unit",
        "wrong_scope",
        "wrong_interval",
        "stale",
        "future",
        "deceptive_accepted",
        "missing_measurement",
        "missing_symbol",
    ] {
        let root = tempfile::tempdir().unwrap();
        let fixture = grounded_assurance_case::empirical(root.path(), fault);
        let investigator = investigator(root.path(), &fixture);
        let outcome = investigator
            .assess(&fixture.original, fixture.proposal.clone(), 0)
            .await
            .unwrap();
        assert!(
            !outcome.assessment.supported,
            "{fault}: {}",
            serde_json::to_string_pretty(&outcome).unwrap()
        );
        let receipt: assurance::AssuranceReceipt =
            serde_json::from_value(outcome.verification).unwrap();
        assert!(!receipt.passed(), "{fault}");
        assert!(
            receipt.checks.iter().any(|c| matches!(
                c.status,
                CheckStatus::Fail | CheckStatus::Unknown | CheckStatus::Error
            )),
            "{fault}"
        );
        if fault == "deceptive_accepted" {
            let native: Value = serde_json::from_slice(
                &fs::read(root.path().join("result/candidates/0000/assessment.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                native["supported"], true,
                "Uninterpreted native entailment and summary gates deliberately pass this adversarial control"
            );
            let source = receipt
                .checks
                .iter()
                .find(|c| c.id == "source_requirement_G1")
                .unwrap();
            assert_eq!(source.status, CheckStatus::Pass);
            let warrant = receipt
                .verification_reports
                .iter()
                .find(|r| r.target_id == "W1")
                .unwrap();
            assert_eq!(warrant.status, eal_core::verification::Status::Fail);
            assert_eq!(
                warrant
                    .atoms
                    .iter()
                    .find(|a| a.symbol == "identifiers_distinct")
                    .unwrap()
                    .truth,
                Some(false)
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired grounded-verification native host"]
async fn native_grounded_derived_claim_composes_verified_warrants_without_new_source() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::empirical_composed(root.path(), "positive");
    let investigator = investigator(root.path(), &fixture);
    let outcome = investigator
        .assess(&fixture.original, fixture.proposal.clone(), 0)
        .await
        .unwrap();
    assert!(
        outcome.assessment.supported,
        "{}",
        serde_json::to_string_pretty(&outcome).unwrap()
    );
    let receipt: assurance::AssuranceReceipt =
        serde_json::from_value(outcome.verification).unwrap();
    assert!(receipt.passed());
    let composed = receipt
        .checks
        .iter()
        .find(|c| c.id == "coverage:current_claim")
        .unwrap();
    assert_eq!(
        composed.details["composed_targets"],
        json!(["A1", "G1", "W1"])
    );
    assert_eq!(composed.status, CheckStatus::Pass);
    assert!(
        !receipt
            .checks
            .iter()
            .any(|c| c.id == "source_composition_condition")
    );
    let dependencies = &composed.details["verification"]["obligations"];
    assert_eq!(dependencies.as_array().unwrap().len(), 3);
    assert!(
        dependencies
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["status"] == "pass" && o["details"]["report_sha256"].is_string())
    );
}

#[tokio::test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired native host; executes the real public build CLI"]
async fn native_pure_theorem_builds_and_executes_with_zero_observations() {
    let root = tempfile::tempdir().unwrap();
    let fixture = grounded_assurance_case::theorem(root.path());
    check_fixture(&fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(root.path())
        .args([
            "build",
            "--input",
            "input.json",
            "--tools",
            "tools.toml",
            "--output-dir",
            "package",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let built: Value = serde_json::from_slice(&output.stdout).unwrap();
    let claim = built["claims"]["current_claim"].as_str().unwrap();
    let native = Command::new(std::env::var_os("ENGARG_EAL_HOST").unwrap())
        .current_dir(root.path())
        .args([
            "run",
            "package/case.eal",
            "--tools",
            "package/tools.toml",
            "--workspace",
            "native-run",
            "--claim",
            claim,
        ])
        .output()
        .unwrap();
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let executed: Value = serde_json::from_slice(&native.stdout).unwrap();
    assert_eq!(executed["is_error"], false, "{executed}");
    assert_eq!(executed["result"]["assessment"]["valid"], true);
    assert_eq!(executed["result"]["assessment"]["complete"], true);
    assert_eq!(
        executed["result"]["assessment"]["claims"][claim]["status"],
        "supported"
    );
    assert!(
        executed["result"]["collection"]["records"]
            .as_object()
            .unwrap()
            .is_empty()
    );

    let investigator = investigator(root.path(), &fixture);
    let outcome = investigator
        .assess(&fixture.original, fixture.proposal.clone(), 0)
        .await
        .unwrap();
    assert!(
        outcome.assessment.supported,
        "{}",
        serde_json::to_string_pretty(&outcome).unwrap()
    );
    let receipt: assurance::AssuranceReceipt =
        serde_json::from_value(outcome.verification.clone()).unwrap();
    assert!(receipt.passed());
    assert_eq!(receipt.verification_reports.len(), 2);
    assert!(
        receipt
            .checks
            .iter()
            .all(|c| c.details.get("observed_at").is_none())
    );
    let retained: Value = serde_json::from_slice(
        &fs::read(root.path().join("result/candidates/0000/execution.json")).unwrap(),
    )
    .unwrap();
    assert!(
        retained["result"]["collection"]["records"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "set ENGARG_GROUNDED_FIXTURE_DIR to a new empty scratch directory"]
fn export_grounded_source_fixture() {
    let root = PathBuf::from(
        std::env::var_os("ENGARG_GROUNDED_FIXTURE_DIR").expect("ENGARG_GROUNDED_FIXTURE_DIR"),
    );
    if !root.exists() {
        fs::create_dir(&root).unwrap();
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    grounded_assurance_case::empirical(&root, "positive");
}

#[test]
#[ignore = "set ENGARG_MATHEMATICAL_FIXTURE_DIR to a new empty scratch directory"]
fn export_pure_mathematical_fixture() {
    let root = PathBuf::from(
        std::env::var_os("ENGARG_MATHEMATICAL_FIXTURE_DIR")
            .expect("ENGARG_MATHEMATICAL_FIXTURE_DIR"),
    );
    if !root.exists() {
        fs::create_dir(&root).unwrap();
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    grounded_assurance_case::theorem(&root);
}
