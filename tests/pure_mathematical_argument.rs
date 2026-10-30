//! Explicit mathematical claims use native proof without manufactured observations.
use engarg::{
    domain::{
        ArgumentCase, MethodClaimMode, MethodScalar, ReasoningMethodBinding, Request, case_schema,
        validate_case,
    },
    support_export::compile_direct,
    support_requirements::{
        SUPPORT_ELABORATION_SCHEMA, SupportElaboration, material_targets, validate_elaboration,
    },
};
use serde_json::{Value, json};

#[path = "common/live_output.rs"]
mod live_output;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;

fn fixture(method: &str, query: Value) -> (Request, ArgumentCase, SupportElaboration) {
    let mode = reasoning::cases().remove(0);
    let (request, mut case) = reasoning::fixture(&mode);
    case.grounds.clear();
    case.assumptions.clear();
    case.evidence_plan.clear();
    case.reading[0].paragraphs[0].reference_ids = vec!["current_claim".into(), "W1".into()];
    case.scope.system_boundary = "Literal immutable mathematical definitions only.".into();
    case.scope.operating_conditions =
        "No physical or sampled system is represented by this proof.".into();
    case.warrants[0].premise_ids.clear();
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut case.warrants[0].reasoning_method else {
        panic!("typed fixture")
    };
    bound.claim_mode = MethodClaimMode::MathematicalConsequence;
    bound.method = serde_json::from_value(json!({"identifier":method,"query":query})).unwrap();
    bound.formal_premises.clear();
    bound.evidence_requirements.clear();
    bound.assumption_ids.clear();
    bound.conclusion.subject = "Literal authored mathematical definitions".into();
    bound.conclusion.scope = serde_json::from_value(json!({
        "kind":"mathematical_model", "selectors":[{"name":"definition","value":"closed-test-v1"}],
        "identity_dimensions":["definition"]
    }))
    .unwrap();
    bound.conclusion.result.path = if method == "finite_probability/1" {
        "satisfies"
    } else {
        "entailed"
    }
    .into();
    let support = SupportElaboration {
        schema: SUPPORT_ELABORATION_SCHEMA.into(),
        requirements: vec![],
        acquisitions: vec![],
        derivations: vec![],
        evaluations: vec![],
        revisions: vec![],
        acquisition_revisions: vec![],
        alternative_support: vec![],
    };
    (request, case, support)
}

fn theorem() -> Value {
    json!({"premises":[],"conclusion":{"implies":[{"and":["p",{"implies":["p","q"]}]},"q"]}})
}

fn finite(threshold: &str) -> Value {
    json!({
        "nodes":[{"name":"Coin","states":["heads","tails"],"parents":[],"probabilities":["1/4","3/4"]}],
        "evidence":{}, "target":{"variable":"Coin","state":"heads"},
        "comparison":{"operator":"==","threshold":threshold}
    })
}

fn assess(method: &str, query: Value, expected: &str) {
    let (request, case, support) = fixture(method, query);
    validate_case(&request, &case).unwrap();
    assert!(material_targets(&case).is_empty());
    assert!(validate_elaboration(&case, &support).is_empty());
    let projection = compile_direct(
        &request,
        &case,
        &support,
        &Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert!(projection.review_requests.is_empty());
    assert!(!projection.bundle.source.contains("\nevidence "));
    assert!(!projection.bundle.source.contains(" binding "));
    if method == "deductive/1" {
        assert!(
            projection
                .bundle
                .source
                .contains("require consistent_premises == true")
        );
    }
    let program = eal_core::composition::compile(
        &projection.bundle.source,
        None,
        "",
        eal_core::ExecutionLimits::default(),
    )
    .unwrap();
    assert!(eal_core::validation::validate_default(&program).is_empty());
    let result = eal_core::evaluation::evaluate_default(
        &program,
        &json!({}),
        reasoning::NOW,
        &projection.bundle.context,
    )
    .unwrap();
    let claim = &projection.bundle.claim_names["current_claim"];
    assert_eq!(result["claims"][claim]["status"], expected, "{result}");
    assert_eq!(result["evidence"], json!({}));
    let route = projection.bundle.source_map["W1"]
        .declarations
        .iter()
        .find(|declaration| declaration.kind == "argument")
        .unwrap();
    assert_eq!(
        result["arguments"][&route.name]["reasoning_result"]["input_origin"],
        "authored_program"
    );
}

#[test]
fn closed_implication_and_exact_finite_model_execute_without_observations() {
    assess("deductive/1", theorem(), "supported");
    assess("finite_probability/1", finite("1/4"), "supported");
}

#[test]
fn a_false_closed_conjecture_is_admitted_but_never_supported() {
    assess(
        "deductive/1",
        json!({"premises":[],"conclusion":{"and":["p",{"not":"p"}]}}),
        "unsupported",
    );
    assess("finite_probability/1", finite("1/2"), "unsupported");
}

#[test]
fn omission_keeps_historical_empirical_source_requirements() {
    let mode = reasoning::cases().remove(0);
    let (_, case) = reasoning::fixture(&mode);
    let persisted = serde_json::to_value(&case).unwrap();
    assert!(
        persisted["warrants"][0]["reasoning_method"]
            .get("claim_mode")
            .is_none()
    );
    let loaded: ArgumentCase = serde_json::from_value(persisted.clone()).unwrap();
    assert_eq!(
        loaded.warrants[0]
            .reasoning_method
            .as_ref()
            .unwrap()
            .bound()
            .unwrap()
            .claim_mode,
        MethodClaimMode::Empirical
    );
    assert_eq!(serde_json::to_value(loaded).unwrap(), persisted);
    let (request, mut pure, support) = fixture("deductive/1", theorem());
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut pure.warrants[0].reasoning_method else {
        unreachable!()
    };
    bound.claim_mode = MethodClaimMode::Empirical;
    assert!(validate_case(&request, &pure).is_err());
    assert!(material_targets(&pure).contains("W1"));
    assert!(!validate_elaboration(&pure, &support).is_empty());
}

#[test]
fn mathematical_mode_cannot_hide_external_premises_physical_scope_or_a_false_predicate() {
    for mutation in ["scope", "premises", "predicate", "method"] {
        let (request, mut case, _) = fixture("deductive/1", theorem());
        let Some(ReasoningMethodBinding::Bound(bound)) = &mut case.warrants[0].reasoning_method
        else {
            unreachable!()
        };
        match mutation {
            "scope" => bound.conclusion.scope.kind = "physical_installation".into(),
            "premises" => bound.method.query = json!({"premises":["p"],"conclusion":"p"}),
            "predicate" => bound.conclusion.result.expected = MethodScalar::Boolean(false),
            "method" => {
                bound.method =
                    serde_json::from_value(json!({"identifier":"structured/1","query":{}})).unwrap()
            }
            _ => unreachable!(),
        }
        assert!(validate_case(&request, &case).is_err(), "{mutation}");
    }
    let mode = reasoning::cases().remove(0);
    let (request, mut case) = reasoning::fixture(&mode);
    let Some(ReasoningMethodBinding::Bound(bound)) = &mut case.warrants[0].reasoning_method else {
        unreachable!()
    };
    bound.claim_mode = MethodClaimMode::MathematicalConsequence;
    bound.conclusion.scope.kind = "mathematical_model".into();
    let errors = validate_case(&request, &case).unwrap_err().join("; ");
    assert!(errors.contains("without ground premises"), "{errors}");
}

#[test]
fn authoring_transport_requires_an_explicit_claim_mode() {
    let (_, case, _) = fixture("deductive/1", theorem());
    let mut authored = live_output::generation_value(&case);
    let validator = jsonschema::validator_for(&case_schema()).unwrap();
    assert!(
        validator.is_valid(&authored),
        "{:?}",
        validator
            .iter_errors(&authored)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );
    authored["warrants"][0]["reasoning_method"]
        .as_object_mut()
        .unwrap()
        .remove("claim_mode");
    assert!(!validator.is_valid(&authored));
}
