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
    let (request, case, elaboration, _execution, routes, _locations) = support_fixture::fixture();
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
    let transcript = json!([{"role":"adjudicator","prompt":{"role":"adjudicator","system":"retained fixture","user":"{}","schema":{}},"response":{"content":response,"model":"original-model","input_tokens":7,"output_tokens":5,"request_id":"original-request","service_tier":null,"reasoning_effort":null},"response_content_bytes":response.len(),"response_content_sha256":"retained-fixture","response_content_omitted":false,"elapsed_ms":1}]);
    let original = json!({"request":request,"argument":case,"transcript":transcript,"tooling":null,"retained_numeric":serde_json::from_str::<Value>("0.123456789012345678901234567890123456789").unwrap(),"evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":initial}});
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
    for key in ["request", "argument", "transcript", "retained_numeric"] {
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
    let exported = directory.path().join("argument.json");
    let result = cli()
        .args(["eal", "export", "--format", "json", "--input"])
        .arg(&restored)
        .arg("--output")
        .arg(&exported)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let prepared = directory.path().join("prepared");
    let result = cli()
        .args(["eal", "prepare", "--input"])
        .arg(&restored)
        .arg("--output-dir")
        .arg(&prepared)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
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
    let (_, _, shared, _, routes, _) = support_fixture::fixture();
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
    let original = json!({"request":request,"argument":case,"transcript":[],"evidence_planning":{"schema":"engarg.support-planning/1","elaboration":elaboration,"routes":initial}});
    let config_path = root.join("planning.json");
    let input = root.join("original.json");
    write(&config_path, &config);
    write(&input, &original);
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
