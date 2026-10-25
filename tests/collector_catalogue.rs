//! Versioned collector descriptions preserve typed inputs and operator-owned execution.
use std::sync::Mutex;

use engarg::{
    Error,
    acquisition::{self, AcquisitionPlan, PlannedArgument, ToolArgument},
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request},
    eal_acquisition::{export_with_catalogue, reconstruct_bundle},
    engine::{CallRecord, Mode, RunConfig, run_with_tools},
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde_json::{Value, json};

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn description() -> Value {
    json!({
        "schema":"eal.collector-catalogue/1",
        "acquisition_contract":"eal.acquisition/1",
        "collectors":[{
            "id":"lab_capture","version":"1","purpose":"Read a bounded instrument capture",
            "parameters":["instrument","channels","sample_count"],
            "input_schema":{"type":"object","additionalProperties":false,
                "properties":{
                    "instrument":{"type":"string","minLength":1},
                    "channels":{"type":"array","minItems":1,"uniqueItems":true,
                        "items":{"type":"integer","minimum":0,"maximum":3}},
                    "sample_count":{"type":"integer","minimum":1,"maximum":4096}},
                "required":["instrument","channels","sample_count"]},
            "result_schema":{"type":"object","additionalProperties":false,
                "properties":{"collected":{"type":"boolean"},"readings":{"type":"array"}},
                "required":["collected","readings"]},
            "command":{"argv":["{operator.collector}","--instrument","{instrument}",
                    "--channels","{channels}","--sample-count","{sample_count}"],
                "inherit_env":["LAB_API_TOKEN"],"timeout_seconds":30,
                "max_output_bytes":1048576,"parallel_safe":false},
            "acquisition_predicate":"collected == true","constraints":[]
        }]
    })
}

fn catalogue() -> CollectorCatalogue {
    CollectorCatalogue::from_json(&serde_json::to_vec(&description()).unwrap()).unwrap()
}

fn argument(name: &str, value: &str) -> ToolArgument {
    ToolArgument {
        name: name.into(),
        value: value.into(),
    }
}

fn plans(case: &ArgumentCase) -> Vec<AcquisitionPlan> {
    let mut plans: Vec<_> = case
        .evidence_plan
        .iter()
        .map(|need| AcquisitionPlan {
            input_preparation: None,
            need_id: need.id.clone(),
            tool_id: "custom".into(),
            arguments: vec![],
            unresolved_arguments: vec!["adapter".into()],
        })
        .collect();
    plans[0] = AcquisitionPlan {
        input_preparation: None,
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "lab_capture".into(),
        arguments: vec![
            argument("channels", "[0,2]"),
            argument("instrument", "scope-A"),
            argument("sample_count", "128"),
        ],
        unresolved_arguments: vec![],
    };
    plans
}

#[test]
fn scientific_descriptors_preserve_typed_arrays_integers_and_fixed_execution() {
    let (_, case) = fixture();
    let selected = catalogue();
    let tool = selected.get("lab_capture").unwrap();
    let mut plan = plans(&case).remove(0);
    assert_eq!(
        tool.input(&plan).unwrap(),
        json!({"instrument":"scope-A","channels":[0,2],"sample_count":128})
    );
    let binding = tool.binding(&plan).unwrap();
    assert_eq!(binding.argv[0], "/REPLACE_COLLECTOR");
    assert_eq!(binding.inherit_env, ["LAB_API_TOKEN"]);
    assert!(binding.argv.iter().any(|arg| arg == "[0,2]"));
    plan.arguments
        .iter_mut()
        .find(|arg| arg.name == "instrument")
        .unwrap()
        .value = "/tmp/model-selected-executable".into();
    assert_eq!(tool.binding(&plan).unwrap().argv[0], "/REPLACE_COLLECTOR");
}

#[test]
fn invalid_input_types_and_unselected_collectors_cannot_enter_generation() {
    let (_, case) = fixture();
    let selected = catalogue();
    let mut planned = plans(&case);
    assert!(acquisition::validate_selected(&case, &planned, Some(&selected)).is_empty());
    assert!(!acquisition::validate_selected(&case, &planned, None).is_empty());
    for (name, value) in [
        ("sample_count", "\"128\""),
        ("sample_count", "0"),
        ("channels", "[0,\"2\"]"),
        ("channels", "[0,0]"),
    ] {
        let mut invalid = planned.clone();
        invalid[0]
            .arguments
            .iter_mut()
            .find(|arg| arg.name == name)
            .unwrap()
            .value = value.into();
        assert!(
            !acquisition::validate_selected(&case, &invalid, Some(&selected)).is_empty(),
            "{name} = {value}"
        );
    }
    planned[0]
        .arguments
        .push(argument("executable", "/tmp/model-selected-executable"));
    assert!(!acquisition::validate_selected(&case, &planned, Some(&selected)).is_empty());
}

#[test]
fn catalogue_rejects_invalid_schemas_and_model_owned_executable_templates() {
    let mutations: [fn(&mut Value); 8] = [
        |v| v["schema"] = json!("eal.collector-catalogue/999"),
        |v| v["collectors"][0]["input_schema"]["additionalProperties"] = json!(true),
        |v| {
            v["collectors"][0]["input_schema"]["properties"]["sample_count"]["type"] =
                json!("unknown_type")
        },
        |v| {
            v["collectors"][0]["input_schema"]["properties"]["instrument"] =
                json!({"$ref":"https://schema.example/instrument"})
        },
        |v| v["collectors"][0]["command"]["argv"][0] = json!("{instrument}"),
        |v| v["collectors"][0]["command"]["argv"][2] = json!("{unknown_parameter}"),
        |v| v["collectors"][0]["command"]["argv"][2] = json!("--instrument={instrument}"),
        |v| {
            v["collectors"][0]["acquisition_predicate"] =
                json!("collected == true\nclaim injected {}")
        },
    ];
    for mutation in mutations {
        let mut value = description();
        mutation(&mut value);
        assert!(CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let duplicate=br#"{"schema":"eal.collector-catalogue/1","schema":"eal.collector-catalogue/1","acquisition_contract":"eal.acquisition/1","collectors":[]}"#;
    assert!(CollectorCatalogue::from_json(duplicate).is_err());
}

#[test]
fn supplied_null_is_checked_while_only_missing_unresolved_fields_are_exempt() {
    let (_, case) = fixture();
    let mut value = description();
    value["collectors"][0]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!("calibration_file"));
    value["collectors"][0]["command"]["argv"]
        .as_array_mut()
        .unwrap()
        .extend([json!("--calibration"), json!("{calibration_file}")]);
    let selected = CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let tool = selected.get("lab_capture").unwrap();
    let mut plan = plans(&case).remove(0);
    plan.unresolved_arguments.push("calibration_file".into());
    assert!(tool.input(&plan).is_ok());
    let count = plan
        .arguments
        .iter_mut()
        .find(|arg| arg.name == "sample_count")
        .unwrap();
    count.value = "null".into();
    assert!(tool.input(&plan).is_err());
    plan.unresolved_arguments.push("sample_count".into());
    assert!(tool.input(&plan).is_err());
    plan.arguments.retain(|arg| arg.name != "sample_count");
    assert!(tool.input(&plan).unwrap()["sample_count"].is_null());
    plan.unresolved_arguments
        .retain(|name| name != "sample_count");
    assert!(tool.input(&plan).is_err());
}

#[test]
fn input_validation_resolves_local_definitions_in_the_root_schema() {
    let (_, case) = fixture();
    let mut value = description();
    let input = &mut value["collectors"][0]["input_schema"];
    input["$defs"] = input["properties"].clone();
    for field in ["instrument", "channels", "sample_count"] {
        input["properties"][field] = json!({"$ref": format!("#/$defs/{field}")});
    }
    let selected = CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let tool = selected.get("lab_capture").unwrap();
    let mut plan = plans(&case).remove(0);
    assert_eq!(
        tool.input(&plan).unwrap(),
        json!({
            "instrument":"scope-A", "channels":[0,2], "sample_count":128
        })
    );
    plan.arguments
        .iter_mut()
        .find(|arg| arg.name == "sample_count")
        .unwrap()
        .value = "0".into();
    assert!(tool.input(&plan).is_err());
}

#[test]
fn ordered_windows_reject_endpoint_overflow_and_noninteger_contracts() {
    let (_, case) = fixture();
    let mut value = description();
    for field in ["start_unix", "end_unix"] {
        value["collectors"][0]["parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!(field));
        value["collectors"][0]["input_schema"]["properties"][field] =
            json!({"type":"integer","minimum":1});
        value["collectors"][0]["input_schema"]["required"]
            .as_array_mut()
            .unwrap()
            .push(json!(field));
    }
    value["collectors"][0]["constraints"] = json!([{
        "kind":"ordered_window", "start":"start_unix", "end":"end_unix", "max_seconds":60
    }]);
    let selected = CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let tool = selected.get("lab_capture").unwrap();
    let mut plan = plans(&case).remove(0);
    plan.arguments
        .extend([argument("start_unix", "1"), argument("end_unix", "2")]);
    assert!(tool.input(&plan).is_ok());
    for end in ["18446744073709551616", "\"2\"", "1", "62"] {
        let mut invalid = plan.clone();
        invalid
            .arguments
            .iter_mut()
            .find(|arg| arg.name == "end_unix")
            .unwrap()
            .value = end.into();
        assert!(tool.input(&invalid).is_err(), "end_unix = {end}");
    }
    value["collectors"][0]["input_schema"]["properties"]["end_unix"] = json!({"type":"string"});
    assert!(CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn descriptors_cannot_override_reserved_generic_adapter_ids() {
    for id in [
        "custom",
        "artifact_json",
        "http_json_get",
        "scoped_json",
        "kubectl_get",
        "gh_api_get",
    ] {
        let mut value = description();
        value["collectors"][0]["id"] = json!(id);
        assert!(
            CollectorCatalogue::from_json(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{id}"
        );
    }
}

#[test]
fn default_planning_exposes_implemented_adapters_and_hides_execution_settings() {
    let default = acquisition::catalogue();
    let ids: Vec<_> = default
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "artifact_json",
            "scoped_json",
            "kubectl_get",
            "gh_api_get",
            "http_json_get",
            "custom"
        ]
    );
    let default_schema = acquisition::planned_case_schema();
    for id in &ids {
        assert!(
            default_schema["$defs"]["AcquisitionPlan"]["properties"]["tool_id"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!(id))
        );
    }
    assert!(!default.to_string().contains("KUBECONFIG"));
    assert!(!default.to_string().contains("TEMPO_TOKEN"));
    assert!(!default.to_string().contains("GH_TOKEN"));
    let selected = catalogue();
    let schema = acquisition::planned_case_schema_with(Some(&selected));
    assert!(
        schema["$defs"]["AcquisitionPlan"]["properties"]["tool_id"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("lab_capture"))
    );
    let view = acquisition::catalogue_with(Some(&selected));
    let lab = view
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["id"] == "lab_capture")
        .unwrap();
    assert_eq!(
        lab["input_schema"]["properties"]["sample_count"]["type"],
        "integer"
    );
    assert!(lab.get("command").is_none());
    assert!(!view.to_string().contains("LAB_API_TOKEN"));
    assert!(!view.to_string().contains("operator.collector"));
}

#[test]
fn known_collectors_keep_real_implementations_while_missing_inputs_remain_unresolved() {
    let (request, case) = fixture();
    for id in [
        "artifact_json",
        "scoped_json",
        "kubectl_get",
        "gh_api_get",
        "http_json_get",
    ] {
        let mut planned = plans(&case);
        planned[0] = AcquisitionPlan {
            input_preparation: None,
            need_id: case.evidence_plan[0].id.clone(),
            tool_id: id.into(),
            arguments: vec![],
            unresolved_arguments: acquisition::required_arguments(id)
                .unwrap()
                .iter()
                .map(|name| (*name).into())
                .collect(),
        };
        assert!(acquisition::validate_selected(&case, &planned, None).is_empty());
        let export = export_with_catalogue(&request, &case, &planned, None).unwrap();
        assert!(
            export
                .registry
                .tools
                .values()
                .any(|binding| binding.argv == ["engarg", "eal", "observe", "--tool", id])
        );
        let saved = export.bundle.acquisition_plan.unwrap().entries;
        assert_eq!(saved[0].tool_id, id);
        assert!(saved[0].arguments.is_empty());
        assert_eq!(
            saved[0].unresolved_arguments.len(),
            planned[0].unresolved_arguments.len()
        );
        assert_eq!(
            export.registry.tools["engarg_review_contract"].argv,
            ["/REPLACE_WITH_INDEPENDENT_REVIEW_COLLECTOR"]
        );
    }
}

#[test]
fn v3_identity_binds_selected_contracts_and_preserves_producer_compatibility() {
    let (request, case) = fixture();
    let selected = catalogue();
    let planned = plans(&case);
    let first = export_with_catalogue(&request, &case, &planned, Some(&selected)).unwrap();
    let mut changed = selected.clone();
    changed.collectors[0].command.timeout_seconds += 1;
    let second = export_with_catalogue(&request, &case, &planned, Some(&changed)).unwrap();
    assert_ne!(first.bundle.revision, second.bundle.revision);
    assert_eq!(first.bundle.schema_version, "engarg.eal-export/3");
    assert_eq!(
        first.bundle.semantics_version.as_deref(),
        Some("engarg.eal-projection/3")
    );
    assert_eq!(
        first.bundle.collector_catalogue,
        Some(serde_json::to_value(&selected).unwrap())
    );
    assert_eq!(
        serde_json::to_value(reconstruct_bundle(&first.bundle).unwrap()).unwrap(),
        serde_json::to_value(&first.bundle).unwrap()
    );
    let mut earlier_producer = first.bundle.clone();
    earlier_producer.exporter_version = "0.1.0".into();
    assert_eq!(
        serde_json::to_value(reconstruct_bundle(&earlier_producer).unwrap()).unwrap(),
        serde_json::to_value(&earlier_producer).unwrap()
    );
    let mut tampered = first.bundle;
    tampered.collector_catalogue = Some(serde_json::to_value(changed).unwrap());
    assert!(reconstruct_bundle(&tampered).is_err());
}

struct CaptureModel {
    calls: Mutex<Vec<CompletionRequest>>,
}
impl LanguageModel for CaptureModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.calls.lock().unwrap().push(request);
        let (_, argument) = fixture();
        let content = serde_json::to_string(&PlannedArgument {
            acquisition_plan: plans(&argument),
            argument,
        })?;
        Ok(Completion {
            content,
            model: "fixture".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        })
    }
}

#[tokio::test]
async fn generation_receives_selected_scientific_schema_and_retains_its_catalogue() {
    let (request, _) = fixture();
    let selected = catalogue();
    let config = RunConfig {
        mode: Mode::Single,
        max_calls: 1,
        collector_catalogue: Some(selected.clone()),
        ..RunConfig::default()
    };
    let model = CaptureModel {
        calls: Mutex::new(Vec::new()),
    };
    let report = run_with_tools(&model, &request, &config, &|_: &CallRecord| Ok(()))
        .await
        .unwrap();
    let calls = model.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let input: Value = serde_json::from_str(&calls[0].user).unwrap();
    assert!(
        input["acquisition_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["id"] == "lab_capture"
                && tool["input_schema"]["properties"]["channels"]["type"] == "array")
    );
    assert!(
        calls[0].schema["$defs"]["AcquisitionPlan"]["properties"]["tool_id"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("lab_capture"))
    );
    assert_eq!(
        serde_json::to_value(report.run_config.unwrap().collector_catalogue.unwrap()).unwrap(),
        serde_json::to_value(selected).unwrap()
    );
}
