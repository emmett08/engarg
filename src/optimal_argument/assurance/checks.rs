//! Execute fixed formal checks and inspect actual scoped native evidence outcomes.
use super::*;
use crate::{
    domain::{ArgumentIndex, BoundReasoningMethod},
    support_requirements::SupportedArgument,
};
use eal_core::{ExecutionLimits, methods};

fn outcome(
    check: &VerificationCheck,
    status: CheckStatus,
    reasons: Vec<String>,
    details: Value,
) -> CheckResult {
    CheckResult {
        id: check.id.clone(),
        role: check.role,
        target_id: check.target_id.clone(),
        status,
        reasons,
        details,
    }
}

fn selected_method<'a>(
    proposal: &'a SupportedArgument,
    conclusion: &str,
) -> Result<&'a BoundReasoningMethod, String> {
    let index = ArgumentIndex::new(&proposal.argument);
    let composition = index
        .composition_for(conclusion)
        .ok_or("No composition for formal check target")?;
    let [selected] = composition.selected_warrant_ids.as_slice() else {
        return Err(
            "A formal method check requires exactly one selected warrant for its target".into(),
        );
    };
    proposal
        .argument
        .warrants
        .iter()
        .find(|w| &w.id == selected)
        .and_then(|w| w.reasoning_method.as_ref())
        .and_then(|m| m.bound())
        .ok_or_else(|| "Selected warrant has no bound native method".into())
}

fn native_status(result: &Value) -> CheckStatus {
    match result["status"].as_str() {
        Some("supported") => CheckStatus::Pass,
        Some("unsupported") => CheckStatus::Fail,
        Some("incomplete" | "not_run") => CheckStatus::Unknown,
        _ => CheckStatus::Error,
    }
}

pub(super) fn prepare_check(
    proposal: &SupportedArgument,
    check: &VerificationCheck,
) -> Result<CheckResult, Error> {
    match &check.procedure {
        CheckProcedure::Requirement { requirement_id } => Ok(outcome(
            check,
            CheckStatus::Unknown,
            vec![
                "Actual source collection and native requirement assessment remain pending".into(),
            ],
            json!({"deferred_requirement":true,"requirement_id":requirement_id}),
        )),
        CheckProcedure::Value { pointer, expected } => {
            check_value(proposal, check, pointer, expected)
        }
        CheckProcedure::Method {
            conclusion_id,
            identifier,
            reference_query,
            result,
        } => check_method(
            proposal,
            check,
            conclusion_id,
            identifier,
            reference_query,
            result,
        ),
        CheckProcedure::LogicalEquivalence {
            conclusion_id,
            reference_query,
        } => check_equivalence(proposal, check, conclusion_id, reference_query),
    }
}

fn check_value(
    proposal: &SupportedArgument,
    check: &VerificationCheck,
    pointer: &str,
    expected: &Value,
) -> Result<CheckResult, Error> {
    let value = serde_json::to_value(proposal)?;
    let Some(actual) = value.pointer(pointer) else {
        return Ok(outcome(
            check,
            CheckStatus::Unknown,
            vec![format!("Candidate has no formal binding at {pointer}")],
            json!({"pointer":pointer,"expected":expected}),
        ));
    };
    let same = digest(actual)? == digest(expected)?;
    Ok(outcome(
        check,
        if same {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        vec![if same {
            "Candidate binding matches the independent intended value".into()
        } else {
            format!("Candidate binding at {pointer} differs from the independent intended value")
        }],
        json!({"pointer":pointer,"expected":expected,"actual":actual}),
    ))
}

fn check_method(
    proposal: &SupportedArgument,
    check: &VerificationCheck,
    conclusion_id: &str,
    identifier: &str,
    reference_query: &Value,
    result: &MethodResultPredicate,
) -> Result<CheckResult, Error> {
    let bound = match selected_method(proposal, conclusion_id) {
        Ok(bound) => bound,
        Err(reason) => {
            return Ok(outcome(
                check,
                CheckStatus::Unknown,
                vec![reason],
                json!({}),
            ));
        }
    };
    if bound.method.identifier() != identifier
        || digest(&bound.method.query)? != digest(reference_query)?
        || digest(&bound.conclusion.result)? != digest(result)?
    {
        return Ok(outcome(check, CheckStatus::Fail, vec!["Selected formal method, query or result predicate differs from the independent intended formalisation".into()],
            json!({"expected_method":identifier,"expected_query":reference_query,"expected_result":result,"actual_method":bound.method.identifier(),"actual_query":bound.method.query,"actual_result":bound.conclusion.result})));
    }
    let registry = methods::default_registry();
    let computed = methods::assess_input_with_limits(
        identifier,
        &bound.method.query,
        registry,
        &ExecutionLimits::default(),
    );
    let mut status = native_status(&computed);
    let mut predicate = json!({});
    if status == CheckStatus::Pass {
        let mut conclusion = bound.conclusion.clone();
        conclusion.result = result.clone();
        let proposition =
            serde_json::from_value(conclusion.proposition(&bound.method).map_err(invalid)?)?;
        predicate = eal_core::propositions::check_result(
            &proposition,
            identifier,
            &computed["details"],
            &json!({"status":"bound"}),
            registry,
        );
        status = native_status(&predicate);
    }
    Ok(outcome(check, status, vec!["Recomputed native method and its declared result predicate against the independent query".into()],
        json!({"input_sha256":digest(&bound.method.query)?,"method_result":computed,"predicate_result":predicate,"durability":"immutable programme data; no observation clock created"})))
}

fn conditions(query: &Value) -> Value {
    let premises = query["premises"]
        .as_array()
        .expect("native deductive query admission");
    match premises.as_slice() {
        [] => json!({"implies":["assurance_tautology","assurance_tautology"]}),
        [one] => one.clone(),
        _ => json!({"and":premises}),
    }
}

fn severity(status: CheckStatus) -> u8 {
    match status {
        CheckStatus::Pass => 0,
        CheckStatus::Unknown => 1,
        CheckStatus::Fail => 2,
        CheckStatus::Error => 3,
    }
}

fn check_equivalence(
    proposal: &SupportedArgument,
    check: &VerificationCheck,
    conclusion_id: &str,
    reference_query: &Value,
) -> Result<CheckResult, Error> {
    let bound = match selected_method(proposal, conclusion_id) {
        Ok(bound) => bound,
        Err(reason) => {
            return Ok(outcome(
                check,
                CheckStatus::Unknown,
                vec![reason],
                json!({}),
            ));
        }
    };
    if bound.method.identifier() != "deductive/1" {
        return Ok(outcome(
            check,
            CheckStatus::Fail,
            vec!["Logical equivalence requires a bound deductive/1 query".into()],
            json!({}),
        ));
    }
    let registry = methods::default_registry();
    let query = &bound.method.query;
    let mut errors = methods::validate_query("deductive/1", query, registry);
    errors.extend(methods::validate_query(
        "deductive/1",
        reference_query,
        registry,
    ));
    if !errors.is_empty() {
        return Ok(outcome(check, CheckStatus::Error, errors, json!({})));
    }
    let mut parts = Vec::new();
    let mut overall = CheckStatus::Pass;
    for (name, left, right) in [
        (
            "premise_conditions",
            conditions(reference_query),
            conditions(query),
        ),
        (
            "conclusion",
            reference_query["conclusion"].clone(),
            query["conclusion"].clone(),
        ),
    ] {
        let equivalence = json!({"premises":[],"conclusion":{"and":[{"implies":[left.clone(),right.clone()]},{"implies":[right,left]}]}});
        let computed = methods::assess_input_with_limits(
            "deductive/1",
            &equivalence,
            registry,
            &ExecutionLimits::default(),
        );
        let mut status = native_status(&computed);
        if status == CheckStatus::Pass
            && (computed["details"]["entailed"] != true
                || computed["details"]["consistent_premises"] != true)
        {
            status = CheckStatus::Fail;
        }
        if severity(status) > severity(overall) {
            overall = status;
        }
        parts.push(json!({"relation":name,"method_result":computed}));
    }
    Ok(outcome(check, overall, vec!["Checked premise conditions and conclusions for equivalence under the declared symbol interpretation".into()], json!({"parts":parts,"prose_verified":false})))
}

pub(super) fn finish_requirement(
    proposal: &SupportedArgument,
    execution: &Value,
    check: &VerificationCheck,
    id: &str,
) -> CheckResult {
    if execution["is_error"] != false || execution["result"]["schema"] != "EAL/execution/1" {
        return outcome(
            check,
            CheckStatus::Unknown,
            vec!["No successful native execution supplies the required premise evidence".into()],
            json!({}),
        );
    }
    let Some(requirement) = proposal
        .support_elaboration
        .requirements
        .iter()
        .find(|r| r.id == id)
    else {
        return outcome(
            check,
            CheckStatus::Unknown,
            vec![format!("Proposed support omits required obligation {id}")],
            json!({}),
        );
    };
    if requirement.target_id != check.target_id {
        return outcome(
            check,
            CheckStatus::Fail,
            vec!["Proposed requirement changed its target".into()],
            json!({}),
        );
    }
    let declaration = crate::eal_export::name("requirement", id);
    let run = &execution["result"];
    let assessment = &run["assessment"];
    let evidence = &assessment["evidence"][&declaration];
    let record = &run["collection"]["records"][&declaration];
    let status = if assessment["valid"] != true
        || assessment["complete"] != true
        || record["status"] != "ok"
        || !eal_coordination_contracts::ticket::evidence_is_measured(evidence)
    {
        CheckStatus::Unknown
    } else if evidence["status"] == "available" {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };
    outcome(
        check,
        status,
        vec![format!(
            "Native evidence assessment for the exact requirement {id}; source clocks preserved"
        )],
        json!({"declaration":declaration,"native_evidence":evidence,"record_status":record["status"],
            "evaluation_input":record["input"],"evaluation_value":record["value"],
            "acceptance":requirement.acceptance,"dependencies":dependency_provenance(record),
            "observed_at":record["collected_at"],"assessed_at":assessment["assessed_at"],"source_digest":run["source_digest"]}),
    )
}

fn dependency_provenance(record: &Value) -> Value {
    let Some(dependencies) = record["dependencies"].as_array() else {
        return Value::Null;
    };
    // Only origin identities, digests and original clocks belong in the model
    // view. In particular, never forward acquisition requests or contexts.
    Value::Array(
        dependencies
            .iter()
            .filter_map(Value::as_object)
            .map(|dependency| {
                Value::Object(
                    [
                        "evidence_id",
                        "run_id",
                        "origin_run_id",
                        "source_digest",
                        "data_digest",
                        "input_digest",
                        "acquisition_request_digest",
                        "tool_binding_digest",
                        "collected_at",
                    ]
                    .into_iter()
                    .filter_map(|key| {
                        dependency
                            .get(key)
                            .filter(|value| value.is_string() || value.is_null())
                            .map(|value| (key.into(), value.clone()))
                    })
                    .collect(),
                )
            })
            .collect(),
    )
}
