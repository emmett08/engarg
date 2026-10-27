//! Executable EAL contains the acquisitions, checks and argument semantics.
use eal_core::{ExecutionLimits, composition, methods::default_registry, validation};
use engarg::{export_policy::ExportPolicy, support_export};
use std::collections::BTreeMap;
#[path = "common/support.rs"]
mod support_fixture;

#[test]
fn direct_bindings_compile_without_capabilities_or_registration() {
    let (request, case, elaboration, _, _) = support_fixture::fixture();
    let versions = BTreeMap::from([("shared_source".into(), "1".into())]);
    let output = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions,
        &ExportPolicy::default(),
    )
    .unwrap();
    assert!(output.bundle.source.starts_with("language \"EAL/4\""));
    assert!(!output.bundle.source.contains("engarg_support_graph"));
    assert!(!output.bundle.source.contains("execution_plan_sha256"));
    let program =
        composition::compile(&output.bundle.source, None, "", ExecutionLimits::default()).unwrap();
    let errors = validation::validate(&program, default_registry());
    assert!(errors.is_empty(), "{errors:?}\n{}", output.bundle.source);
    assert_eq!(
        program
            .evidence
            .values()
            .filter(|e| e.tool.as_deref()
                == Some(&support_export::operation_tool_name("shared_source")))
            .count(),
        1
    );
    assert!(program.evidence.values().any(|e| e.derivation.is_some()));
}

#[test]
fn missing_concrete_binding_is_a_build_error() {
    let (request, case, elaboration, _, _) = support_fixture::fixture();
    let error = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &BTreeMap::new(),
        &ExportPolicy::default(),
    )
    .err()
    .expect("missing binding must fail");
    assert!(error.to_string().contains("No direct TOML binding"));
}

#[test]
fn compilation_is_deterministic_and_has_one_canonical_claim() {
    let (request, case, elaboration, _, _) = support_fixture::fixture();
    let versions = BTreeMap::from([("shared_source".into(), "1".into())]);
    let a = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions,
        &ExportPolicy::default(),
    )
    .unwrap();
    let b = support_export::compile_direct(
        &request,
        &case,
        &elaboration,
        &versions,
        &ExportPolicy::default(),
    )
    .unwrap();
    assert_eq!(a.bundle.source, b.bundle.source);
    assert_eq!(a.bundle.revision, b.bundle.revision);
    assert!(!a.bundle.claim_names["current_claim"].starts_with("support_claim"));
    assert!(!a.bundle.source.contains("Generated support gates"));
}
