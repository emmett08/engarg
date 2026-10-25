//! Existing batch interfaces consume a whole collection through exported EAL.
use eal_collector_contracts::{
    capability::{
        CapabilityDescriptor, LogicalStrength, SupportContract, SupportType, schema_identity,
    },
    execution::{
        AcquisitionStep, DependencyBinding, DerivationOperation, DerivationStep,
        EvaluationCriterion, OutputReference, RequiredCoverage, SourceIdentity, canonical_digest,
    },
};
use engarg::{
    eal_host::{HostConfig, invoke},
    export_policy::ExportPolicy,
    support_export::{self, AcquisitionRouteProof},
    support_requirements::AcquisitionBlueprint,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[path = "common/support.rs"]
mod support_fixture;

fn descriptor(
    id: &str,
    contract: &SupportContract,
    input: Value,
    pointer: &str,
) -> CapabilityDescriptor {
    let mut capability = support_fixture::descriptor();
    capability.id = id.into();
    capability.provider.id = id.into();
    capability.input_schema = input;
    capability.input_schema_sha256 = schema_identity(&capability.input_schema).unwrap();
    capability.output_schema = json!({"type":"object"});
    capability.output_schema_sha256 = schema_identity(&capability.output_schema).unwrap();
    let semantics = capability.semantics.as_mut().unwrap();
    semantics.subject_kinds = vec![contract.scope.kind.clone()];
    semantics.methods = contract.acceptable_methods.clone();
    semantics.quantities = contract
        .quantities
        .iter()
        .map(|quantity| {
            serde_json::from_value(
                json!({"name":quantity.name,"unit":quantity.unit,"output_pointer":pointer}),
            )
            .unwrap()
        })
        .collect();
    capability
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn write_record(path: &Path, measurement: Value, observed_at: &str) -> Value {
    let bytes =
        serde_json::to_vec(&json!({"measurement":measurement,"observed_at":observed_at})).unwrap();
    std::fs::write(path, &bytes).unwrap();
    json!({"kind":"retained_artifact","path":path,"sha256":digest(&bytes)})
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires matching native EAL binaries; set ENGARG_EAL_HOST"]
async fn existing_batch_identity_reader_receives_all_ids_once_and_serves_shared_requirements() {
    let directory = tempfile::tempdir().unwrap();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let observed_at = String::from_utf8(
        std::process::Command::new("date")
            .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let (request, case, mut elaboration, mut execution, _, mut locations) =
        support_fixture::fixture();
    let base = elaboration.acquisitions[0].contract.clone();
    let mut resource = base.clone();
    resource.scope.kind = "k8s_resource".into();
    resource.scope.subject = "The independently retained resource".into();
    resource.acceptable_methods = vec!["fixture.resource-uid/1".into()];
    resource.quantities[0].name = "resource_uid".into();
    resource.quantities[0].unit = "opaque_identity".into();
    let mut search = base.clone();
    search.scope.kind = "trace_search".into();
    search.scope.subject = "The independently retained search result".into();
    search.acceptable_methods = vec!["fixture.search/1".into()];
    search.quantities[0].name = "ids".into();
    search.quantities[0].unit = "trace_id".into();
    let mut batch = base;
    batch.scope.kind = "trace_batch".into();
    batch.scope.subject = "The trace identities fetched by one existing batch operation".into();
    batch.acceptable_methods = vec!["fixture.batch-trace-uids/1".into()];
    batch.quantities[0].name = "resource_uids".into();
    batch.quantities[0].unit = "opaque_identity".into();
    elaboration.acquisitions = vec![
        AcquisitionBlueprint {
            id: "resource".into(),
            contract: resource.clone(),
            arguments: json!({"case":"dev"}),
            bindings: vec![],
            for_each: None,
            operation_spec: None,
        },
        AcquisitionBlueprint {
            id: "search".into(),
            contract: search.clone(),
            arguments: json!({"case":"dev"}),
            bindings: vec![],
            for_each: None,
            operation_spec: None,
        },
        AcquisitionBlueprint {
            id: "batch".into(),
            contract: batch.clone(),
            arguments: json!({"case":"dev","trace_ids":null}),
            bindings: vec![DependencyBinding {
                output: OutputReference {
                    step_id: "search".into(),
                    pointer: "/ids".into(),
                },
                argument_pointer: "/trace_ids".into(),
            }],
            for_each: None,
            operation_spec: None,
        },
    ];
    elaboration.derivations = vec![DerivationStep {
        id: "membership".into(),
        inputs: vec![
            OutputReference {
                step_id: "resource".into(),
                pointer: "/resource_uid".into(),
            },
            OutputReference {
                step_id: "batch".into(),
                pointer: "/resource_uids".into(),
            },
        ],
        operation: DerivationOperation::AnyIdentityEqual,
    }];
    let mut relation = resource.clone();
    relation.support_type = SupportType::Calculation;
    relation.logical_strength = LogicalStrength::ExactCalculation;
    relation.scope.kind = "identity_relation".into();
    relation.scope.subject = "The resource identity occurs in the selected trace batch".into();
    relation.scope.selectors = std::collections::BTreeMap::from([
        ("case".into(), json!("dev")),
        (
            "operands".into(),
            json!({"resource":resource.scope,"batch":batch.scope}),
        ),
        (
            "operand_times".into(),
            json!({"resource":resource.time,"search":search.time,"batch":batch.time}),
        ),
    ]);
    relation.scope.identity_dimensions = vec!["case".into(), "operands".into()];
    relation.acceptable_methods = vec!["engarg.any-identity-equal/1".into()];
    relation.quantities[0].name = "any_identity_equal".into();
    relation.quantities[0].unit = "boolean".into();
    for requirement in elaboration
        .requirements
        .iter_mut()
        .filter(|requirement| requirement.id.starts_with("shared_requirement"))
    {
        requirement.contract = Some(relation.clone());
        requirement.acceptance = EvaluationCriterion::Equals {
            expected: json!(true),
        };
        let evaluation = elaboration
            .evaluations
            .iter_mut()
            .find(|evaluation| evaluation.requirement_id == requirement.id)
            .unwrap();
        evaluation.inputs = vec![OutputReference {
            step_id: "membership".into(),
            pointer: "".into(),
        }];
        evaluation.criterion = requirement.acceptance.clone();
        evaluation.required_coverage = RequiredCoverage::Witness;
    }
    let call_log = directory.path().join("batch-calls.jsonl");
    let script = directory.path().join("existing_batch_reader.py");
    std::fs::write(&script,"import json,sys\ninputs=json.load(sys.stdin)\nassert inputs == {'case':'dev','trace_ids':['trace-a','trace-b']}\nwith open(sys.argv[2], 'a') as log: log.write(json.dumps(inputs)+'\\n')\nprint(json.dumps({'measurement':{'case':'dev','traces':{'resource_uids':['other-resource','wanted-resource']}},'observed_at':sys.argv[1]}))\n").unwrap();
    let python = String::from_utf8(
        std::process::Command::new("python3")
            .args(["-c", "import sys;print(sys.executable)"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let python = std::fs::canonicalize(python.trim()).unwrap();
    let supervisor = host.with_file_name("eal-process-supervisor");
    let transports = [
        write_record(
            &directory.path().join("resource.json"),
            json!({"case":"dev","resource":{"uid":"wanted-resource"}}),
            &observed_at,
        ),
        write_record(
            &directory.path().join("search.json"),
            json!({"case":"dev","results":{"ids":["trace-a","trace-b"]}}),
            &observed_at,
        ),
        json!({"kind":"command","program":python,"supervisor":supervisor,"arguments":[script,observed_at,call_log],"cwd":directory.path(),"inherit_env":[],"protocol":{"kind":"raw_json"}}),
    ];
    let input = json!({"type":"object","properties":{"case":{"type":"string"}},"required":["case"],"additionalProperties":false});
    let batch_input = json!({"type":"object","properties":{"case":{"type":"string"},"trace_ids":{"type":"array","items":{"type":"string"}}},"required":["case","trace_ids"],"additionalProperties":false});
    let mut descriptors = [
        descriptor("resource_reader", &resource, input.clone(), "/resource/uid"),
        descriptor("search_reader", &search, input, "/results/ids"),
        descriptor(
            "existing_batch_reader",
            &batch,
            batch_input,
            "/traces/resource_uids",
        ),
    ];
    let mut providers = vec![];
    let mut routes = vec![];
    execution.acquisitions.clear();
    execution.derivations.clear();
    execution.evaluations = elaboration.evaluations.clone();
    for index in 0..3 {
        let capability = &mut descriptors[index];
        let transport = &transports[index];
        capability.provider.transport = if index == 2 {
            eal_collector_contracts::capability::TransportKind::Command
        } else {
            eal_collector_contracts::capability::TransportKind::RetainedArtifact
        };
        let mapping = json!({"tool":capability.id,"raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/measurement","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{"case":"/case"}});
        let pinned = if index == 2 {
            json!([{"path":script,"sha256":digest(&std::fs::read(&script).unwrap())}])
        } else {
            json!([])
        };
        capability.provider.configuration_sha256 =
            canonical_digest(&json!([transport, mapping, pinned, []])).unwrap();
        let source = SourceIdentity {
            provider_id: capability.provider.id.clone(),
            capability_id: capability.id.clone(),
            capability_version: capability.version.clone(),
            input_schema_sha256: capability.input_schema_sha256.clone(),
            output_schema_sha256: capability.output_schema_sha256.clone(),
            qualified_contract_sha256: capability.measurement_identity().unwrap(),
            configuration_sha256: capability.provider.configuration_sha256.clone(),
            authentication_scope_sha256: canonical_digest(
                &capability.provider.authentication_scope,
            )
            .unwrap(),
            scope: json!({"case":"dev"}),
            historical_interval: None,
        };
        let blueprint = &elaboration.acquisitions[index];
        execution.acquisitions.push(AcquisitionStep {
            id: blueprint.id.clone(),
            max_age_seconds: blueprint.contract.time.max_age_seconds,
            source,
            arguments: blueprint.arguments.clone(),
            bindings: blueprint.bindings.clone(),
            for_each: None,
            availability: Default::default(),
            reuse: Default::default(),
        });
        routes.push(AcquisitionRouteProof {
            blueprint_id: blueprint.id.clone(),
            capability: capability.clone(),
            projection_id: Some(format!("project_{}", blueprint.id)),
            input_bindings: Default::default(),
            operator_arguments: None,
        });
        providers.push(json!({"capability":capability,"transport":transport,"mapping":mapping,"pinned_files":pinned,"credential_env":[]}));
    }
    support_export::project_execution_graph(&elaboration, &routes, &mut execution).unwrap();
    assert!(
        execution
            .acquisitions
            .iter()
            .all(|acquisition| acquisition.for_each.is_none())
    );
    locations.collector = host.with_file_name("eal-collector");
    locations.connections = directory.path().join("connections.json");
    locations.execution_plan = directory.path().join("execution.json");
    locations.store = directory.path().join("source-store");
    std::fs::write(&locations.connections,serde_json::to_vec(&json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":providers,"local_catalogues":[],"approved_catalogues":[]})).unwrap()).unwrap();
    let artifacts = support_export::export(
        &request,
        &case,
        &elaboration,
        &execution,
        &routes,
        &locations,
        &ExportPolicy::default(),
    )
    .unwrap();
    std::fs::write(
        &locations.execution_plan,
        serde_json::to_vec(&artifacts.execution_plan).unwrap(),
    )
    .unwrap();
    let registry = directory.path().join("tools.toml");
    std::fs::write(&registry, &artifacts.tools_toml).unwrap();
    let config = HostConfig {
        executable: host,
        workspace: directory.path().into(),
        database: Some(directory.path().join("case.sqlite")),
        registry: Some(registry),
        collector_env: vec![],
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: std::time::Duration::from_secs(60),
    };
    let result=invoke(&config,&json!({"operation":"collect_claim","source":artifacts.bundle.source,"claim":artifacts.bundle.claim_names["current_claim"],"context":artifacts.bundle.context})).await.unwrap();
    assert_eq!(result["is_error"], false, "{result}");
    for projection in artifacts
        .bundle
        .requirements
        .iter()
        .filter(|projection| projection.requirement_id.starts_with("shared_requirement"))
    {
        let record = &result["result"]["records"][&projection.eal_evidence_name];
        assert_eq!(record["status"], "ok", "{record}");
        assert_eq!(record["value"]["criterion_satisfied"], true, "{record}");
        assert_eq!(record["value"]["source_ids"].as_array().unwrap().len(), 3);
    }
    let calls = std::fs::read_to_string(&call_log).unwrap();
    assert_eq!(
        calls.lines().count(),
        1,
        "the existing batch tool must run once for all shared requirements"
    );
    assert_eq!(
        serde_json::from_str::<Value>(calls.trim()).unwrap()["trace_ids"],
        json!(["trace-a", "trace-b"])
    );
}
