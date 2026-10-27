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
