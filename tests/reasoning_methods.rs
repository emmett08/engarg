//! Native reasoning requires the executable compiler's explicit source bindings.
//! Numerical/runtime acceptance lives in executable_modes and EAL's method tests.
use engarg::{
    domain::{ReasoningMethodBinding, validate_case},
    eal_export::export,
};
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod fixtures;

#[test]
fn native_methods_require_support_elaboration_and_explicit_tool_bindings() {
    for mode in fixtures::cases() {
        let (request, case) = fixtures::fixture(&mode);
        let error = export(&request, &case).unwrap_err().to_string();
        assert!(
            error.contains("BOUND_METHOD_REQUIRES_EXECUTABLE_COMPILATION"),
            "{}: {error}",
            mode.kind
        );
        assert!(error.contains("engarg build"));
    }
}

#[test]
fn result_path_is_checked_against_the_eal_method_descriptor_at_admission() {
    let mode = fixtures::cases().remove(0);
    let (request, mut case) = fixtures::fixture(&mode);
    let ReasoningMethodBinding::Bound(bound) = case.warrants[0].reasoning_method.as_mut().unwrap()
    else {
        unreachable!()
    };
    bound.conclusion.result.path = "/entailed".into();
    assert!(validate_case(&request, &case).is_err());
}
