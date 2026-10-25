use super::*;
use eal_collector_contracts::execution::{DependencyBinding, ForEachBinding, OutputReference};
use serde_json::json;
use std::collections::BTreeMap;

fn fixture() -> (AcquisitionBlueprint, CapabilityDescriptor) {
    let contract = serde_json::from_value(json!({
        "schema":SUPPORT_CONTRACT_SCHEMA,"support_type":"observation",
        "scope":{"kind":"trace","subject":"Trace for a fixed deployment","selectors":{"revision":"r1"},"identity_dimensions":["revision","trace_id"]},
        "acceptable_methods":["trace-read/1"],"quantities":[],"logical_strength":"existential_witness","population":null,
        "time":{"historical_interval":null,"max_age_seconds":60,"max_uncertainty_seconds":null,"require_original_observation_time":true},
        "provenance":{"required_authorities":[],"transformation_history":false},"expected_output_schema_sha256":null
    })).unwrap();
    let input = json!({"type":"object","properties":{"revision":{"type":"string"},"trace_id":{"type":"string"},"count":{"type":"integer"}},"required":["revision","trace_id","count"],"additionalProperties":false});
    let output = json!({"type":"object","properties":{"present":{"type":"boolean"}},"required":["present"],"additionalProperties":false});
    let descriptor = serde_json::from_value(json!({
        "id":"trace_read","version":"1","description":"Read an identified trace",
        "provider":{"id":"trace_service","version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"public","configuration_sha256":"a".repeat(64)},
        "input_schema":input,"input_schema_sha256":schema_identity(&input).unwrap(),"output_schema":output,"output_schema_sha256":schema_identity(&output).unwrap(),
        "semantics":{"qualification_reference":"operator.trace-read/1","support_types":["observation"],"subject_kinds":["trace"],"methods":["trace-read/1"],"identity_dimensions":["revision","trace_id"],"scope_constraints":{},"scope_bindings":{"revision":"/revision","trace_id":"/trace_id"},"quantities":[],"coverage":[{"strength":"existential_witness","population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":true,"uncertainty_seconds":0},"provenance":{"authorities":[],"transformation_history":false},"limitations":[]},
        "configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},"costs":CapabilityCosts::unknown("Not measured")
    })).unwrap();
    let blueprint = AcquisitionBlueprint {
        operation_spec: None,
        id: "read".into(),
        contract,
        arguments: json!({"revision":"r1","trace_id":null,"count":3}),
        bindings: vec![DependencyBinding {
            output: OutputReference {
                step_id: "search".into(),
                pointer: "/trace_id".into(),
            },
            argument_pointer: "/trace_id".into(),
        }],
        for_each: None,
    };
    (blueprint, descriptor)
}

#[test]
fn declared_dependencies_defer_only_their_own_schema_and_identity_slots() {
    let (blueprint, descriptor) = fixture();
    let (arguments, gaps) = materialise(&blueprint, &descriptor, None).unwrap();
    assert!(gaps.is_empty(), "{gaps:?}");
    assert_eq!(arguments["trace_id"], Value::Null);
    let mut malformed = blueprint.clone();
    malformed.arguments["count"] = json!("three");
    let (_, gaps) = materialise(&malformed, &descriptor, None).unwrap();
    assert!(gaps.iter().any(|gap| gap.contains("/count")));
    malformed = blueprint.clone();
    malformed.bindings.clear();
    let (_, gaps) = materialise(&malformed, &descriptor, None).unwrap();
    assert!(gaps.iter().any(|gap| gap.contains("trace_id")));
}

#[test]
fn deferred_slots_follow_operator_physical_mapping_and_bounded_fanout() {
    let (mut blueprint, mut descriptor) = fixture();
    let schema = json!({"type":"object","properties":{"revision":{"type":"string"},"request":{"type":"object","properties":{"trace":{"type":"string"}},"required":["trace"],"additionalProperties":false},"count":{"type":"integer"}},"required":["revision","request","count"],"additionalProperties":false});
    descriptor.input_schema_sha256 = schema_identity(&schema).unwrap();
    descriptor.input_schema = schema;
    descriptor
        .semantics
        .as_mut()
        .unwrap()
        .scope_bindings
        .insert("trace_id".into(), "/request/trace".into());
    let binding = CapabilityInputBinding {
        capability_id: descriptor.id.clone(),
        arguments: json!({"revision":"r1","request":{"trace":null},"count":3}),
        input_bindings: BTreeMap::from([("/request/trace".into(), "/trace_id".into())]),
        configured_prerequisites: vec![],
    };
    let (_, gaps) = materialise(&blueprint, &descriptor, Some(&binding)).unwrap();
    assert!(gaps.is_empty(), "{gaps:?}");
    assert_eq!(
        physical_dependencies(&blueprint, Some(&binding)).0[0].argument_pointer,
        "/request/trace"
    );
    let mut absent_parent = binding.clone();
    absent_parent
        .arguments
        .as_object_mut()
        .unwrap()
        .remove("request");
    let (arguments, gaps) = materialise(&blueprint, &descriptor, Some(&absent_parent)).unwrap();
    assert!(gaps.is_empty(), "{gaps:?}");
    assert_eq!(arguments.pointer("/request/trace"), Some(&Value::Null));
    let mut conflict = binding.clone();
    conflict.arguments["request"] = json!("operator scalar");
    let (_, gaps) = materialise(&blueprint, &descriptor, Some(&conflict)).unwrap();
    assert!(gaps.iter().any(|gap| gap.contains("parent")));
    blueprint.bindings.clear();
    blueprint.for_each = Some(ForEachBinding {
        output: OutputReference {
            step_id: "search".into(),
            pointer: "/ids".into(),
        },
        argument_pointer: "/trace_id".into(),
        maximum_items: 8,
        stop_on_witness: None,
    });
    let (_, gaps) = materialise(&blueprint, &descriptor, Some(&binding)).unwrap();
    assert!(gaps.is_empty(), "{gaps:?}");
    assert_eq!(
        physical_dependencies(&blueprint, Some(&binding))
            .1
            .unwrap()
            .argument_pointer,
        "/request/trace"
    );
}

#[test]
fn pointer_preparation_never_resizes_operator_arrays() {
    let mut input = json!({"entries":[]});
    assert!(set_pointer(&mut input, "/entries/0/id", json!("trace")).is_err());
    assert_eq!(input, json!({"entries":[]}));
}

#[test]
fn dynamic_bindings_cannot_overwrite_fixed_scope_or_create_undeclared_properties() {
    let (mut blueprint, descriptor) = fixture();
    blueprint.bindings[0].argument_pointer = "/revision".into();
    let (_, gaps) = materialise(&blueprint, &descriptor, None).unwrap();
    assert!(
        gaps.iter()
            .any(|gap| gap.contains("replace fixed scope selector revision"))
    );
    blueprint.bindings[0].argument_pointer = "/unexpected".into();
    let (_, gaps) = materialise(&blueprint, &descriptor, None).unwrap();
    assert!(
        gaps.iter()
            .any(|gap| gap.contains("Static capability input"))
    );
}

#[test]
fn dynamic_history_cannot_replace_the_original_query_interval() {
    let (mut blueprint, mut descriptor) = fixture();
    blueprint.contract.time.historical_interval = Some(HistoricalInterval {
        start_unix: 0,
        end_unix: 60,
    });
    blueprint.arguments["start"] = json!(0);
    blueprint.arguments["end"] = json!(60);
    blueprint.bindings.push(DependencyBinding {
        output: OutputReference {
            step_id: "window".into(),
            pointer: "/start".into(),
        },
        argument_pointer: "/start".into(),
    });
    descriptor.input_schema["properties"]["start"] = json!({"type":"integer"});
    descriptor.input_schema["properties"]["end"] = json!({"type":"integer"});
    descriptor.input_schema_sha256 = schema_identity(&descriptor.input_schema).unwrap();
    let time = &mut descriptor.semantics.as_mut().unwrap().time;
    time.historical_intervals = true;
    time.historical_input = Some(HistoricalInputBinding {
        start_pointer: "/start".into(),
        end_pointer: "/end".into(),
        encoding: HistoricalTimeEncoding::UnixSeconds,
    });
    let (_, gaps) = materialise(&blueprint, &descriptor, None).unwrap();
    assert!(
        gaps.iter()
            .any(|gap| gap.contains("replace the fixed historical"))
    );
}
