//! Replanning performs configured discovery without another argument/model run.
#![cfg(unix)]
#[path = "common/support.rs"]
mod support_fixture;
use eal_collector_contracts::capability::CapabilityCatalogue;
use engarg::{
    report,
    route_planning::{DiscoverySnapshot, PlanningConfig},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    command
        .env_remove("OPENAI_API_KEY")
        .env_remove("ENGARG_API_KEY")
        .env_remove("ENGARG_MODEL");
    command
}
fn write(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn current_report(mut report: Value) -> Value {
    report["schema_version"] = json!(engarg::report::RUN_REPORT_SCHEMA);
    report["execution_mode"] = json!("offline_synthetic_fixture");
    report["input_sha256"] =
        json!(eal_collector_contracts::execution::canonical_digest(&report["request"]).unwrap());
    report["mode"] = json!("single");
    report["limitations"] = json!(["Synthetic discovery fixture; no provider calls."]);
    if report.get("transcript").is_none() {
        report["transcript"] = json!([]);
    }
    report
}

fn current_graph_report(root: &Path) -> Value {
    let (request, case, elaboration, _, _) = support_fixture::fixture();
    let catalogue = CapabilityCatalogue {
        schema: eal_collector_contracts::capability::CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities: vec![],
    };
    let configuration: PlanningConfig = serde_json::from_value(json!({
        "schema":engarg::route_planning::PLANNING_CONFIG_SCHEMA,
        "collector":root.join("not-executed"), "connections":root.join("connections.json"),
        "store":root.join("store"), "environment":[],
        "ranking":{"dimensions":["execution_requests"]},
        "scheduling":{"maximum_concurrent_sources":2,"per_provider":{}}, "bindings":[]
    }))
    .unwrap();
    let routes = engarg::route_planning::compile(
        &case,
        &elaboration,
        &catalogue,
        &configuration,
        &DiscoverySnapshot::default(),
    )
    .unwrap();
    current_report(json!({
        "request":request,"argument":case,
        "evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":routes}
    }))
}

#[test]
fn nullable_authoring_metadata_survives_raw_replanning_and_typed_reading() {
    let directory = tempfile::tempdir().unwrap();
    let mut original = current_graph_report(directory.path());
    original["authoring_recovery"] = Value::Null;
    report::read_direct(&original).unwrap();
    let mut replanned = original.clone();
    report::retain_replanning(&original, &mut replanned).unwrap();
    assert_eq!(replanned.get("authoring_recovery"), Some(&Value::Null));
    assert_eq!(
        replanned["replanning"]["input_report_sha256"],
        eal_collector_contracts::execution::canonical_digest(&original).unwrap()
    );
    // Raw history is intact before the consumer converts the same report into
    // typed fields. An optional null cannot make that conversion lose its origin.
    report::validate_replanning(&replanned, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    report::read_direct(&replanned).unwrap();
    let mut invalid_parent = original.clone();
    invalid_parent
        .as_object_mut()
        .unwrap()
        .remove("schema_version");
    let mut proposal = original;
    proposal["evidence_planning"]["routes"]["discovery"]
        .as_object_mut()
        .unwrap()
        .remove("limits");
    let before = proposal.clone();
    assert!(report::retain_replanning(&invalid_parent, &mut proposal).is_err());
    assert_eq!(
        proposal, before,
        "failed admission changed the proposed report"
    );
}

#[test]
fn nested_optional_and_default_encodings_survive_repeated_typed_replanning() {
    let directory = tempfile::tempdir().unwrap();
    let mut original = current_graph_report(directory.path());
    let mut request: engarg::domain::Request =
        serde_json::from_value(original["request"].clone()).unwrap();
    request.context.clear();
    original["request"] = serde_json::to_value(&request).unwrap();
    original["input_sha256"] = json!(engarg::authoring_checkpoint::digest(
        serde_json::to_vec(&request).unwrap()
    ));
    original["request"]
        .as_object_mut()
        .unwrap()
        .remove("context");
    original["argument"]["dissent_revisions"] = Value::Null;
    original["argument"]["warrants"][0]["reasoning_method"] = Value::Null;
    original["evidence_planning"]["routes"]["discovery"]
        .as_object_mut()
        .unwrap()
        .remove("limits");
    report::read_direct(&original).unwrap();
    let parent_digest = eal_collector_contracts::execution::canonical_digest(&original).unwrap();
    let mut replanned = original.clone();
    report::retain_replanning(&original, &mut replanned).unwrap();
    assert_eq!(
        replanned["replanning"]["input_report_sha256"],
        parent_digest
    );
    let encoding = replanned["replanning"]["previous_encoding"]
        .as_object()
        .unwrap();
    for pointer in [
        "/request/context",
        "/argument/dissent_revisions",
        "/argument/warrants/0/reasoning_method",
    ] {
        assert!(encoding.contains_key(pointer), "missing encoding {pointer}");
    }
    assert!(
        !encoding
            .keys()
            .any(|pointer| pointer.starts_with("/evidence_planning"))
    );
    assert!(replanned["evidence_planning"]["routes"]["discovery"]["limits"].is_null());
    let typed = serde_json::to_value(report::read_direct(&replanned).unwrap()).unwrap();
    report::validate_replanning(&typed, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    let mut next = typed.clone();
    report::retain_replanning(&typed, &mut next).unwrap();
    let typed_next = serde_json::to_value(report::read_direct(&next).unwrap()).unwrap();
    report::validate_replanning(&typed_next, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    assert_eq!(
        typed_next["replanning"]["previous_extensions"]["replanning"]["input_report_sha256"],
        parent_digest
    );
}

#[test]
fn encoding_restore_cannot_conceal_a_changed_argument() {
    let directory = tempfile::tempdir().unwrap();
    let original = current_graph_report(directory.path());
    let mut replanned = original.clone();
    report::retain_replanning(&original, &mut replanned).unwrap();
    replanned["argument"]["adjudication"] = json!("This unrecorded argument was changed.");
    replanned["replanning"]["previous_encoding"]["/argument/adjudication"] =
        json!({"kind":"value", "value":original["argument"]["adjudication"]});
    assert!(
        report::validate_replanning(&replanned, Path::new(env!("CARGO_BIN_EXE_engarg"))).is_err()
    );
    assert!(report::read_direct(&replanned).is_err());
}

#[test]
fn encoding_restore_cannot_conceal_changed_support_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let original = current_graph_report(directory.path());
    let mut replanned = original.clone();
    report::retain_replanning(&original, &mut replanned).unwrap();
    let pointer = "/elaboration/requirements/0/acceptance_rationale";
    let mut forged_planning = original["evidence_planning"].clone();
    *forged_planning.pointer_mut(pointer).unwrap() =
        json!("This support rationale was changed outside an explicit revision.");
    replanned["evidence_planning"] = forged_planning.clone();
    replanned["replanning"]["previous_evidence_planning"] = forged_planning.clone();
    let digest = eal_collector_contracts::execution::canonical_digest(&forged_planning).unwrap();
    replanned["replanning"]["current_planning_sha256"] = json!(digest);
    replanned["replanning"]["previous_planning_sha256"] = json!(digest);
    replanned["replanning"]["previous_encoding"][format!("/evidence_planning{pointer}")] =
        json!({"kind":"value", "value":original["evidence_planning"].pointer(pointer).unwrap()});
    assert!(
        report::validate_replanning(&replanned, Path::new(env!("CARGO_BIN_EXE_engarg"))).is_err()
    );
    assert!(report::read_direct(&replanned).is_err());
}

#[test]
fn credentials_restore_routes_without_changing_the_argument_or_requesting_model_calls() {
    let directory = tempfile::tempdir().unwrap();
    let collector = directory.path().join("discovery");
    let log = directory.path().join("discovery.log");
    let script = format!(
        r#"#!/usr/bin/python3
import json,os,pathlib,sys
assert sys.argv[1]=='evidence-discover',sys.argv
config=json.loads(pathlib.Path(sys.argv[sys.argv.index('--config')+1]).read_text())
capability=config['local_catalogues'][0]['capabilities'][0]
admitted=bool(os.environ.get('REPLAN_FIXTURE_TOKEN'))
with open({log:?},'a') as source: source.write(json.dumps({{'operation':'discovery','admitted':admitted}})+'\n')
print(json.dumps({{'schema':'eal.evidence-discovery/1','catalogue':config['local_catalogues'][0],'admitted_capabilities':[capability['id']] if admitted else [],'interfaces':[],'gaps':[] if admitted else [{{'kind':'configuration','description':'Fixture credential is absent'}}],'incomplete':False,'metrics':{{'requests':1,'request_bytes':0,'response_bytes':0,'elapsed_ms':0,'server_starts':1}},'limits':{{'requests':64,'bytes':4194304,'timeout_seconds':60,'pages_per_connection':16}},'budget_usage':{{'requests_upper_bound':1,'bytes_upper_bound':1024,'pages_by_connection':{{}}}}}}))
"#,
        log = log.to_str().unwrap()
    );
    std::fs::write(&collector, script).unwrap();
    std::fs::set_permissions(&collector, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (request, case, elaboration, _execution, routes) = support_fixture::fixture();
    let catalogue = CapabilityCatalogue {
        schema: eal_collector_contracts::capability::CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities: vec![routes[0].capability.clone()],
    };
    let connections = directory.path().join("connections.json");
    write(
        &connections,
        &json!({"schema":"eal.evidence-config/1","connections":[],"approved_catalogues":[],"local_catalogues":[catalogue]}),
    );
    let configuration:PlanningConfig=serde_json::from_value(json!({"schema":engarg::route_planning::PLANNING_CONFIG_SCHEMA,"collector":collector,"connections":connections,"store":directory.path().join("store"),"environment":["REPLAN_FIXTURE_TOKEN"],"ranking":{"dimensions":["execution_requests"]},"scheduling":{"maximum_concurrent_sources":2,"per_provider":{}},"bindings":[]})).unwrap();
    let initial = engarg::route_planning::compile(
        &case,
        &elaboration,
        &catalogue,
        &configuration,
        &DiscoverySnapshot::default(),
    )
    .unwrap();
    assert!(!initial.routes[0].configuration_gaps.is_empty());
    let config_path = directory.path().join("planning.json");
    write(&config_path, &configuration);
    let response = json!({"argument":case}).to_string();
    let digest = Sha256::digest(response.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let transcript = json!([{"role":"adjudicator","prompt":{"role":"adjudicator","system":"retained fixture","user":"{}","schema":{}},"response":{"content":response,"model":"original-model","input_tokens":7,"output_tokens":5,"request_id":"original-request","service_tier":null,"reasoning_effort":null},"response_content_bytes":response.len(),"response_content_sha256":digest,"response_content_omitted":false,"elapsed_ms":1}]);
    let original = current_report(
        json!({"request":request,"argument":case,"transcript":transcript,"tooling":null,"evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":initial}}),
    );
    let input = directory.path().join("original.json");
    write(&input, &original);
    let still_missing = directory.path().join("still-missing.json");
    let result = cli()
        .args(["eal", "replan", "--input"])
        .arg(&input)
        .arg("--evidence-config")
        .arg(&config_path)
        .arg("--output")
        .arg(&still_missing)
        .env_remove("REPLAN_FIXTURE_TOKEN")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let missing: Value = serde_json::from_slice(&std::fs::read(&still_missing).unwrap()).unwrap();
    assert!(
        !missing["evidence_planning"]["routes"]["routes"][0]["configuration_gaps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let restored = directory.path().join("restored.json");
    let result = cli()
        .args(["eal", "replan", "--input"])
        .arg(&still_missing)
        .arg("--evidence-config")
        .arg(&config_path)
        .arg("--output")
        .arg(&restored)
        .env("REPLAN_FIXTURE_TOKEN", "synthetic-fixture-credential")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let updated: Value = serde_json::from_slice(&std::fs::read(&restored).unwrap()).unwrap();
    for key in ["request", "argument", "transcript"] {
        assert_eq!(updated[key], original[key], "replan must preserve {key}");
    }
    assert_eq!(
        updated["evidence_planning"]["elaboration"],
        original["evidence_planning"]["elaboration"]
    );
    assert!(
        updated["evidence_planning"]["routes"]["routes"][0]["configuration_gaps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        updated["evidence_planning"]["routes"]["execution"]["acquisitions"][0]["availability"]["kind"],
        "configured"
    );
    assert_eq!(updated["replanning"]["model_calls"], 0);
    // Restoring one credential cannot discharge independently unavailable
    // sources, typed support or judgement requirements in the same report.
    let retained_gaps = updated["evidence_planning"]["routes"]["gaps"]
        .as_array()
        .unwrap();
    assert!(!retained_gaps.is_empty());
    for evaluation in original["evidence_planning"]["elaboration"]["evaluations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|evaluation| {
            matches!(
                evaluation["criterion"]["kind"].as_str(),
                Some("unavailable" | "judgement")
            )
        })
    {
        assert!(
            updated["evidence_planning"]["routes"]["execution"]["evaluations"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |retained| retained["requirement_id"] == evaluation["requirement_id"]
                        && retained["criterion"] == evaluation["criterion"]
                )
        );
    }
    assert!(updated.get("tooling").is_none());
    assert_eq!(
        updated["replanning"]["previous_extensions"]["replanning"]["previous_extensions"]["tooling"],
        Value::Null
    );
    assert!(
        !String::from_utf8(std::fs::read(&restored).unwrap())
            .unwrap()
            .contains("synthetic-fixture-credential")
    );
    report::validate_replanning(&updated, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    let calls = std::fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 2);
    assert!(
        calls
            .lines()
            .all(|line| serde_json::from_str::<Value>(line).unwrap()["operation"] == "discovery")
    );
    // An explicit per-requirement revision is retained as a separate, zero-model
    // replan transition and preserves stricter source freshness.
    let mut missing_age = original.clone();
    for requirement in missing_age["evidence_planning"]["elaboration"]["requirements"]
        .as_array_mut()
        .unwrap()
    {
        if requirement["id"] == "shared_requirement_0" {
            requirement["contract"]["time"]["max_age_seconds"] = Value::Null;
        }
    }
    for evaluation in missing_age["evidence_planning"]["elaboration"]["evaluations"]
        .as_array_mut()
        .unwrap()
    {
        if evaluation["requirement_id"] == "shared_requirement_0" {
            evaluation["max_age_seconds"] = Value::Null;
        }
    }
    for evaluation in missing_age["evidence_planning"]["routes"]["execution"]["evaluations"]
        .as_array_mut()
        .unwrap()
    {
        if evaluation["requirement_id"] == "shared_requirement_0" {
            evaluation["max_age_seconds"] = Value::Null;
        }
    }
    let missing_path = directory.path().join("missing-age.json");
    write(&missing_path, &missing_age);
    let check = cli()
        .args(["eal", "freshness-check", "--input"])
        .arg(&missing_path)
        .output()
        .unwrap();
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("shared_requirement_0"));
    let clarification = directory.path().join("freshness.json");
    write(
        &clarification,
        &json!({"schema":"engarg.freshness-clarification/1", "rationale":"Operator approved this fixture age", "requirements":{"shared_requirement_0":3600}}),
    );
    let clarified_path = directory.path().join("clarified.json");
    let result = cli()
        .args(["eal", "replan", "--input"])
        .arg(&missing_path)
        .arg("--evidence-config")
        .arg(&config_path)
        .arg("--freshness-clarifications")
        .arg(&clarification)
        .arg("--output")
        .arg(&clarified_path)
        .env("REPLAN_FIXTURE_TOKEN", "synthetic-fixture-credential")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let clarified: Value =
        serde_json::from_slice(&std::fs::read(&clarified_path).unwrap()).unwrap();
    assert_eq!(clarified["argument"], missing_age["argument"]);
    assert_eq!(clarified["transcript"], missing_age["transcript"]);
    assert_eq!(
        clarified["evidence_planning"]["elaboration"]["acquisitions"],
        missing_age["evidence_planning"]["elaboration"]["acquisitions"]
    );
    assert_eq!(clarified["replanning"]["model_calls"], 0);
    report::validate_replanning(&clarified, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    let check = cli()
        .args(["eal", "freshness-check", "--input"])
        .arg(&clarified_path)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let mut tampered_clarification = clarified.clone();
    tampered_clarification["replanning"]["freshness_clarification"]["requirements"]["shared_requirement_0"] =
        json!(3601);
    assert!(
        report::validate_replanning(
            &tampered_clarification,
            Path::new(env!("CARGO_BIN_EXE_engarg"))
        )
        .is_err()
    );
    // Discovery remains an optional authoring input. Execution receives explicit
    // operator bindings rather than interpreting the retained route sidecar.
    let tools = directory.path().join("tools.toml");
    std::fs::write(
        &tools,
        "[tools.shared_source]\nkind='command'\nversion='1'\nargv=['/usr/bin/false']\n",
    )
    .unwrap();
    let calls_before_export = std::fs::read(&log).unwrap();
    let exported = directory.path().join("executable-package");
    let result = cli()
        .args(["eal", "export", "--input"])
        .arg(&restored)
        .arg("--tools")
        .arg(&tools)
        .arg("--output")
        .arg(&exported)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["schema"], "engarg.executable-package/1");
    assert_eq!(receipt["language"], "EAL/4");
    assert!(receipt.get("profile").is_none());
    assert_eq!(receipt["model_calls"], 0);
    assert!(!receipt["pending_reviews"].as_array().unwrap().is_empty());
    let source = std::fs::read_to_string(exported.join("case.eal")).unwrap();
    assert!(source.starts_with("language \"EAL/4\""));
    assert!(exported.join("tools.toml").is_file());
    assert_eq!(std::fs::read(&log).unwrap(), calls_before_export);
    assert!(
        !directory.path().join("store").exists(),
        "export must not acquire evidence"
    );
    // Retained discovery does not replace operator authority. The public build
    // path must reject missing bindings, then compile the same program as export.
    let empty_tools = directory.path().join("unbound-tools.toml");
    std::fs::write(&empty_tools, "[tools]\n").unwrap();
    let unbound_package = directory.path().join("unbound-package");
    let result = cli()
        .args(["build", "--input"])
        .arg(&restored)
        .arg("--tools")
        .arg(&empty_tools)
        .arg("--output-dir")
        .arg(&unbound_package)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("Missing direct tool binding for shared_source")
    );
    assert!(!unbound_package.exists());
    let built = directory.path().join("built-package");
    let result = cli()
        .args(["build", "--input"])
        .arg(&restored)
        .arg("--tools")
        .arg(&tools)
        .arg("--output-dir")
        .arg(&built)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let build_receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(build_receipt, receipt);
    for file in ["case.eal", "tools.toml"] {
        assert_eq!(
            std::fs::read(built.join(file)).unwrap(),
            std::fs::read(exported.join(file)).unwrap()
        );
    }
    assert_eq!(std::fs::read(&log).unwrap(), calls_before_export);
    assert!(!directory.path().join("store").exists());
    let mut tampered = updated.clone();
    tampered["argument"]["adjudication"] = json!("changed after replanning");
    assert!(
        report::validate_replanning(&tampered, Path::new(env!("CARGO_BIN_EXE_engarg"))).is_err()
    );
    tampered = updated.clone();
    tampered["replanning"]["previous_evidence_planning"]["elaboration"]["requirements"][0]["acceptance_rationale"] =
        json!("changed previous obligation");
    assert!(
        report::validate_replanning(&tampered, Path::new(env!("CARGO_BIN_EXE_engarg"))).is_err()
    );

    // Sixteen immutable transitions remain readable. The next transition must
    // fail admission instead of silently discarding the original history.
    let mut retained = original;
    for _ in 0..report::MAX_REPLANNING_HISTORY {
        let mut next = retained.clone();
        report::retain_replanning(&retained, &mut next).unwrap();
        retained = next;
    }
    report::validate_replanning(&retained, Path::new(env!("CARGO_BIN_EXE_engarg"))).unwrap();
    let exhausted = directory.path().join("history-full.json");
    write(&exhausted, &retained);
    let rejected_output = directory.path().join("history-overflow.json");
    let calls_before = std::fs::read(&log).unwrap();
    let rejected = cli()
        .args(["eal", "replan", "--input"])
        .arg(&exhausted)
        .arg("--evidence-config")
        .arg(&config_path)
        .arg("--output")
        .arg(&rejected_output)
        .env("REPLAN_FIXTURE_TOKEN", "synthetic-fixture-credential")
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("already contains 16 revisions"));
    assert_eq!(std::fs::read(&log).unwrap(), calls_before);
    assert!(!rejected_output.exists());
    let mut excessive = retained.clone();
    report::retain_replanning(&retained, &mut excessive).unwrap();
    let error = report::validate_replanning(&excessive, Path::new(env!("CARGO_BIN_EXE_engarg")))
        .unwrap_err()
        .to_string();
    assert!(error.contains("history exceeds 16 revisions"), "{error}");
}

#[test]
fn oversized_replan_input_is_rejected_before_discovery_or_output_creation() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("oversized.json");
    std::fs::File::create(&input)
        .unwrap()
        .set_len(16 * 1_048_576 + 1)
        .unwrap();
    let output = directory.path().join("output.json");
    let result = cli()
        .args(["eal", "replan", "--input"])
        .arg(&input)
        .arg("--evidence-config")
        .arg(directory.path().join("unopened-config.json"))
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("byte limit"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn sole_missing_credential_recovers_the_complete_plan_without_model_calls() {
    use engarg::{
        domain::{ArgumentCase, Request},
        support_requirements::{SUPPORT_ELABORATION_SCHEMA, SupportElaboration},
    };
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let request: Request = serde_json::from_value(json!({
        "claim":"The synthetic counter for case dev reports zero.",
        "context":"A local protocol fixture checks credential recovery only. No real service or claim is assessed.",
        "evidence":[]
    })).unwrap();
    // This independent case has one literal observation obligation. It does not
    // remove the missing reviews or challenges from the richer fixture above.
    let acceptance = "The qualified synthetic counter returns the integer zero for case dev.";
    let case: ArgumentCase = serde_json::from_value(json!({
        "original_claim":request.claim,
        "current_claim":{"text":request.claim,"qualifier":"Pending collection from the identified synthetic counter."},
        "stronger_target":{"text":"A retained observation records the synthetic counter value for case dev.","qualifier":"Bound to its original observation time."},
        "status":"unsupported",
        "scope":{"system_boundary":"One synthetic counter","configuration":"case dev","operating_conditions":"Local deterministic fixture","units":"count","time_horizon":"The retained observation time","consequences":"Exercises planning recovery only"},
        "subclaims":[],"grounds":[],"warrants":[],"compositions":[],"assumptions":[],"defeaters":[],"alternatives":[],
        "evidence_plan":[{"id":"N1","target_id":"current_claim","question":"Does the synthetic counter report zero?","method":"Read the qualified fixture counter for case dev.","measurement":"An integer count with its original timestamp.","comparator":"The literal integer zero.","acceptance_criterion":acceptance,"if_failed":"Retain the measured nonzero value or acquisition failure.","enables_claim":"current_claim","priority":1,"cost":"One local fixture read."}],
        "reading":[{"claim_id":"current_claim","title":"A pending counter observation","paragraphs":[{"purpose":"answer","text":"The counter value remains unobserved until the configured source is collected.","reference_ids":["current_claim","N1"]}]}],
        "residual_risks":["This fixture establishes no real service property."],
        "decision_limits":["An executable plan supplies no observation by itself."],
        "adjudication":"The only planned acquisition needs its configured credential. The claim remains unsupported until collection."
    })).unwrap();
    engarg::domain::validate_argument(&request, &case).unwrap();
    let (_, _, shared, _, routes) = support_fixture::fixture();
    let contract = &shared.acquisitions[0].contract;
    let elaboration: SupportElaboration = serde_json::from_value(json!({
        "schema":SUPPORT_ELABORATION_SCHEMA,
        "requirements":[{"id":"counter_zero","target_id":"current_claim","evidence_need_ids":["N1"],"original_acceptance_criteria":[acceptance],"contract":contract,"discriminating_evidence":["A nonzero count rejects this literal criterion."],"acceptance":{"kind":"equals","expected":0},"acceptance_rationale":"Compare the measured count directly with zero.","gaps":[]}],
        "acquisitions":[{"id":"counter","contract":contract,"arguments":{"case":"dev"},"bindings":[],"for_each":null,"operation_spec":null}],
        "derivations":[],
        "evaluations":[{"id":"counter_evaluation","requirement_id":"counter_zero","inputs":[{"step_id":"counter","pointer":"/value"}],"criterion":{"kind":"equals","expected":0},"max_age_seconds":300,"required_coverage":"witness","applicability":[],"challenges":[]}],
        "revisions":[],"alternative_support":[]
    })).unwrap();
    assert!(engarg::support_requirements::validate_elaboration(&case, &elaboration).is_empty());
    let catalogue = CapabilityCatalogue {
        schema: eal_collector_contracts::capability::CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities: vec![routes[0].capability.clone()],
    };
    let collector = root.join("credential-discovery");
    let log = root.join("calls.log");
    std::fs::write(&collector, format!(r#"#!/usr/bin/python3
import json,os,pathlib,sys
assert sys.argv[1]=='evidence-discover',sys.argv
config=json.loads(pathlib.Path(sys.argv[sys.argv.index('--config')+1]).read_text())
catalogue=config['local_catalogues'][0]
ready=bool(os.environ.get('REPLAN_SINGLE_TOKEN'))
with open({log:?},'a') as stream: stream.write('discovery\n')
print(json.dumps({{'schema':'eal.evidence-discovery/1','catalogue':catalogue,'admitted_capabilities':[catalogue['capabilities'][0]['id']] if ready else [],'interfaces':[],'gaps':[] if ready else [{{'kind':'configuration','description':'The synthetic credential is missing'}}],'incomplete':False,'metrics':{{'requests':1,'request_bytes':0,'response_bytes':0,'elapsed_ms':0,'server_starts':1}},'limits':{{'requests':64,'bytes':4194304,'timeout_seconds':60,'pages_per_connection':16}},'budget_usage':{{'requests_upper_bound':1,'bytes_upper_bound':1024,'pages_by_connection':{{}}}}}}))
"#, log=log.to_str().unwrap())).unwrap();
    std::fs::set_permissions(&collector, std::fs::Permissions::from_mode(0o755)).unwrap();
    let connections = root.join("connections.json");
    write(
        &connections,
        &json!({"schema":"eal.evidence-config/1","connections":[],"approved_catalogues":[],"local_catalogues":[catalogue]}),
    );
    let config: PlanningConfig = serde_json::from_value(json!({
        "schema":engarg::route_planning::PLANNING_CONFIG_SCHEMA,"collector":collector,
        "connections":connections,"store":root.join("store"),"environment":["REPLAN_SINGLE_TOKEN"],
        "ranking":{"dimensions":["execution_requests"]},"scheduling":{"maximum_concurrent_sources":1,"per_provider":{}},"bindings":[]
    })).unwrap();
    let initial = engarg::route_planning::compile(
        &case,
        &elaboration,
        &catalogue,
        &config,
        &DiscoverySnapshot::default(),
    )
    .unwrap();
    assert_eq!(initial.gaps.len(), 1);
    assert!(matches!(
        initial.gaps[0].kind,
        engarg::route_planning::GapKind::Configuration
    ));
    let original = current_report(
        json!({"request":request,"argument":case,"evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":initial}}),
    );
    let config_path = root.join("planning.json");
    let input = root.join("original.json");
    write(&config_path, &config);
    write(&input, &original);
    let original_support: SupportElaboration =
        serde_json::from_value(original["evidence_planning"]["elaboration"].clone()).unwrap();
    let expected_review = serde_json::to_value(
        engarg::support_requirements::review_acquisition_design(
            &original_support,
            &[],
            engarg::authoring_checkpoint::MAX_REPORT_BYTES,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(expected_review["status"], "represented");
    assert_eq!(expected_review["active_acquisitions"], 1);
    assert_eq!(expected_review["pending_designs"], json!([]));
    let mut previous = input;
    for (name, credential) in [("missing", None), ("configured", Some("synthetic-token"))] {
        let output = root.join(format!("{name}.json"));
        let mut command = cli();
        command
            .args(["eal", "replan", "--input"])
            .arg(&previous)
            .arg("--evidence-config")
            .arg(&config_path)
            .arg("--output")
            .arg(&output)
            .env_remove("REPLAN_SINGLE_TOKEN");
        if let Some(credential) = credential {
            command.env("REPLAN_SINGLE_TOKEN", credential);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let updated: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        for key in ["request", "argument", "transcript"] {
            assert_eq!(updated[key], original[key]);
        }
        assert_eq!(updated["transcript"], json!([]));
        // Replanning records the recomputed source-design review. Representation
        // of the unchanged acquisition is not an accepted observation.
        assert_eq!(
            updated["evidence_planning"]["design_review"],
            expected_review
        );
        assert_eq!(
            updated["evidence_planning"]["elaboration"],
            original["evidence_planning"]["elaboration"]
        );
        assert_eq!(updated["replanning"]["model_calls"], 0);
        let routes = &updated["evidence_planning"]["routes"];
        assert_eq!(
            routes["gaps"].as_array().unwrap().len(),
            usize::from(credential.is_none())
        );
        assert_eq!(
            routes["discovery"]["gaps"].as_array().unwrap().len(),
            usize::from(credential.is_none())
        );
        assert_eq!(
            routes["execution"]["acquisitions"][0]["availability"]["kind"],
            if credential.is_some() {
                "configured"
            } else {
                "configuration_missing"
            }
        );
        assert_eq!(updated["argument"]["status"], "unsupported");
        report::read_direct(&updated).unwrap();
        assert!(
            !std::fs::read_to_string(&output)
                .unwrap()
                .contains("synthetic-token")
        );
        previous = output;
    }
    assert_eq!(
        std::fs::read_to_string(log).unwrap(),
        "discovery\ndiscovery\n"
    );
    assert!(
        !root.join("store").exists(),
        "replanning must not acquire evidence"
    );
}

#[test]
fn operator_freshness_changes_only_selected_missing_bounds_and_rejects_overrides() {
    let (_, _, mut elaboration, _, _) = support_fixture::fixture();
    let requirement = elaboration
        .requirements
        .iter_mut()
        .find(|r| r.id == "shared_requirement_0")
        .unwrap();
    requirement.contract.as_mut().unwrap().time.max_age_seconds = None;
    elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == "shared_requirement_0")
        .unwrap()
        .max_age_seconds = None;
    let original = serde_json::to_value(&elaboration).unwrap();
    let clarification: report::freshness::FreshnessClarification = serde_json::from_value(json!({
        "schema":"engarg.freshness-clarification/1",
        "rationale":"Operator explicitly authorised this age for the selected fixture.",
        "requirements":{"shared_requirement_0":3600}
    }))
    .unwrap();
    assert!(
        engarg::support_requirements::validate_freshness(&elaboration)
            .iter()
            .any(|e| e.contains("shared_requirement_0"))
    );
    report::freshness::apply(&mut elaboration, &clarification).unwrap();
    let after = serde_json::to_value(&elaboration).unwrap();
    assert_eq!(after["acquisitions"], original["acquisitions"]);
    let requirement = elaboration
        .requirements
        .iter()
        .find(|r| r.id == "shared_requirement_0")
        .unwrap();
    assert_eq!(
        requirement.contract.as_ref().unwrap().time.max_age_seconds,
        Some(3600)
    );
    assert_eq!(
        elaboration
            .evaluations
            .iter()
            .find(|e| e.requirement_id == requirement.id)
            .unwrap()
            .max_age_seconds,
        Some(3600)
    );
    assert_eq!(
        elaboration
            .requirements
            .iter()
            .find(|r| r.id == "shared_requirement_1")
            .unwrap()
            .contract
            .as_ref()
            .unwrap()
            .time
            .max_age_seconds,
        Some(300)
    );
    assert_eq!(
        after["evaluations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "evaluation_retained_judgement"),
        original["evaluations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "evaluation_retained_judgement")
    );
    let before_rejection = serde_json::to_value(&elaboration).unwrap();
    assert!(report::freshness::apply(&mut elaboration, &clarification).is_err());
    assert_eq!(
        serde_json::to_value(&elaboration).unwrap(),
        before_rejection
    );
    let mut unknown = clarification.clone();
    unknown.requirements = [("unknown".into(), 3600)].into();
    assert!(report::freshness::apply(&mut elaboration, &unknown).is_err());
    let mut invalid_age = clarification;
    invalid_age.requirements = [("shared_requirement_1".into(), 0)].into();
    assert!(report::freshness::apply(&mut elaboration, &invalid_age).is_err());
}

#[test]
fn freshness_check_rejects_mismatched_evaluation_and_requirement() {
    let (_, _, mut elaboration, _, _) = support_fixture::fixture();
    let evaluation = elaboration
        .evaluations
        .iter_mut()
        .find(|e| e.requirement_id == "shared_requirement_0")
        .unwrap();
    evaluation.max_age_seconds = Some(30);
    let errors = engarg::support_requirements::validate_freshness(&elaboration);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("does not match requirement shared_requirement_0"))
    );
}
