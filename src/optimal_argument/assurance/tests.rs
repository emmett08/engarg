//! Independently specified policy attacks and actual native computation checks.
use super::*;
use crate::{direct_planning::DirectPlan, domain::SupportRelationship, engine::Mode};

#[path = "../../../tests/common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "../../../tests/common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "../../../tests/common/reasoning_graph.rs"]
mod reasoning_graph;

fn original() -> RunReport {
    let (request, argument, support) = executable_case::fixture();
    RunReport {
        schema_version: "engarg.run.v3".into(),
        execution_mode: "offline_synthetic_fixture".into(),
        input_sha256: digest(&request).unwrap(),
        provider_config: None,
        mode: Mode::Single,
        run_config: None,
        request,
        argument,
        transcript: vec![],
        tooling: None,
        direct_bindings: Some(DirectPlan::unbound(&support)),
        support_elaboration: Some(support),
        design_review: None,
        argument_revisions: vec![],
        target_runtime: None,
        authoring_recovery: None,
        evidence_planning: None,
        replanning: None,
        support_recovery: None,
        limitations: vec![],
    }
}

fn candidate(original: &RunReport) -> SupportedArgument {
    SupportedArgument {
        argument: original.argument.clone(),
        support_elaboration: original.support_elaboration.clone().unwrap(),
    }
}

fn policy(original: &RunReport) -> AssurancePlan {
    let index = crate::domain::ArgumentIndex::new(&original.argument);
    let selected = index.selected_dependency("current_claim");
    let mut checks: Vec<_> = original
        .support_elaboration
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .filter(|r| selected.contains(r.target_id.as_str()))
        .map(|r| VerificationCheck {
            id: format!("check_{}", r.id),
            role: CheckRole::Premise,
            target_id: r.target_id.clone(),
            procedure: CheckProcedure::Requirement {
                requirement_id: r.id.clone(),
            },
            discharges: vec![],
        })
        .collect();
    let bound = original.argument.warrants[0]
        .reasoning_method
        .as_ref()
        .unwrap()
        .bound()
        .unwrap();
    checks.push(VerificationCheck {
        id: "formal_query".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Method {
            conclusion_id: "current_claim".into(),
            identifier: bound.method.identifier().into(),
            reference_query: bound.method.query.clone(),
            result: bound.conclusion.result.clone(),
        },
        discharges: vec![],
    });
    AssurancePlan {
        schema: PLAN_SCHEMA.into(),
        checks,
        verification: vec![],
    }
}

fn execution(proposal: &SupportedArgument, status: &str, issues: Value) -> Value {
    let mut records = json!({});
    let mut evidence = json!({});
    for requirement in &proposal.support_elaboration.requirements {
        let id = crate::eal_export::name("requirement", &requirement.id);
        records[&id] = json!({"status":"ok","collected_at":"2026-10-25T12:00:00Z"});
        evidence[&id] = json!({"status":status,"availability_issues":issues});
    }
    json!({"is_error":false,"result":{"schema":"EAL/execution/1","source_digest":"synthetic-source",
        "collection":{"records":records},"assessment":{"valid":true,"complete":true,
        "assessed_at":"2026-10-25T12:00:00Z","evidence":evidence}}})
}

#[test]
fn complete_serialized_plan_roundtrips_every_procedure_without_precision_loss() {
    let original = original();
    let mut plan = policy(&original);
    let exact = eal_collector_contracts::strict_json_exact(
        br#"{"integer":9007199254740993123456789,"decimal":0.12345678901234567890123456789,"$serde_json::private::Number":"literal reserved key"}"#,
        Default::default(),
    ).unwrap();
    plan.checks.push(VerificationCheck {
        id: "exact_reference".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Value {
            pointer: "/argument/adjudication".into(),
            expected: exact.clone(),
        },
        discharges: vec![],
    });
    let bound = original.argument.warrants[0]
        .reasoning_method
        .as_ref()
        .unwrap()
        .bound()
        .unwrap();
    plan.checks.push(VerificationCheck {
        id: "equivalent_reference".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::LogicalEquivalence {
            conclusion_id: "current_claim".into(),
            reference_query: bound.method.query.clone(),
        },
        discharges: vec![],
    });
    let serialized = serde_json::to_vec(&plan).unwrap();
    let decoded: AssurancePlan = serde_json::from_slice(&serialized).unwrap();
    decoded.validate(&original).unwrap();
    assert_eq!(digest(&decoded).unwrap(), digest(&plan).unwrap());
    let CheckProcedure::Value { expected, .. } =
        &decoded.checks[decoded.checks.len() - 2].procedure
    else {
        panic!("typed value reference")
    };
    assert_eq!(expected["integer"].to_string(), "9007199254740993123456789");
    assert_eq!(
        expected["decimal"].to_string(),
        "0.12345678901234567890123456789"
    );
    assert_eq!(
        expected["$serde_json::private::Number"],
        "literal reserved key"
    );
    let from_value: AssurancePlan =
        serde_json::from_value(serde_json::to_value(&decoded).unwrap()).unwrap();
    assert_eq!(digest(&from_value).unwrap(), digest(&plan).unwrap());
}

#[test]
fn procedure_decoding_rejects_duplicates_unknown_tags_and_unknown_fields() {
    for invalid in [
        r#"{"kind":"value","pointer":"/x","expected":{"x":1,"x":2}}"#,
        r#"{"kind":"value","kind":"requirement","pointer":"/x","expected":1}"#,
        r#"{"kind":"value","pointer":"/x","expected":1,"extra":true}"#,
        r#"{"kind":"requirement","requirement_id":"R1","extra":true}"#,
        r#"{"kind":"method","conclusion_id":"current_claim","identifier":"deductive/1","reference_query":{"premises":[],"premises":[],"conclusion":"p"},"result":{"path":"entailed","operator":"==","expected":true}}"#,
        r#"{"kind":"logical_equivalence","conclusion_id":"current_claim","reference_query":{},"extra":true}"#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":3}"#,
    ] {
        assert!(
            serde_json::from_str::<CheckProcedure>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn native_formal_check_and_empirical_checks_remain_separate() {
    let original = original();
    let proposal = candidate(&original);
    let plan = policy(&original);
    let mut receipt = prepare(&original, &proposal, &plan).unwrap();
    assert!(receipt.ready_for_execution());
    assert!(!receipt.passed());
    let formal = receipt
        .checks
        .iter()
        .find(|c| c.id == "formal_query")
        .unwrap();
    assert_eq!(formal.status, CheckStatus::Pass);
    assert_eq!(formal.details["method_result"]["details"]["entailed"], true);
    assert!(formal.details.get("observed_at").is_none());
    finish(
        &mut receipt,
        &proposal,
        &execution(&proposal, "available", json!([])),
        &plan,
    )
    .unwrap();
    assert!(
        !receipt.passed(),
        "legacy checks cannot certify missing semantic coverage"
    );
    assert!(
        receipt
            .checks
            .iter()
            .filter(|c| c.id != "coverage:legacy_plan")
            .all(|c| c.status == CheckStatus::Pass)
    );
    assert!(
        receipt
            .checks
            .iter()
            .filter(|c| c.role == CheckRole::Premise)
            .all(|c| c.details["observed_at"] == "2026-10-25T12:00:00Z")
    );
}

#[test]
fn requirement_receipt_exposes_evaluated_projection_and_original_provenance_only() {
    let original = original();
    let candidate = candidate(&original);
    let policy = policy(&original);
    let requirement = candidate
        .support_elaboration
        .requirements
        .iter()
        .find(|requirement| requirement.target_id == "G1")
        .unwrap();
    let declaration = crate::eal_export::name("requirement", &requirement.id);
    let input = json!({"values":{"v0":{"count":3,"distinct":true}}});
    let dependency = json!({
        "evidence_id":"acquisition_measured_source", "run_id":"retained-run",
        "origin_run_id":"original-run", "collected_at":"2026-10-25T12:00:00Z",
        "source_digest":"1".repeat(64), "data_digest":"2".repeat(64),
        "input_digest":"3".repeat(64), "acquisition_request_digest":"4".repeat(64),
        "tool_binding_digest":"5".repeat(64),
        "acquisition_request":{"credential":"FORBIDDEN_RAW_REQUEST"},
        "context":"FORBIDDEN_CONTEXT", "transport":"FORBIDDEN_TRANSPORT",
        "env":{"TOKEN":"FORBIDDEN_ENV"}
    });
    let mut native = execution(&candidate, "available", json!([]));
    let record = &mut native["result"]["collection"]["records"][&declaration];
    record["input"] = input.clone();
    record["value"] = json!({"accepted":true});
    record["dependencies"] = json!([dependency]);
    record["context"] = json!({"credential":"FORBIDDEN_RECORD_CONTEXT"});
    native["result"]["collection"]["records"]["acquisition_unrelated"] =
        json!({"value":{"other_measurement":"FORBIDDEN_UNSELECTED_SOURCE"}});
    let mut receipt = prepare(&original, &candidate, &policy).unwrap();
    finish(&mut receipt, &candidate, &native, &policy).unwrap();
    assert!(
        !receipt.passed(),
        "legacy source receipts leave semantic coverage unknown"
    );
    let details = &receipt
        .checks
        .iter()
        .find(|check| check.target_id == "G1")
        .unwrap()
        .details;
    assert_eq!(details["evaluation_input"], input);
    assert_eq!(details["evaluation_input"]["values"]["v0"]["count"], 3);
    assert_eq!(details["evaluation_value"], json!({"accepted":true}));
    assert_ne!(details["evaluation_input"], details["evaluation_value"]);
    assert_eq!(
        details["acceptance"],
        serde_json::to_value(&requirement.acceptance).unwrap()
    );
    assert_eq!(details["observed_at"], "2026-10-25T12:00:00Z");
    assert_eq!(details["assessed_at"], "2026-10-25T12:00:00Z");
    let retained = &details["dependencies"][0];
    assert_eq!(retained["origin_run_id"], "original-run");
    assert_eq!(retained["collected_at"], "2026-10-25T12:00:00Z");
    assert_eq!(retained["data_digest"], "2".repeat(64));
    assert_eq!(retained["acquisition_request_digest"], "4".repeat(64));
    assert_eq!(retained.as_object().unwrap().len(), 9);
    assert!(
        !serde_json::to_string(&receipt)
            .unwrap()
            .contains("FORBIDDEN_")
    );
}

#[test]
fn stale_and_failed_observations_remain_unknown_while_measured_negatives_fail() {
    let original = original();
    let proposal = candidate(&original);
    let plan = policy(&original);
    for (issues, expected) in [
        (json!(["stale_observation"]), CheckStatus::Unknown),
        (json!(["tool_error"]), CheckStatus::Unknown),
        (json!(["predicate_not_met"]), CheckStatus::Fail),
    ] {
        let mut receipt = prepare(&original, &proposal, &plan).unwrap();
        finish(
            &mut receipt,
            &proposal,
            &execution(&proposal, "unavailable", issues),
            &plan,
        )
        .unwrap();
        assert!(!receipt.passed());
        assert!(
            receipt
                .checks
                .iter()
                .filter(|c| c.role == CheckRole::Premise)
                .all(|c| c.status == expected)
        );
    }
}

#[test]
fn wrong_formalisation_and_wrong_native_result_cannot_pass() {
    let original = original();
    let plan = policy(&original);
    let mut proposal = candidate(&original);
    if let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut proposal.argument.warrants[0].reasoning_method
    {
        bound.method.query["conclusion"] = json!("p");
    }
    let receipt = prepare(&original, &proposal, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    assert_eq!(
        receipt
            .checks
            .iter()
            .find(|c| c.id == "formal_query")
            .unwrap()
            .status,
        CheckStatus::Fail
    );
    let mut plan = plan;
    let mut proposal = candidate(&original);
    if let CheckProcedure::Method { result, .. } = &mut plan.checks.last_mut().unwrap().procedure {
        result.expected = crate::domain::MethodScalar::Boolean(false);
    }
    if let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut proposal.argument.warrants[0].reasoning_method
    {
        bound.conclusion.result.expected = crate::domain::MethodScalar::Boolean(false);
    }
    let receipt = prepare(&original, &proposal, &plan).unwrap();
    let formal = receipt
        .checks
        .iter()
        .find(|c| c.id == "formal_query")
        .unwrap();
    assert_eq!(formal.status, CheckStatus::Fail);
    assert_eq!(formal.details["method_result"]["status"], "supported");
    assert_eq!(formal.details["predicate_result"]["status"], "unsupported");
}

#[test]
fn equivalence_uses_countermodels_instead_of_outer_method_status() {
    let original = original();
    let proposal = candidate(&original);
    let mut check = policy(&original).checks.pop().unwrap();
    let mut reference = proposal.argument.warrants[0]
        .reasoning_method
        .as_ref()
        .unwrap()
        .bound()
        .unwrap()
        .method
        .query
        .clone();
    reference["conclusion"] = json!("p");
    check.procedure = CheckProcedure::LogicalEquivalence {
        conclusion_id: "current_claim".into(),
        reference_query: reference,
    };
    let result = checks::prepare_check(&proposal, &check).unwrap();
    assert_eq!(result.status, CheckStatus::Fail);
    let conclusion = &result.details["parts"][1]["method_result"];
    assert_eq!(conclusion["status"], "supported");
    assert_eq!(conclusion["details"]["entailed"], false);
    assert!(conclusion["details"]["counterexample"].is_object());
}

#[test]
fn target_weakening_and_criterion_rewriting_are_rejected() {
    let original = original();
    let plan = policy(&original);
    let mut changed = candidate(&original);
    changed
        .argument
        .current_claim
        .qualifier
        .push_str(" or a different workload");
    let receipt = prepare(&original, &changed, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    let mut changed = candidate(&original);
    changed.support_elaboration.requirements[0]
        .acceptance_rationale
        .push_str(" relaxed");
    let receipt = prepare(&original, &changed, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| r.contains("rewritten without a checked discharge"))
    );
}

#[test]
fn retained_but_unreachable_critical_conditions_are_not_preserved() {
    let original = original();
    let plan = policy(&original);
    let mut changed = candidate(&original);
    let mut shortcut = changed.argument.warrants[0].clone();
    shortcut.id = "W2".into();
    if let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut shortcut.reasoning_method
    {
        bound.assumption_ids.clear();
        bound
            .evidence_requirements
            .retain(|r| r.evidence_need_id != "N6");
    }
    changed.argument.warrants.push(shortcut);
    changed.argument.compositions[0].relationship = SupportRelationship::AlternativeRoutes;
    changed.argument.compositions[0].selected_warrant_ids = vec!["W2".into()];
    crate::domain::validate_case(&original.request, &changed.argument).unwrap();
    let receipt = prepare(&original, &changed, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| r.contains("Critical condition A1"))
    );
}

#[test]
fn changed_symbol_binding_requires_an_independent_binding_reference() {
    let original = original();
    let plan = policy(&original);
    let mut changed = candidate(&original);
    if let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut changed.argument.warrants[0].reasoning_method
    {
        bound.formal_premises[0].interpretation =
            "A different meaning for this formal atom.".into();
    }
    let receipt = prepare(&original, &changed, &plan).unwrap();
    assert!(!receipt.ready_for_execution());
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| r.contains("symbol-to-premise bindings"))
    );
}

#[test]
fn empirical_discharge_missing_coverage_and_missing_formalisation_fail_plan_admission() {
    let original = original();
    let mut plan = policy(&original);
    let empirical = original
        .support_elaboration
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .find(|r| r.target_id == "G1")
        .unwrap();
    plan.checks.last_mut().unwrap().discharges.push(Discharge {
        requirement_id: empirical.id.clone(),
        requirement_sha256: digest(empirical).unwrap(),
    });
    assert!(
        plan.validate(&original)
            .unwrap_err()
            .to_string()
            .contains("empirical observation")
    );
    let mut plan = policy(&original);
    plan.checks.remove(0);
    assert!(
        plan.validate(&original)
            .unwrap_err()
            .to_string()
            .contains("omits selected original obligation")
    );
    let mut plan = policy(&original);
    plan.checks.pop();
    assert!(
        plan.validate(&original)
            .unwrap_err()
            .to_string()
            .contains("formalisation check")
    );
}

#[test]
fn receipts_are_bound_to_the_exact_candidate_and_policy() {
    let original = original();
    let mut proposal = candidate(&original);
    let plan = policy(&original);
    let mut receipt = prepare(&original, &proposal, &plan).unwrap();
    proposal
        .argument
        .adjudication
        .push_str("Changed after checking.");
    assert!(
        finish(
            &mut receipt,
            &proposal,
            &execution(&proposal, "available", json!([])),
            &plan
        )
        .is_err()
    );
}

#[test]
fn a_checked_digest_bound_formal_discharge_licenses_only_its_original_obligation() {
    use eal_collector_contracts::{
        capability::{LogicalStrength, SupportType},
        execution::{EvaluationCriterion, RequiredCoverage},
    };
    let mut original = original();
    let support = original.support_elaboration.as_mut().unwrap();
    let mut formal = support
        .requirements
        .iter()
        .find(|r| r.target_id == "W1")
        .unwrap()
        .clone();
    formal.id = "formal_proof".into();
    formal.evidence_need_ids.clear();
    formal.original_acceptance_criteria.clear();
    let contract = formal.contract.as_mut().unwrap();
    contract.support_type = SupportType::ProofCheck;
    contract.logical_strength = LogicalStrength::CheckedProof;
    formal.acceptance = EvaluationCriterion::Unavailable {
        reason: "A checked native derivation of the declared q consequence is required.".into(),
    };
    let mut evaluation = support
        .evaluations
        .iter()
        .find(|e| e.requirement_id == "requirement_W1")
        .unwrap()
        .clone();
    evaluation.id = "formal_proof_check".into();
    evaluation.requirement_id = formal.id.clone();
    evaluation.inputs.clear();
    evaluation.criterion = formal.acceptance.clone();
    evaluation.required_coverage = RequiredCoverage::Unspecified;
    support.requirements.push(formal.clone());
    support.evaluations.push(evaluation);
    crate::report::validate(&original).unwrap();
    let mut policy = policy(&original);
    policy.checks.retain(|c|!matches!(&c.procedure,CheckProcedure::Requirement {requirement_id} if requirement_id=="formal_proof"));
    policy
        .checks
        .last_mut()
        .unwrap()
        .discharges
        .push(Discharge {
            requirement_id: "formal_proof".into(),
            requirement_sha256: digest(&formal).unwrap(),
        });
    policy.validate(&original).unwrap();
    let mut candidate = candidate(&original);
    candidate.support_elaboration.requirements.iter_mut().find(|r|r.id=="formal_proof").unwrap()
        .acceptance_rationale="The operator-authorised immutable native proof check now discharges this formal obligation.".into();
    let receipt = prepare(&original, &candidate, &policy).unwrap();
    assert!(receipt.ready_for_execution(), "{:?}", receipt.checks);
    assert_eq!(receipt.discharged_requirement_ids, ["formal_proof"]);
    assert!(!receipt.passed(), "Empirical source checks remain pending");
    let mut stale = policy.clone();
    stale.checks.last_mut().unwrap().discharges[0].requirement_sha256 = "0".repeat(64);
    assert!(stale.validate(&original).is_err());
    candidate
        .support_elaboration
        .requirements
        .iter_mut()
        .find(|r| r.target_id == "G1")
        .unwrap()
        .acceptance_rationale = "This other empirical obligation was weakened too.".into();
    assert!(
        !prepare(&original, &candidate, &policy)
            .unwrap()
            .ready_for_execution()
    );
}

#[test]
fn reusing_a_ground_id_cannot_change_what_its_observation_is_claimed_to_establish() {
    let original = original();
    let policy = policy(&original);
    let mut candidate = candidate(&original);
    candidate.argument.grounds[0].statement =
        "Every physical deployment is universally correct.".into();
    let receipt = prepare(&original, &candidate, &policy).unwrap();
    assert!(!receipt.ready_for_execution());
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| r.contains("changes its proposition"))
    );
}

#[test]
fn typed_subject_unit_scope_and_interval_cannot_drift_under_the_same_query() {
    let original = original();
    let policy = policy(&original);
    for mutation in ["subject", "quantity", "scope", "interval"] {
        let mut candidate = candidate(&original);
        let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
            &mut candidate.argument.warrants[0].reasoning_method
        else {
            panic!("bound fixture")
        };
        match mutation {
            "subject" => bound.conclusion.subject = "A different installation".into(),
            "quantity" => bound.conclusion.quantity = "dimensionless".into(),
            "scope" => {
                bound.conclusion.scope.selectors[0].value =
                    crate::domain::MethodScopeScalar::String("different-fixture".into())
            }
            "interval" => bound.conclusion.valid_until = "2100-01-01T00:00:00Z".into(),
            _ => unreachable!(),
        }
        let receipt = prepare(&original, &candidate, &policy).unwrap();
        assert!(!receipt.ready_for_execution(), "{mutation}");
        let expected_reason = if mutation == "quantity" {
            "does not accept quantity"
        } else {
            "validity interval"
        };
        assert!(
            receipt
                .checks
                .iter()
                .flat_map(|c| &c.reasons)
                .any(|r| r.contains(expected_reason)),
            "{mutation}: {:?}",
            receipt.checks
        );
    }
    // A malformed unit is rejected by ordinary typed argument admission too.
    let mut candidate = candidate(&original);
    let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut candidate.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
    };
    bound.conclusion.unit = "s".into();
    assert!(
        !prepare(&original, &candidate, &policy)
            .unwrap()
            .ready_for_execution()
    );
}

#[test]
fn an_operator_complete_conclusion_reference_can_authorise_a_new_interval_explicitly() {
    let original = original();
    let mut policy = policy(&original);
    let mut candidate = candidate(&original);
    let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut candidate.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
    };
    bound.conclusion.valid_until = "2100-01-01T00:00:00Z".into();
    let intended = serde_json::to_value(&bound.conclusion).unwrap();
    policy.checks.push(VerificationCheck {
        id: "intended_conclusion".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Value {
            pointer: "/argument/warrants/0/reasoning_method/conclusion".into(),
            expected: intended,
        },
        discharges: vec![],
    });
    let receipt = prepare(&original, &candidate, &policy).unwrap();
    assert!(receipt.ready_for_execution(), "{:?}", receipt.checks);
    assert!(
        !receipt.passed(),
        "Actual empirical premise checks remain pending"
    );
}

#[test]
fn an_unrelated_formal_value_cannot_authorise_changed_query_or_result() {
    let original = original();
    for mutation in ["query", "result"] {
        let mut candidate = candidate(&original);
        let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
            &mut candidate.argument.warrants[0].reasoning_method
        else {
            panic!("bound fixture")
        };
        if mutation == "query" {
            bound.method.query["conclusion"] = json!("p");
        } else {
            bound.conclusion.result.expected = crate::domain::MethodScalar::Boolean(false);
        }
        let mut policy = policy(&original);
        policy.checks.last_mut().unwrap().procedure = CheckProcedure::Value {
            pointer: "/argument/warrants/0/reasoning_method/formal_premises".into(),
            expected: serde_json::to_value(&bound.formal_premises).unwrap(),
        };
        let receipt = prepare(&original, &candidate, &policy).unwrap();
        assert!(!receipt.ready_for_execution(), "{mutation}");
        assert!(
            receipt
                .checks
                .iter()
                .flat_map(|c| &c.reasons)
                .any(|r| { r.contains("changes its reasoning method binding") }),
            "{mutation}: {:?}",
            receipt.checks
        );

        // An independent complete binding reference explicitly authorises the
        // exact programme change, rather than incidentally matching a subfield.
        policy.checks.last_mut().unwrap().procedure = CheckProcedure::Value {
            pointer: "/argument/warrants/0/reasoning_method".into(),
            expected: serde_json::to_value(&candidate.argument.warrants[0].reasoning_method)
                .unwrap(),
        };
        assert!(
            prepare(&original, &candidate, &policy)
                .unwrap()
                .ready_for_execution()
        );
    }
}

#[test]
fn native_method_removal_or_downgrade_requires_a_complete_operator_reference() {
    let original = original();
    let mut policy = policy(&original);
    policy.checks.last_mut().unwrap().procedure = CheckProcedure::Value {
        pointer: "/argument/current_claim".into(),
        expected: serde_json::to_value(&original.argument.current_claim).unwrap(),
    };
    for binding in [
        None,
        Some(crate::domain::ReasoningMethodBinding::Unresolved {
            method: crate::domain::ReasoningMethodKind::Deductive,
            reasons: vec!["No checked native formalisation is supplied.".into()],
            evidence_need_ids: vec![],
        }),
    ] {
        let mut candidate = candidate(&original);
        candidate.argument.warrants[0].reasoning_method = binding;
        let receipt = prepare(&original, &candidate, &policy).unwrap();
        assert!(!receipt.ready_for_execution());
        assert!(
            receipt
                .checks
                .iter()
                .flat_map(|c| &c.reasons)
                .any(|r| { r.contains("changes its reasoning method binding") }),
            "{:?}",
            receipt.checks
        );
    }
}

#[test]
fn finite_probability_cannot_replace_observed_input_equality_with_authored_definition() {
    use crate::domain::{FormalInputCorrespondence, ReasoningMethodBinding};
    let mut original = original();
    let Some(ReasoningMethodBinding::Bound(bound)) =
        &mut original.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
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
    bound.formal_premises[0].correspondence = FormalInputCorrespondence::IdentityValue {
        source_input_index: 0,
        source_pointer: "/nodes".into(),
    };
    let mut policy = policy(&original);
    let mut candidate = candidate(&original);
    let Some(ReasoningMethodBinding::Bound(bound)) =
        &mut candidate.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
    };
    bound.formal_premises[0].correspondence = FormalInputCorrespondence::AuthoredDefinition;
    let expected = serde_json::to_value(&bound.formal_premises).unwrap();
    let receipt = prepare(&original, &candidate, &policy).unwrap();
    assert!(!receipt.ready_for_execution());
    assert_eq!(
        receipt
            .checks
            .iter()
            .find(|c| c.id == "formal_query")
            .unwrap()
            .status,
        CheckStatus::Pass
    );
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| { r.contains("changes formal input correspondences") }),
        "{:?}",
        receipt.checks
    );
    policy.checks.push(VerificationCheck {
        id: "intended_correspondence".into(),
        role: CheckRole::Formalisation,
        target_id: "current_claim".into(),
        procedure: CheckProcedure::Value {
            pointer: "/argument/warrants/0/reasoning_method/formal_premises".into(),
            expected,
        },
        discharges: vec![],
    });
    assert!(
        prepare(&original, &candidate, &policy)
            .unwrap()
            .ready_for_execution()
    );
}

#[test]
fn logical_equivalence_cannot_turn_a_failed_entailment_into_a_false_result_claim() {
    let mut original = original();
    let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut original.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
    };
    bound.method.query["conclusion"] = json!("r");
    let query = bound.method.query.clone();
    let computed = eal_core::methods::assess_input_with_limits(
        "deductive/1",
        &query,
        eal_core::methods::default_registry(),
        &eal_core::ExecutionLimits::default(),
    );
    assert_eq!(computed["details"]["entailed"], false);
    let mut policy = policy(&original);
    policy.checks.last_mut().unwrap().procedure = CheckProcedure::LogicalEquivalence {
        conclusion_id: "current_claim".into(),
        reference_query: query,
    };
    let mut candidate = candidate(&original);
    let Some(crate::domain::ReasoningMethodBinding::Bound(bound)) =
        &mut candidate.argument.warrants[0].reasoning_method
    else {
        panic!("bound fixture")
    };
    bound.conclusion.result.expected = crate::domain::MethodScalar::Boolean(false);
    let receipt = prepare(&original, &candidate, &policy).unwrap();
    assert_eq!(
        receipt
            .checks
            .iter()
            .find(|c| c.id == "formal_query")
            .unwrap()
            .status,
        CheckStatus::Pass
    );
    assert!(!receipt.ready_for_execution());
    assert!(
        receipt
            .checks
            .iter()
            .flat_map(|c| &c.reasons)
            .any(|r| { r.contains("changes its declared result predicate") }),
        "{:?}",
        receipt.checks
    );
}

#[test]
fn lifecycle_marks_missing_or_mismatched_native_execution_incomplete() {
    let original = original();
    let candidate = candidate(&original);
    let plan = policy(&original);
    let mut receipt = prepare(&original, &candidate, &plan).unwrap();
    receipt
        .record_execution(&candidate, &original, &json!({}), "supported")
        .unwrap();
    assert_eq!(receipt.lifecycle.phase, "execution_incomplete");
    assert!(!receipt.lifecycle.bindings.as_ref().unwrap().replay_checked);
    let mut native = execution(&candidate, "available", json!([]));
    native["result"]["executable_digest"] = json!("executable");
    native["result"]["collection"]["source_digest"] = native["result"]["source_digest"].clone();
    native["result"]["collection"]["collection_id"] = json!("collection");
    native["result"]["collection"]["context"] = json!({});
    native["result"]["assessment"]["source_digest"] = native["result"]["source_digest"].clone();
    native["result"]["assessment"]["collection_id"] = json!("collection");
    native["result"]["assessment"]["assessment_id"] = json!("assessment");
    receipt
        .record_execution(&candidate, &original, &native, "supported")
        .unwrap();
    assert_eq!(receipt.lifecycle.phase, "post_execution");
    assert!(receipt.lifecycle.bindings.as_ref().unwrap().replay_checked);
    for mutation in ["collection", "source", "complete", "executable", "schema"] {
        let mut wrong = native.clone();
        match mutation {
            "collection" => wrong["result"]["assessment"]["collection_id"] = json!("other"),
            "source" => wrong["result"]["collection"]["source_digest"] = json!("other"),
            "complete" => wrong["result"]["assessment"]["complete"] = json!(false),
            "executable" => wrong["result"]["executable_digest"] = json!(""),
            "schema" => wrong["result"]["schema"] = json!("wrong"),
            _ => unreachable!(),
        }
        receipt
            .record_execution(&candidate, &original, &wrong, "supported")
            .unwrap();
        assert_eq!(
            receipt.lifecycle.phase, "execution_incomplete",
            "{mutation}"
        );
        assert!(
            !receipt.lifecycle.bindings.as_ref().unwrap().replay_checked,
            "{mutation}"
        );
    }
}
