//! Exercise frozen verification, generated repairs and native source collection.
use engarg::{
    Error,
    domain::{ArgumentIndex, ReasoningMethodBinding},
    optimal_argument::{
        assurance::{self, CheckStatus},
        development::{self, DevelopmentConfig},
        execution::ExecutionConfig,
        investigation::VerifiedInvestigator,
    },
    provider::{Completion, CompletionRequest, LanguageModel, Role},
    support_requirements::SupportedArgument,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, fs, path::Path, process::Command, sync::Mutex, time::Duration};

#[path = "common/executable_case.rs"]
mod executable_case;
#[path = "common/investigation_case.rs"]
mod investigation_case;
#[path = "common/live_output.rs"]
mod live_output;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

fn envelope(proposal: &SupportedArgument) -> String {
    json!({"argument":live_output::generation_value(&proposal.argument),
        "support_elaboration_json":serde_json::to_string(&proposal.support_elaboration).unwrap()})
    .to_string()
}

struct Model {
    replies: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}
impl Model {
    fn new(replies: Vec<String>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(vec![]),
        }
    }
}
impl LanguageModel for Model {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.requests.lock().unwrap().push(request);
        Ok(Completion {
            content: self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("declared model reply"),
            model: "protocol-test-model".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

fn run_cli(root: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(root)
        .env_remove("ENGARG_MODEL")
        .args([
            "investigate",
            "--input",
            "input.json",
            "--tools",
            "tools.toml",
            "--verification",
            "verification.json",
            "--output-dir",
            "result",
            "--model",
            "unused-admission-test-model",
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn investigation_help_explains_the_frozen_contract_and_call_budget() {
    let root = tempfile::tempdir().unwrap();
    let output = run_cli(root.path(), &["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--verification"));
    assert!(help.contains("--max-calls"));
    assert!(help.contains("Failed calls consume"));
    assert!(!root.path().join("result").exists());
}

#[test]
fn incomplete_operator_plan_is_rejected_before_host_provider_or_output() {
    let root = tempfile::tempdir().unwrap();
    let (_, _, mut plan) = investigation_case::fixture(root.path());
    plan.checks
        .retain(|check| check.role != assurance::CheckRole::Formalisation);
    fs::write(
        root.path().join("verification.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let output = run_cli(root.path(), &["--eal-host", "/nonexistent/host"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("formalisation check"), "{error}");
    assert!(!root.path().join("result").exists());
}

#[test]
fn complete_plan_queries_deserialize_before_missing_host_is_reported() {
    let root = tempfile::tempdir().unwrap();
    investigation_case::fixture(root.path());
    let output = run_cli(root.path(), &["--eal-host", "/nonexistent/host"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("No such file or directory"), "{error}");
    assert!(!root.path().join("result").exists());
}

#[test]
fn frozen_checks_reject_wrong_query_criteria_and_missing_applicability() {
    let root = tempfile::tempdir().unwrap();
    let (original, proposal, plan) = investigation_case::fixture(root.path());
    let baseline = SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().unwrap(),
    };
    assert!(
        !assurance::prepare(&original, &baseline, &plan)
            .unwrap()
            .ready_for_execution()
    );
    let checked = assurance::prepare(&original, &proposal, &plan).unwrap();
    assert!(checked.ready_for_execution(), "{checked:?}");
    assert!(
        !checked.passed(),
        "Actual premise collection must still be pending"
    );
    assert_eq!(
        checked.discharged_requirement_ids,
        [investigation_case::REDUNDANT_REQUIREMENT]
    );
    let mut wrong_query = proposal.clone();
    let Some(ReasoningMethodBinding::Bound(bound)) =
        &mut wrong_query.argument.warrants[0].reasoning_method
    else {
        unreachable!()
    };
    bound.method.query["conclusion"] = json!("p");
    let receipt = assurance::prepare(&original, &wrong_query, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    assert!(receipt.discharged_requirement_ids.is_empty());
    let mut weakened = proposal.clone();
    let requirement = weakened
        .support_elaboration
        .requirements
        .iter_mut()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap();
    requirement.acceptance = eal_collector_contracts::execution::EvaluationCriterion::AllTrue;
    weakened
        .support_elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .unwrap()
        .criterion = requirement.acceptance.clone();
    assert!(
        !assurance::prepare(&original, &weakened, &plan)
            .unwrap()
            .ready_for_execution()
    );
    let mut removed_condition = proposal;
    removed_condition.argument.assumptions.clear();
    if let Some(ReasoningMethodBinding::Bound(bound)) =
        &mut removed_condition.argument.warrants[0].reasoning_method
    {
        bound.assumption_ids.clear();
    }
    let result = assurance::prepare(&original, &removed_condition, &plan);
    assert!(!result.is_ok_and(|receipt| receipt.ready_for_execution()));
}

#[tokio::test]
#[ignore = "requires ENGARG_EAL_HOST selecting the paired native host"]
async fn native_investigation_retains_legacy_native_result_without_grounding_approval() {
    let root = tempfile::tempdir().unwrap();
    let (original, proposal, plan) = investigation_case::fixture(root.path());
    let original_digest = assurance::digest(&original).unwrap();
    let mut wrong_query = proposal.clone();
    let Some(ReasoningMethodBinding::Bound(bound)) =
        &mut wrong_query.argument.warrants[0].reasoning_method
    else {
        unreachable!()
    };
    bound.method.query["conclusion"] = json!("p");
    let mut weakened = proposal.clone();
    let requirement = weakened
        .support_elaboration
        .requirements
        .iter_mut()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap();
    requirement.acceptance = eal_collector_contracts::execution::EvaluationCriterion::AllTrue;
    weakened
        .support_elaboration
        .evaluations
        .iter_mut()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .unwrap()
        .criterion = requirement.acceptance.clone();
    let model = Model::new(vec![envelope(&wrong_query), envelope(&weakened), envelope(&proposal), json!({"critical_issues":[],"observations":["The immutable finite proof, exact mapping and separately collected protocol conditions all passed; no physical conclusion is established."]}).to_string()]);
    let output = root.path().join("result");
    fs::create_dir(&output).unwrap();
    let investigator = VerifiedInvestigator {
        original: original.clone(),
        plan: plan.clone(),
        execution: ExecutionConfig {
            tools: root.path().join("tools.toml"),
            output: output.clone(),
            eal_host: std::env::var_os("ENGARG_EAL_HOST")
                .expect("ENGARG_EAL_HOST")
                .into(),
            timeout: Duration::from_secs(120),
            collector_env: vec!["PATH".into()],
            public_collector_env: vec![],
            policy: Default::default(),
        },
    };
    let events = Mutex::new(Vec::<Value>::new());
    let observer = |event: &Value| -> Result<(), Error> {
        events.lock().unwrap().push(event.clone());
        Ok(())
    };
    let result = development::develop(
        &model,
        &original,
        DevelopmentConfig {
            rounds: 3,
            max_calls: 4,
            max_input_bytes: 1_048_576,
            max_response_bytes: 1_048_576,
            beam_width: 1,
        },
        &json!({"verification_plan":plan,"protocol":"synthetic finite fixture"}),
        &investigator,
        &observer,
    )
    .await
    .unwrap();
    assert_eq!(result.calls_attempted, 4);
    assert_eq!(result.candidates.len(), 4);
    assert_eq!(
        result.candidates[0].outcome.assessment.status,
        "verification_failed"
    );
    assert!(!result.candidates[1].eligible);
    assert!(!result.candidates[2].eligible);
    // Historical plan/1 checks establish the formal query and collected
    // predicates, but never certify the missing source-to-symbol meanings.
    assert!(
        result.best.is_none(),
        "{}",
        serde_json::to_string_pretty(&result).unwrap()
    );
    let retained = &result.candidates[3];
    assert!(!retained.outcome.assessment.supported);
    assert_eq!(
        retained.outcome.assessment.status,
        "verification_incomplete"
    );
    assert!(!retained.eligible);
    assert_eq!(retained.length, Some(4));
    assert_eq!(result.candidates[0].length, Some(5));
    assert!(!result.search_complete);
    let report = retained.outcome.report.as_ref().unwrap();
    assert_eq!(
        report.argument.current_claim.text,
        original.argument.current_claim.text
    );
    assert_eq!(
        assurance::digest(&report.argument.scope).unwrap(),
        assurance::digest(&original.argument.scope).unwrap()
    );
    assert_eq!(
        report.argument.grounds.len(),
        original.argument.grounds.len()
    );
    assert!(
        ArgumentIndex::new(&report.argument)
            .selected_dependency("current_claim")
            .contains("A1")
    );
    assert_eq!(report.argument_revisions.len(), 1);
    assert_eq!(report.transcript.len(), original.transcript.len());
    assert_eq!(assurance::digest(&original).unwrap(), original_digest);
    let receipt: assurance::AssuranceReceipt =
        serde_json::from_value(retained.outcome.verification.clone()).unwrap();
    assert!(!receipt.passed(), "{receipt:?}");
    assert!(
        receipt
            .checks
            .iter()
            .any(|check| check.status == CheckStatus::Unknown
                && check.details["legacy_missing_coverage"] == true)
    );
    for check in receipt
        .checks
        .iter()
        .filter(|check| check.id.starts_with("source_"))
    {
        assert!(check.details["observed_at"].is_string(), "{check:?}");
        assert!(check.details["assessed_at"].is_string(), "{check:?}");
        assert_eq!(check.details["record_status"], "ok");
    }
    let retained_path = output.join(&retained.outcome.assessment.artifact);
    for file in [
        "report.json",
        "execution.json",
        "verification.json",
        "investigation-assessment.json",
        "package/case.eal",
    ] {
        assert!(
            retained_path.join(file).exists(),
            "Missing retained artifact {file}"
        );
    }
    let native_assessment: Value =
        serde_json::from_slice(&fs::read(retained_path.join("assessment.json")).unwrap()).unwrap();
    assert_eq!(
        native_assessment["supported"], true,
        "The new unresolved grounding gate must not manufacture a native arithmetic failure"
    );
    assert!(output.join("candidates/0001/verification.json").exists());
    assert!(output.join("candidates/0002/verification.json").exists());
    let requests = model.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.role)
            .collect::<Vec<_>>(),
        [
            Role::Constructor,
            Role::Constructor,
            Role::Constructor,
            Role::Sceptic
        ]
    );
    assert!(
        requests[1]
            .user
            .contains("differs from the independent intended formalisation")
    );
    assert!(
        requests[2]
            .user
            .contains("was removed or rewritten without a checked discharge")
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "model_completion")
    );
}

/// Explicitly requested fixture export supports an inspectable real-model demo.
#[test]
#[ignore = "set ENGARG_INVESTIGATION_FIXTURE_DIR to a new empty scratch directory"]
fn export_synthetic_investigation_fixture() {
    let root = std::path::PathBuf::from(
        std::env::var_os("ENGARG_INVESTIGATION_FIXTURE_DIR")
            .expect("ENGARG_INVESTIGATION_FIXTURE_DIR"),
    );
    if !root.exists() {
        fs::create_dir(&root).unwrap();
    }
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        0,
        "Fixture export needs an empty directory"
    );
    investigation_case::fixture(&root);
}
