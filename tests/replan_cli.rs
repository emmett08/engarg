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
}
