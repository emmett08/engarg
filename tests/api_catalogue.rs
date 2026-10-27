//! API catalogues keep optional structured inputs and native measurements portable.
use engarg::{
    acquisition::{self, AcquisitionPlan, ToolArgument},
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request},
};
use serde_json::{Value, json};

fn case() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}
fn plans(case: &ArgumentCase, first: AcquisitionPlan) -> Vec<AcquisitionPlan> {
    case.evidence_plan
        .iter()
        .enumerate()
        .map(|(index, need)| {
            if index == 0 {
                first.clone()
            } else {
                AcquisitionPlan {
                    input_preparation: None,
                    need_id: need.id.clone(),
                    tool_id: "custom".into(),
                    arguments: vec![],
                    unresolved_arguments: vec!["adapter".into()],
                }
            }
        })
        .collect()
}
fn arg(name: &str, value: &str) -> ToolArgument {
    ToolArgument {
        name: name.into(),
        value: value.into(),
    }
}
fn openapi_catalogue() -> CollectorCatalogue {
    CollectorCatalogue::from_json(&serde_json::to_vec(&json!({"schema":"eal.collector-catalogue/1","acquisition_contract":"eal.acquisition/1","collectors":[{
        "id":"openapi.fixture","version":"2","purpose":"Acquire the approved API operation",
        "parameters":["path","query","body","scope"],
        "input_schema":{"type":"object","additionalProperties":false,"required":["path"],"properties":{
            "path":{"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":{"type":"integer"}}},
            "query":{"type":"object","additionalProperties":false,"properties":{"details":{"type":"boolean"}}},
            "body":{"type":["object","null"]}}},
        "result_schema":{"type":"object","required":["collected"],"properties":{"collected":{"type":"boolean"},"observation":{"type":"object"}}},
        "command":{"argv":["{operator.executable}","openapi","--scope","{scope}","--config","{operator.config}"],"inherit_env":["API_TOKEN"],"timeout_seconds":30,"max_output_bytes":1048576,"parallel_safe":true},
        "acquisition_predicate":"collected == true","constraints":[]
    }]})).unwrap()).unwrap()
}
#[test]
fn optional_inputs_remain_absent_and_command_parameters_remain_required() {
    let (_, case) = case();
    let catalogue = openapi_catalogue();
    let first = AcquisitionPlan {
        input_preparation: None,
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "openapi.fixture".into(),
        arguments: vec![arg("path", "{\"id\":7}"), arg("scope", "production")],
        unresolved_arguments: vec![],
    };
    let mut planned = plans(&case, first);
    assert!(acquisition::validate_with_catalogue(&case, &planned, Some(&catalogue)).is_empty());
    let descriptor = catalogue.get("openapi.fixture").unwrap();
    assert_eq!(
        descriptor.input(&planned[0]).unwrap(),
        json!({"path":{"id":7}})
    );
    let binding = descriptor.binding(&planned[0]).unwrap();
    assert_eq!(binding.argv[0], "/REPLACE_EXECUTABLE");
    assert_eq!(binding.argv[3], "production");
    planned[0].arguments.retain(|arg| arg.name != "scope");
    assert!(!acquisition::validate_with_catalogue(&case, &planned, Some(&catalogue)).is_empty());
}
#[test]
fn explicit_null_and_unresolved_optional_values_preserve_typed_inputs() {
    let (_, case) = case();
    let catalogue = openapi_catalogue();
    for (optional, unresolved) in [
        (vec![arg("body", "null")], vec!["query".into()]),
        (vec![], vec![]),
    ] {
        let mut arguments = vec![arg("path", "{\"id\":7}"), arg("scope", "production")];
        arguments.extend(optional);
        let planned = plans(
            &case,
            AcquisitionPlan {
                input_preparation: None,
                need_id: case.evidence_plan[0].id.clone(),
                tool_id: "openapi.fixture".into(),
                arguments,
                unresolved_arguments: unresolved,
            },
        );
        assert!(acquisition::validate_with_catalogue(&case, &planned, Some(&catalogue)).is_empty());
        let tool = catalogue.get("openapi.fixture").unwrap();
        let input = tool.input(&planned[0]).unwrap();
        if !planned[0].unresolved_arguments.is_empty() {
            assert_eq!(input["body"], Value::Null);
            assert!(input.get("query").is_some());
        } else {
            assert!(input.get("query").is_none());
            assert!(input.get("body").is_none());
        }
    }
}
#[test]
fn planning_view_exposes_measurement_schemas_and_required_fields_without_execution_details() {
    let catalogue = openapi_catalogue();
    let view = catalogue.planning_view();
    let descriptor = &view[0];
    assert_eq!(descriptor["required_arguments"], json!(["path", "scope"]));
    assert_eq!(descriptor["result_schema"]["type"], "object");
    let text = view.to_string();
    assert!(!text.contains("API_TOKEN"));
    assert!(!text.contains("operator.executable"));
    assert!(!text.contains("--config"));
}
#[test]
fn third_native_catalogue_preserves_typed_telemetry_and_delivery_plans() {
    let catalogue =
        CollectorCatalogue::from_json(include_bytes!("../contracts/collectors/native-v3.json"))
            .unwrap();
    let (_, case) = case();
    for (id, arguments) in [
        (
            "datadog_logs",
            vec![
                arg("query", "service:api env:prod"),
                arg("start_unix", "100"),
                arg("end_unix", "200"),
            ],
        ),
        (
            "prometheus_range",
            vec![
                arg("query", "sum(rate(errors_total[5m]))"),
                arg("start_unix", "100"),
                arg("end_unix", "200"),
            ],
        ),
        (
            "github_workflow",
            vec![
                arg("repository", "owner/repo"),
                arg("commit", &"a".repeat(40)),
                arg("run_id", "17"),
                arg("attempt", "2"),
            ],
        ),
        (
            "report",
            vec![
                arg("revision", &"a".repeat(40)),
                arg("report_sha256", &"b".repeat(64)),
            ],
        ),
    ] {
        let plan = AcquisitionPlan {
            input_preparation: None,
            need_id: case.evidence_plan[0].id.clone(),
            tool_id: id.into(),
            arguments,
            unresolved_arguments: vec![],
        };
        let input = catalogue.get(id).unwrap().input(&plan).unwrap();
        assert!(input.get("start_unix").is_none_or(|v| v.is_u64()));
        assert!(
            acquisition::validate_with_catalogue(
                &case,
                &plans(&case, plan.clone()),
                Some(&catalogue)
            )
            .is_empty()
        );
        let binding = catalogue.get(id).unwrap().binding(&plan).unwrap();
        assert!(!binding.argv.is_empty());
    }
}
