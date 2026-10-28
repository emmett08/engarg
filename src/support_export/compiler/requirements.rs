//! Evaluate each canonical obligation once and compose explicit target support.
use super::{
    CompiledEvidence, DerivedSpec, ENVIRONMENT, ProgramPlan, invalid, write_bind, write_derived,
};
use crate::{
    Error,
    eal_export::{name, quoted},
    export_policy::ExportPolicy,
    support_requirements::{SupportElaboration, SupportRequirementRole},
};
use eal_collector_contracts::execution::{EvaluationCriterion, RequiredCoverage};
use serde_json::{Value, json};
pub(super) fn compile(
    elaboration: &SupportElaboration,
    plan: &ProgramPlan,
    policy: &ExportPolicy,
    compiled: &mut CompiledEvidence,
) -> Result<(), Error> {
    let mut canonical_obligations = std::collections::BTreeMap::<String, String>::new();
    // One exact requirement is evaluated once. Several argument nodes may reuse
    // its evidence identity; source reuse does not create independent evidence.
    for requirement in &elaboration.requirements {
        let evaluation = plan
            .evaluations
            .iter()
            .find(|e| e.requirement_id == requirement.id)
            .ok_or_else(|| invalid(format!("No evaluation for {}", requirement.id)))?;
        let identity = eal_collector_contracts::execution::canonical_digest(&json!({
            "target":requirement.target_id,"role":requirement.role,"contract":requirement.contract,
            "criterion":evaluation.criterion,"interpretation":requirement.acceptance_rationale,
            "acceptance_clauses":requirement.original_acceptance_criteria,
            "discrimination":requirement.discriminating_evidence,"gaps":requirement.gaps,
            "inputs":evaluation.inputs,"applicability":evaluation.applicability,
            "challenges":evaluation.challenges,"required_coverage":evaluation.required_coverage,
            "coverage":evaluation.coverage,
            "max_age":evaluation.max_age_seconds.unwrap_or(policy.review_max_age_seconds),
        }))
        .map_err(Error::InvalidInput)?;
        if let Some(existing) = canonical_obligations.get(&identity) {
            compiled
                .requirements
                .insert(requirement.id.clone(), existing.clone());
            continue;
        }
        let declaration = name("requirement", &requirement.id);
        canonical_obligations.insert(identity, declaration.clone());
        compiled
            .requirements
            .insert(requirement.id.clone(), declaration.clone());
        let targets = match requirement.role {
            SupportRequirementRole::Support => &mut compiled.targets,
            SupportRequirementRole::ChallengeActivation => &mut compiled.challenge_activation,
            SupportRequirementRole::ChallengeResolution => &mut compiled.challenge_resolution,
        };
        targets
            .entry(requirement.target_id.clone())
            .or_default()
            .push(declaration.clone());
        let age = evaluation
            .max_age_seconds
            .unwrap_or(policy.review_max_age_seconds);
        let mut input = json!({"values":{}});
        let mut binds = String::new();
        let mut fields = Vec::new();
        for (position, reference) in evaluation.inputs.iter().enumerate() {
            let field = format!("v{position}");
            input["values"][&field] = Value::Null;
            write_bind(&mut binds, &format!("/values/{field}"), reference, plan);
            fields.push(field);
        }
        let mut predicates = Vec::new();
        match &evaluation.criterion {
            EvaluationCriterion::Equals { expected } => {
                if fields.len() != 1 {
                    return Err(invalid(format!(
                        "{} equality requires one input",
                        requirement.id
                    )));
                }
                predicates.push(format!(
                    "equal(values.v0, {})",
                    serde_json::to_string(expected)?
                ));
            }
            EvaluationCriterion::Numeric {
                comparison,
                threshold,
                value_unit,
                threshold_unit,
            } => {
                if fields.len() != 1 || value_unit != threshold_unit {
                    return Err(invalid(format!(
                        "{} needs one numeric input and an explicit same-unit comparison",
                        requirement.id
                    )));
                }
                let operator = match comparison {
                    eal_collector_contracts::execution::NumericComparison::LessThan => "<",
                    eal_collector_contracts::execution::NumericComparison::AtMost => "<=",
                    eal_collector_contracts::execution::NumericComparison::Equal => "==",
                    eal_collector_contracts::execution::NumericComparison::AtLeast => ">=",
                    eal_collector_contracts::execution::NumericComparison::GreaterThan => ">",
                };
                predicates.push(format!(
                    "exact_compare(values.v0, {}, {}, {})",
                    serde_json::to_string(threshold)?,
                    quoted(operator),
                    quoted(value_unit)
                ));
            }
            EvaluationCriterion::AllTrue | EvaluationCriterion::AnyTrue => {
                if fields.is_empty() {
                    return Err(invalid("Boolean support requires acquired inputs"));
                }
                input = json!({"values":{},"truth_values":vec![Value::Null;fields.len()]});
                binds.clear();
                for (position, reference) in evaluation.inputs.iter().enumerate() {
                    write_bind(
                        &mut binds,
                        &format!("/truth_values/{position}"),
                        reference,
                        plan,
                    );
                }
                let function = if matches!(evaluation.criterion, EvaluationCriterion::AllTrue) {
                    "all_values"
                } else {
                    "any_values"
                };
                predicates.push(format!("{function}(truth_values)"));
            }
            EvaluationCriterion::Judgement { criterion, scope } => {
                if fields.len() == 1 && scope.is_some() {
                    let checked = format!("{declaration}_scoped_review");
                    let mut review_bind = String::new();
                    write_bind(&mut review_bind, "/review", &evaluation.inputs[0], plan);
                    write_derived(
                        &mut compiled.declarations,
                        DerivedSpec {
                            name: &checked,
                            kind: "scoped_review",
                            max_age: age,
                            input: &json!({"criterion":criterion,"target":requirement.target_id,"scope":scope,"review":null}),
                            bindings: &review_bind,
                            expression: "review",
                            predicates: &["verdict == \"supported\"".into()],
                        },
                    )?;
                    input = json!({"values":{"v0":null}});
                    binds = format!("  bind \"/values/v0\" from {checked} \"\"\n");
                    predicates.extend([
                        "values.v0.verdict == \"supported\"".into(),
                        format!("values.v0.criterion == {}", quoted(criterion)),
                        format!(
                            "equal(values.v0.scope, {})",
                            serde_json::to_string(scope.as_ref().unwrap())?
                        ),
                        format!("values.v0.target == {}", quoted(&requirement.target_id)),
                        "values.v0.reviewer != \"\"".into(),
                        "values.v0.reviewed_at != \"\"".into(),
                        "count(values.v0.reasons) > 0".into(),
                    ]);
                } else {
                    emit_review(
                        &mut compiled.declarations,
                        &mut compiled.review_requests,
                        &declaration,
                        requirement,
                        age,
                    )?;
                    let reason = if scope.is_none() {
                        "unscoped judgement remains unresolved; author the exact Judgement contract scope before a review can discharge it"
                    } else if fields.is_empty() {
                        "scoped judgement observation required at execution through engarg_review_contract; this static export obligation is not a runtime assessment"
                    } else {
                        "scoped judgement needs one acquired review input; multiple inputs do not identify a single review record, so engarg_review_contract remains required at execution"
                    };
                    compiled
                        .pending
                        .push(format!("{}: {reason}", requirement.id));
                    continue;
                }
            }
            EvaluationCriterion::Unavailable { reason } => {
                emit_review(
                    &mut compiled.declarations,
                    &mut compiled.review_requests,
                    &declaration,
                    requirement,
                    age,
                )?;
                compiled
                    .pending
                    .push(format!("{}: {reason}", requirement.id));
                continue;
            }
        }
        for (role, references, expected) in [
            ("applicability", &evaluation.applicability, true),
            ("challenge", &evaluation.challenges, false),
            ("coverage", &evaluation.coverage, true),
        ] {
            for (position, reference) in references.iter().enumerate() {
                let field = format!("{role}{position}");
                input["values"][&field] = Value::Null;
                write_bind(&mut binds, &format!("/values/{field}"), reference, plan);
                predicates.push(format!("values.{field} == {expected}"));
            }
        }
        let missing_coverage = matches!(
            evaluation.required_coverage,
            RequiredCoverage::CompleteEnumeration | RequiredCoverage::Statistical
        ) && evaluation.coverage.is_empty();
        if missing_coverage {
            // Coverage is an explicit proof obligation. Neither a field name,
            // a successful call nor a provider's self-description discharges it.
            compiled.pending.push(format!(
                "{}: {:?} requires explicit coverage evidence",
                requirement.id, evaluation.required_coverage
            ));
        }
        let decision = predicates
            .iter()
            .map(|predicate| format!("({predicate})"))
            .collect::<Vec<_>>()
            .join(" and ");
        let decision = if missing_coverage {
            "pending.required_coverage".into()
        } else {
            decision
        };
        write_derived(
            &mut compiled.declarations,
            DerivedSpec {
                name: &declaration,
                kind: "calculation",
                max_age: age,
                input: &input,
                bindings: &binds,
                expression: &format!("object(\"accepted\", {decision})"),
                predicates: &["accepted == true".into()],
            },
        )?;
    }
    // A target's conjunction composes already evaluated obligations; it does
    // not ask a second reviewer to repeat the same judgement.
    for (prefix, targets) in [
        ("target_support", &mut compiled.targets),
        ("challenge_activation", &mut compiled.challenge_activation),
        ("challenge_resolution", &mut compiled.challenge_resolution),
    ] {
        for (target, requirements) in targets {
            if requirements.len() <= 1 {
                continue;
            }
            let declaration = name(prefix, target);
            let mut input = json!({"values":{}});
            let mut binds = String::new();
            let mut checks = Vec::new();
            for (index, requirement) in requirements.iter().enumerate() {
                let field = format!("r{index}");
                input["values"][&field] = Value::Null;
                binds.push_str(&format!(
                    "  bind \"/values/{field}\" from {requirement} \"/accepted\"\n"
                ));
                checks.push(format!("values.{field} == true"));
            }
            write_derived(
                &mut compiled.declarations,
                DerivedSpec {
                    name: &declaration,
                    kind: "calculation",
                    max_age: policy.review_max_age_seconds,
                    input: &input,
                    bindings: &binds,
                    expression: &format!("object(\"accepted\", {})", checks.join(" and ")),
                    predicates: &["accepted == true".into()],
                },
            )?;
            *requirements = vec![declaration];
        }
    }
    for alternative in &elaboration.alternative_support {
        for claim in &alternative.claim_ids {
            let target = compiled.targets.entry(claim.clone()).or_default();
            for requirement in &alternative.requirement_ids {
                let evidence = compiled
                    .requirements
                    .get(requirement)
                    .ok_or_else(|| invalid("alternative names an absent requirement"))?;
                if !target.contains(evidence) {
                    target.push(evidence.clone());
                }
            }
        }
    }
    Ok(())
}

fn emit_review(
    source: &mut String,
    requests: &mut Vec<crate::eal_export::ReviewRequest>,
    declaration: &str,
    requirement: &crate::support_requirements::SupportRequirement,
    age: u64,
) -> Result<(), Error> {
    let review = format!("{declaration}_review");
    let criterion = match &requirement.acceptance {
        EvaluationCriterion::Judgement { criterion, .. } => criterion.as_str(),
        _ => requirement.acceptance_rationale.as_str(),
    };
    // A missing empirical source and an unscoped request cannot be replaced
    // by an affirmative review. Only an authored scoped judgement is eligible.
    let (scope, eligible) = match &requirement.acceptance {
        EvaluationCriterion::Judgement {
            scope: Some(scope), ..
        } => (serde_json::to_value(scope)?, true),
        _ => (Value::Null, false),
    };
    let input = json!({"criterion":criterion, "target":requirement.target_id,"role":requirement.role,"scope":scope,"requirement":requirement.id,"acceptance":requirement.acceptance});
    requests.push(crate::eal_export::ReviewRequest {
        evidence_id: review.clone(),
        environment: ENVIRONMENT.into(),
        tool: "engarg_review_contract".into(),
        tool_version: "1".into(),
        input: input.clone(),
        context: json!({}),
    });
    source.push_str(&format!("evidence {review} {{\n  tool engarg_review_contract\n  kind review\n  environment {ENVIRONMENT}\n  max_age {age}\n  input {}\n  require true\n}}\n\n", serde_json::to_string(&input)?));
    let raw_binds = format!("  bind \"/review\" from {review} \"\"\n");
    let binds = if eligible {
        let checked = format!("{declaration}_scoped_review");
        write_derived(
            source,
            DerivedSpec {
                name: &checked,
                kind: "scoped_review",
                max_age: age,
                input: &json!({"criterion":criterion,"target":requirement.target_id,"scope":scope,"review":null}),
                bindings: &raw_binds,
                expression: "review",
                predicates: &["verdict == \"supported\"".into()],
            },
        )?;
        format!("  bind \"/review\" from {checked} \"\"\n")
    } else {
        raw_binds
    };
    let expression = format!(
        "object(\"accepted\", review.verdict == \"supported\" and review.criterion == {} and equal(review.scope, {}) and review.target == {} and review.reviewer != \"\" and review.reviewed_at != \"\" and count(review.reasons) > 0)",
        quoted(criterion),
        serde_json::to_string(&scope)?,
        quoted(&requirement.target_id)
    );
    let expression = if eligible {
        expression
    } else {
        "object(\"accepted\", pending.required_observation)".into()
    };
    write_derived(
        source,
        DerivedSpec {
            name: declaration,
            kind: "calculation",
            max_age: age,
            input: &json!({"review":null}),
            bindings: &binds,
            expression: &expression,
            predicates: &["accepted == true".into()],
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use eal_collector_contracts::execution::{OutputReference, RequirementEvaluation};
    use eal_core::{ExecutionLimits, composition, expressions::evaluate_expression};

    fn elaboration(criterion: EvaluationCriterion) -> SupportElaboration {
        serde_json::from_value(json!({
            "schema":crate::support_requirements::SUPPORT_ELABORATION_SCHEMA,
            "requirements":[{"id":"R","target_id":"G","evidence_need_ids":[],
                "original_acceptance_criteria":[],"contract":null,"discriminating_evidence":[],
                "acceptance":criterion,"acceptance_rationale":"Exact test obligation","gaps":[]}],
            "acquisitions":[],"derivations":[],"evaluations":[],"revisions":[]
        }))
        .unwrap()
    }
    fn evaluation(criterion: EvaluationCriterion) -> RequirementEvaluation {
        serde_json::from_value(json!({"id":"E","requirement_id":"R",
            "inputs":[{"step_id":"source","pointer":"/value"}],"criterion":criterion}))
        .unwrap()
    }
    fn lower(
        elaboration: &SupportElaboration,
        evaluation: RequirementEvaluation,
    ) -> CompiledEvidence {
        let mut result = CompiledEvidence::default();
        compile(
            elaboration,
            &ProgramPlan {
                acquisitions: vec![],
                derivations: vec![],
                evaluations: vec![evaluation],
            },
            &ExportPolicy::default(),
            &mut result,
        )
        .unwrap();
        result
    }
    fn result(
        compiled: &CompiledEvidence,
        input: Value,
    ) -> eal_core::expressions::ExpressionResult {
        let source = format!(
            "language \"EAL/4\"\nenvironment engarg_case {{\n require true\n}}\ntool engarg_review_contract {{\n version \"1\"\n}}\ntool fixture {{\n version \"1\"\n}}\nevidence derivation_736f75726365 {{\n tool fixture\n kind observation\n environment engarg_case\n max_age 300\n input {{}}\n require true\n}}\n{}",
            compiled.declarations
        );
        let program = composition::compile(&source, None, "", ExecutionLimits::default()).unwrap();
        let expression = program.evidence[&name("requirement", "R")]
            .derivation
            .as_ref()
            .unwrap();
        evaluate_expression(expression, &input)
    }

    #[test]
    fn complete_coverage_requires_an_explicit_dependency_not_a_pointer_name() {
        let criterion = EvaluationCriterion::Equals { expected: json!(3) };
        let elaboration = elaboration(criterion.clone());
        let mut evaluation = evaluation(criterion);
        evaluation.required_coverage = RequiredCoverage::CompleteEnumeration;
        evaluation.applicability.push(OutputReference {
            step_id: "source".into(),
            pointer: "/complete".into(),
        });
        let unproved = lower(&elaboration, evaluation.clone());
        let absent = result(&unproved, json!({"values":{"v0":3,"applicability0":true}}));
        assert!(absent.value.is_none());
        assert_eq!(absent.issue.as_deref(), Some("missing_field"));
        evaluation.coverage.push(OutputReference {
            step_id: "source".into(),
            pointer: "/independent_check".into(),
        });
        let proved = lower(&elaboration, evaluation);
        assert_eq!(
            result(
                &proved,
                json!({"values":{"v0":3,"applicability0":true,"coverage0":true}})
            )
            .value,
            Some(json!({"accepted":true}))
        );
        assert_eq!(
            result(
                &proved,
                json!({"values":{"v0":3,"applicability0":true,"coverage0":false}})
            )
            .value,
            Some(json!({"accepted":false}))
        );
    }

    #[test]
    fn affirmative_review_cannot_replace_missing_empirical_evidence_or_scope() {
        for criterion in [
            EvaluationCriterion::Unavailable {
                reason: "Instrument missing".into(),
            },
            EvaluationCriterion::Judgement {
                criterion: "Adequacy".into(),
                scope: None,
            },
        ] {
            let elaboration = elaboration(criterion.clone());
            let mut evaluation = evaluation(criterion);
            evaluation.inputs.clear();
            let compiled = lower(&elaboration, evaluation);
            let request = &compiled.review_requests[0].input;
            let review = json!({"verdict":"supported","criterion":request["criterion"],
                "scope":request["scope"],"target":request["target"],
                "reviewer":"Fixture reviewer","reasons":["An affirmative review is not a measurement."]});
            let absent = result(&compiled, json!({"review":review}));
            assert!(absent.value.is_none());
            assert_eq!(absent.issue.as_deref(), Some("missing_field"));
        }
    }

    #[test]
    fn review_diagnostics_distinguish_static_collection_from_missing_scope() {
        for scoped in [false, true] {
            let scope = scoped.then(|| {
                serde_json::from_value(json!({
                    "kind":"synthetic_review","subject":"Exact fixture judgement",
                    "selectors":{"revision":"fixture-v1"},"identity_dimensions":["revision"]
                }))
                .unwrap()
            });
            let criterion = EvaluationCriterion::Judgement {
                criterion: "Assess the frozen fixture criterion".into(),
                scope,
            };
            let elaboration = elaboration(criterion.clone());
            let mut evaluation = evaluation(criterion);
            evaluation.inputs.clear();
            let compiled = lower(&elaboration, evaluation);
            assert_eq!(compiled.review_requests.len(), 1);
            let diagnostic = &compiled.pending[0];
            if scoped {
                assert!(diagnostic.contains("scoped judgement observation required at execution"));
                assert!(diagnostic.contains("static export obligation"));
            } else {
                assert!(diagnostic.contains("unscoped judgement remains unresolved"));
                assert!(!diagnostic.contains("scoped judgement observation required"));
            }
            let request = &compiled.review_requests[0].input;
            for verdict in ["supported", "rejected", "inconclusive"] {
                let review = json!({"verdict":verdict,"criterion":request["criterion"],
                    "scope":request["scope"],"target":request["target"],
                    "reviewer":"Synthetic fixture reviewer",
                    "reasons":["Software diagnostic control only"],
                    "reviewed_at":"2026-10-28T08:00:00Z"});
                let accepted = result(&compiled, json!({"review":review}));
                if scoped {
                    assert_eq!(
                        accepted.value,
                        Some(json!({"accepted":verdict=="supported"}))
                    );
                } else {
                    assert!(accepted.value.is_none());
                }
            }
        }
    }

    #[test]
    fn multiple_review_inputs_leave_an_explicit_runtime_collection_obligation() {
        let criterion = EvaluationCriterion::Judgement {
            criterion: "Assess the frozen fixture criterion".into(),
            scope: Some(
                serde_json::from_value(json!({
                    "kind":"synthetic_review","subject":"Exact fixture judgement",
                    "selectors":{"revision":"fixture-v1"},"identity_dimensions":["revision"]
                }))
                .unwrap(),
            ),
        };
        let elaboration = elaboration(criterion.clone());
        let mut evaluation = evaluation(criterion);
        evaluation.inputs.push(OutputReference {
            step_id: "another_review".into(),
            pointer: "/value".into(),
        });
        let compiled = lower(&elaboration, evaluation);
        assert_eq!(compiled.review_requests.len(), 1);
        assert!(
            compiled.pending[0].contains("multiple inputs do not identify a single review record")
        );
        assert!(
            compiled.pending[0].contains("engarg_review_contract remains required at execution")
        );
    }
}
