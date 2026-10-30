//! Native reasoning requires the executable compiler's explicit source bindings.
//! Numerical/runtime acceptance lives in executable_modes and EAL's method tests.
use engarg::{
    domain::{ReasoningMethodBinding, validate_case},
    support_export::compile_direct,
};
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

#[test]
fn native_methods_reject_missing_direct_operation_bindings() {
    for mode in reasoning::cases() {
        let (request, case, elaboration, _, _) = reasoning_graph::graph_fixture(&mode);
        let error = compile_direct(
            &request,
            &case,
            &elaboration,
            &Default::default(),
            &Default::default(),
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains("No direct TOML binding for operation"),
            "{}: {error}",
            mode.kind
        );
    }
}

#[test]
fn result_path_is_checked_against_the_eal_method_descriptor_at_admission() {
    let mode = reasoning::cases().remove(0);
    let (request, mut case) = reasoning::fixture(&mode);
    let ReasoningMethodBinding::Bound(bound) = case.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.conclusion.result.path = "/entailed".into();
    assert!(validate_case(&request, &case).is_err());
}

#[test]
fn authored_finite_definitions_are_admitted_only_for_explicit_native_query_fields() {
    use engarg::domain::FormalInputCorrespondence;
    use serde_json::json;
    let mode = reasoning::cases().remove(0);
    let (request, mut case) = reasoning::fixture(&mode);
    let ReasoningMethodBinding::Bound(bound) = case.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.method = serde_json::from_value(json!({
        "identifier":"finite_probability/1", "query":{
            "nodes":[{"name":"Coin","states":["heads","tails"],"parents":[],"probabilities":["1/4","3/4"]}],
            "evidence":{},"target":{"variable":"Coin","state":"heads"},
            "comparison":{"operator":"==","threshold":"1/4"}
        }
    })).unwrap();
    bound.conclusion.quantity = "probability".into();
    bound.conclusion.result.path = "satisfies".into();
    bound.formal_premises.truncate(1);
    bound.formal_premises[0].input_pointer = "/nodes".into();
    bound.formal_premises[0].correspondence = FormalInputCorrespondence::AuthoredDefinition;
    validate_case(&request, &case).unwrap();
    let ReasoningMethodBinding::Bound(bound) = case.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.formal_premises[0].input_pointer = "/absent".into();
    let error = validate_case(&request, &case).unwrap_err().join("; ");
    assert!(error.contains("authored_definition"), "{error}");

    let (request, mut deductive) = reasoning::fixture(&mode);
    let ReasoningMethodBinding::Bound(bound) =
        deductive.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.formal_premises[0].correspondence = FormalInputCorrespondence::AuthoredDefinition;
    assert!(validate_case(&request, &deductive).is_err());
}
