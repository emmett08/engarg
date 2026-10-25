use super::*;
use crate::support_requirements::{
    SupportElaboration,
    tests::{case, pending},
};
use serde_json::json;

fn contract(kind: &str, name: &str, unit: &str) -> Value {
    json!({"schema":"eal.support-contract/1","support_type":if kind=="expression"{"calculation"}else{"observation"},
        "scope":{"kind":kind,"subject":"The explicitly selected values","selectors":{"revision":"r1"},"identity_dimensions":["revision"]},
        "acceptable_methods":[if kind=="expression"{"normalized-latency-ratio/1"}else{"retained.measurement/1"}],
        "quantities":[{"name":name,"unit":unit}],"logical_strength":if kind=="expression"{"exact_calculation"}else{"existential_witness"},"population":null,
        "time":{"historical_interval":null,"max_age_seconds":60,"max_uncertainty_seconds":null,"require_original_observation_time":true},
        "provenance":{"required_authorities":["original measurement authority"],"transformation_history":true},"expected_output_schema_sha256":null})
}

fn fixture() -> (AcquisitionBlueprint, SupportElaboration) {
    let mut elaboration = pending(&case());
    for (id, name, unit) in [
        ("latency_source", "latency", "ms"),
        ("budget_source", "budget", "s"),
    ] {
        elaboration.acquisitions.push(serde_json::from_value(json!({"id":id,"contract":contract("samples",name,unit),"arguments":{"revision":"r1"},"bindings":[],"for_each":null})).unwrap());
    }
    let blueprint=serde_json::from_value(json!({"id":"ratio","contract":contract("expression","ratio","1"),
        "arguments":{"latency":null,"budget":null,"revision":"r1"},
        "bindings":[{"output":{"step_id":"latency_source","pointer":"/latency"},"argument_pointer":"/latency"},{"output":{"step_id":"budget_source","pointer":"/budget"},"argument_pointer":"/budget"}],"for_each":null,
        "operation_spec":{"schema":"engarg.operation-specification/1",
            "definition":"Convert latency and budget to seconds using their declared units, then return latency divided by budget.",
            "input_conditions":["The budget must be positive; reject nonnumeric inputs rather than coercing them."],
            "output_guarantees":["Return a dimensionless ratio without inventing or replacing source provenance."],
            "input_schema":{"type":"object","properties":{"latency":{"type":"number","x-engarg-unit":"ms"},"budget":{"type":"number","x-engarg-unit":"s"},"revision":{"type":"string"}},"required":["latency","budget","revision"],"additionalProperties":false},
            "output_schema":{"type":"object","properties":{"ratio":{"type":"number","x-engarg-unit":"1"}},"required":["ratio"],"additionalProperties":false}}})).unwrap();
    (blueprint, elaboration)
}

#[test]
fn custom_definition_fixed_schemas_and_upstream_units_reach_the_reusable_request() {
    let (blueprint, elaboration) = fixture();
    let request = request(&blueprint, &elaboration).unwrap();
    let purpose: Value = serde_json::from_str(&request.operation_purpose).unwrap();
    assert_eq!(
        purpose["definition"],
        blueprint.operation_spec.as_ref().unwrap().definition
    );
    assert_eq!(
        purpose["input_conditions"],
        json!(["The budget must be positive; reject nonnumeric inputs rather than coercing them."])
    );
    assert_eq!(
        request.input_schema["properties"]["latency"]["type"],
        "number"
    );
    assert_eq!(
        request.input_schema["properties"]["latency"]["x-engarg-source-contract"]["x-engarg-quantity"]
            ["unit"],
        "ms"
    );
    assert_eq!(
        request.input_schema["properties"]["budget"]["x-engarg-source-contract"]["x-engarg-quantity"]
            ["unit"],
        "s"
    );
    assert_eq!(
        request.output_schema["properties"]["ratio"]["type"],
        "number"
    );
    assert_eq!(
        request.output_schema["properties"]["ratio"]["x-engarg-quantity"]["unit"],
        "1"
    );
    assert_eq!(
        request.output_schema["properties"]["collected"]["type"],
        "boolean"
    );
    request.validate().unwrap();
}

#[test]
fn request_cache_preserves_algorithm_changes_but_ignores_consumer_scope_and_source_ids() {
    let (mut blueprint, mut elaboration) = fixture();
    let original = request(&blueprint, &elaboration)
        .unwrap()
        .identity()
        .unwrap();
    blueprint
        .contract
        .scope
        .selectors
        .insert("revision".into(), json!("r2"));
    blueprint.contract.scope.subject = "A different consumer's assessment".into();
    blueprint.contract.time.max_age_seconds = Some(300);
    blueprint.arguments["revision"] = json!("r2");
    for source in &mut elaboration.acquisitions {
        source
            .contract
            .scope
            .selectors
            .insert("revision".into(), json!("r2"));
        source.arguments["revision"] = json!("r2");
    }
    elaboration.acquisitions[0].id = "different_source_id".into();
    blueprint.bindings[0].output.step_id = "different_source_id".into();
    assert_eq!(
        original,
        request(&blueprint, &elaboration)
            .unwrap()
            .identity()
            .unwrap()
    );
    blueprint.operation_spec.as_mut().unwrap().definition =
        "Convert both quantities to seconds and subtract budget from latency.".into();
    assert_ne!(
        original,
        request(&blueprint, &elaboration)
            .unwrap()
            .identity()
            .unwrap()
    );
}

#[test]
fn unspecified_operations_and_contradictory_units_do_not_generate_guessed_code() {
    let (mut blueprint, elaboration) = fixture();
    blueprint.operation_spec.as_mut().unwrap().input_schema["properties"]["latency"]["x-engarg-unit"] =
        json!("s");
    assert!(
        request(&blueprint, &elaboration)
            .unwrap_err()
            .to_string()
            .contains("unit differs")
    );
    blueprint.operation_spec = None;
    assert!(
        request(&blueprint, &elaboration)
            .unwrap_err()
            .to_string()
            .contains("explicit parameterised operation_spec")
    );
}

#[test]
fn known_derived_shapes_cannot_be_discarded_by_a_scalar_function_schema() {
    use eal_collector_contracts::execution::{
        DerivationOperation, DerivationStep, OutputReference,
    };
    let (mut blueprint, mut elaboration) = fixture();
    elaboration.derivations.push(DerivationStep {
        id: "mean_latency".into(),
        inputs: vec![OutputReference {
            step_id: "latency_source".into(),
            pointer: "/latency".into(),
        }],
        operation: DerivationOperation::Mean,
    });
    blueprint.bindings[0].output = OutputReference {
        step_id: "mean_latency".into(),
        pointer: "".into(),
    };
    assert!(
        request(&blueprint, &elaboration)
            .unwrap_err()
            .to_string()
            .contains("known dependency value shape")
    );
}
