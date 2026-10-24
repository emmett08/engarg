//! Evidence references, text obligations and investigation-plan adequacy policy.

use super::*;

pub(super) fn validate(
    _request: &Request,
    case: &ArgumentCase,
    context: &Context<'_>,
    index: &ArgumentIndex<'_>,
    errors: &mut Vec<String>,
) {
    for (name, text) in [
        ("current_claim.text", &case.current_claim.text),
        ("current_claim.qualifier", &case.current_claim.qualifier),
        ("stronger_target.text", &case.stronger_target.text),
        ("stronger_target.qualifier", &case.stronger_target.qualifier),
        ("scope.system_boundary", &case.scope.system_boundary),
        ("scope.configuration", &case.scope.configuration),
        (
            "scope.operating_conditions",
            &case.scope.operating_conditions,
        ),
        ("scope.units", &case.scope.units),
        ("scope.time_horizon", &case.scope.time_horizon),
        ("scope.consequences", &case.scope.consequences),
        ("adjudication", &case.adjudication),
    ] {
        nonempty(errors, name, text);
    }
    if case.decision_limits.is_empty() {
        errors.push("decision_limits must identify at least one limit on reliance".into());
    }
    for text in case.decision_limits.iter().chain(&case.residual_risks) {
        nonempty(errors, "risk or decision limit", text);
    }
    for need in &case.evidence_plan {
        if !(1..=5).contains(&need.priority) {
            errors.push(format!("evidence plan {} priority must be 1–5", need.id));
        }
        for (name, value) in [
            ("question", &need.question),
            ("method", &need.method),
            ("measurement", &need.measurement),
            ("comparator", &need.comparator),
            ("acceptance_criterion", &need.acceptance_criterion),
            ("if_failed", &need.if_failed),
            ("enables_claim", &need.enables_claim),
            ("cost", &need.cost),
        ] {
            nonempty(errors, &format!("evidence plan {} {name}", need.id), value);
        }
    }
    for subclaim in &case.subclaims {
        nonempty(
            errors,
            &format!("subclaim {} text", subclaim.id),
            &subclaim.text,
        );
        nonempty(
            errors,
            &format!("subclaim {} qualifier", subclaim.id),
            &subclaim.qualifier,
        );
    }
    for ground in &case.grounds {
        nonempty(
            errors,
            &format!("ground {} statement", ground.id),
            &ground.statement,
        );
        if ground.status == GroundStatus::Reported && ground.evidence_ids.is_empty() {
            errors.push(format!(
                "reported ground {} needs supplied evidence",
                ground.id
            ));
        }
        if ground.status != GroundStatus::Reported
            && !context.planned_targets.contains(ground.id.as_str())
        {
            errors.push(format!("unmet ground {} needs an evidence plan", ground.id));
        }
    }
    for warrant in &case.warrants {
        nonempty(
            errors,
            &format!("warrant {} rule", warrant.id),
            &warrant.rule,
        );
        nonempty(
            errors,
            &format!("warrant {} applicability", warrant.id),
            &warrant.applicability,
        );
    }
    for assumption in &case.assumptions {
        nonempty(
            errors,
            &format!("assumption {} statement", assumption.id),
            &assumption.statement,
        );
        nonempty(
            errors,
            &format!("assumption {} validation", assumption.id),
            &assumption.validation,
        );
        if matches!(
            assumption.status,
            AssumptionStatus::Supported | AssumptionStatus::Contradicted
        ) && assumption.evidence_ids.is_empty()
        {
            errors.push(format!(
                "supported or contradicted assumption {} needs supplied evidence",
                assumption.id
            ));
        }
        if assumption.critical
            && assumption.status != AssumptionStatus::Supported
            && !context.planned_targets.contains(assumption.id.as_str())
        {
            errors.push(format!(
                "critical assumption {} needs an evidence plan",
                assumption.id
            ));
        }
    }
    for defeater in &case.defeaters {
        nonempty(
            errors,
            &format!("defeater {} challenge", defeater.id),
            &defeater.challenge,
        );
        nonempty(
            errors,
            &format!(
                "defeater {} response (including materiality rationale)",
                defeater.id
            ),
            &defeater.response,
        );
        let outcome_needs_evidence = matches!(
            defeater.status,
            DefeaterStatus::Sustained | DefeaterStatus::Refuted
        );
        let basis_needs_evidence = matches!(
            defeater.basis,
            DefeaterBasis::Observed | DefeaterBasis::SupportedPossibility
        ) && defeater.status
            != DefeaterStatus::InapplicableAfterRevision;
        if (outcome_needs_evidence || basis_needs_evidence) && defeater.evidence_ids.is_empty() {
            errors.push(format!(
                "observed, supported, sustained or refuted defeater {} needs supplied evidence",
                defeater.id
            ));
        }
        if defeater.material
            && matches!(
                defeater.status,
                DefeaterStatus::Open | DefeaterStatus::Sustained
            )
            && !context.planned_targets.contains(defeater.id.as_str())
        {
            errors.push(format!(
                "unresolved material defeater {} needs an evidence plan",
                defeater.id
            ));
        }
    }
    for alternative in &case.alternatives {
        nonempty(errors, "alternative explanation", &alternative.explanation);
        nonempty(
            errors,
            "alternative distinguishing_test",
            &alternative.distinguishing_test,
        );
    }
    // Group needs by enabling claim. One traversal serves every need for the
    // same root and its closure is released before the next root is visited.
    for claim in &context.claim_ids {
        let needs = index.needs_enabling(claim);
        if needs.is_empty() {
            continue;
        }
        let enabled = index.candidate_dependency(claim);
        for need in needs {
            if !enabled.contains(need.target_id.as_str()) {
                errors.push(format!(
                    "evidence plan {} target is outside enables_claim {:?} dependency closure",
                    need.id, need.enables_claim
                ));
            }
        }
    }
}
