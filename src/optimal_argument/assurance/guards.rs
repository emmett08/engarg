//! Preserve target meaning and required conditions across generated simplifications.
use super::*;
use crate::{
    domain::{ArgumentCase, ArgumentIndex, ArgumentNode},
    support_requirements::SupportRequirementRole,
};
use eal_collector_contracts::capability::SupportType;
use std::collections::BTreeSet;

pub(super) fn guard(id: &str, status: CheckStatus, reasons: Vec<String>) -> CheckResult {
    CheckResult {
        id: format!("guard:{id}"),
        role: CheckRole::Applicability,
        target_id: "current_claim".into(),
        status,
        reasons,
        details: json!({}),
    }
}

fn same(left: &impl Serialize, right: &impl Serialize) -> Result<bool, Error> {
    Ok(digest(left)? == digest(right)?)
}

pub(super) fn validate_discharge(
    original: &RunReport,
    check: &VerificationCheck,
    discharge: &Discharge,
) -> Result<(), Error> {
    if !matches!(
        check.procedure,
        CheckProcedure::Method { .. } | CheckProcedure::LogicalEquivalence { .. }
    ) || !matches!(check.role, CheckRole::Formalisation | CheckRole::Inference)
    {
        return Err(invalid(
            "only checked native formal procedures may discharge formal obligations",
        ));
    }
    let requirement = original
        .support_elaboration
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .find(|r| r.id == discharge.requirement_id)
        .ok_or_else(|| invalid("discharge names an unknown original obligation"))?;
    if digest(requirement)? != discharge.requirement_sha256
        || requirement.role != SupportRequirementRole::Support
    {
        return Err(invalid(
            "discharge must bind the exact original support requirement and cannot remove challenge checks",
        ));
    }
    if !requirement.contract.as_ref().is_some_and(|contract| {
        matches!(
            contract.support_type,
            SupportType::Judgement | SupportType::Calculation | SupportType::ProofCheck
        )
    }) {
        return Err(invalid(
            "a formal discharge cannot replace empirical observation, retrieval, experiment or unclassified evidence",
        ));
    }
    let index = ArgumentIndex::new(&original.argument);
    if matches!(
        index.node(&requirement.target_id),
        Some(ArgumentNode::Assumption(_) | ArgumentNode::Defeater(_))
    ) {
        return Err(invalid(
            "formal discharge cannot remove applicability assumptions or material challenges",
        ));
    }
    if check.target_id != requirement.target_id
        && !index
            .selected_dependency(&check.target_id)
            .contains(requirement.target_id.as_str())
    {
        return Err(invalid(
            "formal discharge is unrelated to the declared original argument target",
        ));
    }
    Ok(())
}

fn node_value(case: &ArgumentCase, id: &str) -> Option<Value> {
    let index = ArgumentIndex::new(case);
    match index.node(id)? {
        ArgumentNode::Root(v) => serde_json::to_value(v),
        ArgumentNode::Claim(v) => serde_json::to_value(v),
        ArgumentNode::Ground(v) => serde_json::to_value(v),
        ArgumentNode::Warrant(v) => serde_json::to_value(v),
        ArgumentNode::Assumption(v) => serde_json::to_value(v),
        ArgumentNode::Defeater(v) => serde_json::to_value(v),
    }
    .ok()
}

fn target_metadata(bound: &crate::domain::BoundReasoningMethod) -> Result<Value, Error> {
    let conclusion = &bound.conclusion;
    Ok(
        json!({"subject":conclusion.subject,"quantity":conclusion.quantity,
        "unit":conclusion.unit,"scope":conclusion.scope.identity().map_err(invalid)?,
        "valid_from":conclusion.valid_from,"valid_until":conclusion.valid_until}),
    )
}

fn passed(receipt: &AssuranceReceipt, id: &str) -> bool {
    receipt
        .checks
        .iter()
        .any(|result| result.id == id && result.status == CheckStatus::Pass)
}

pub(super) fn check(
    original: &RunReport,
    proposal: &SupportedArgument,
    plan: &AssurancePlan,
    receipt: &mut AssuranceReceipt,
) -> Result<(), Error> {
    let before = &original.argument;
    let after = &proposal.argument;
    let before_index = ArgumentIndex::new(before);
    let after_index = ArgumentIndex::new(after);
    let baseline = before_index.selected_dependency("current_claim");
    let selected = after_index.selected_dependency("current_claim");
    let prior = original.support_elaboration.as_ref().unwrap();
    let current = &proposal.support_elaboration;
    let discharged: BTreeSet<_> = receipt
        .discharged_requirement_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut violations = Vec::new();
    if !same(&before.current_claim, &after.current_claim)?
        || !same(&before.stronger_target, &after.stronger_target)?
        || !same(&before.scope, &after.scope)?
        || before.original_claim != after.original_claim
    {
        violations.push("Generated candidate changes the fixed claim, qualifier, scope, original claim or stronger target".into());
    }
    for requirement in &prior.requirements {
        let found = current.requirements.iter().find(|r| r.id == requirement.id);
        if !discharged.contains(requirement.id.as_str())
            && !found.is_some_and(|r| same(requirement, r).unwrap_or(false))
        {
            violations.push(format!(
                "Original obligation {} was removed or rewritten without a checked discharge",
                requirement.id
            ));
        }
    }
    for need in &before.evidence_plan {
        let requirements: Vec<_> = prior
            .requirements
            .iter()
            .filter(|r| r.evidence_need_ids.contains(&need.id))
            .collect();
        let licensed = !requirements.is_empty()
            && requirements
                .iter()
                .all(|r| discharged.contains(r.id.as_str()));
        if !licensed
            && !after
                .evidence_plan
                .iter()
                .find(|n| n.id == need.id)
                .is_some_and(|n| same(need, n).unwrap_or(false))
        {
            violations.push(format!(
                "Original evidence need {} changed its acceptance or meaning",
                need.id
            ));
        }
    }
    for acquisition in &prior.acquisitions {
        if !current
            .acquisitions
            .iter()
            .find(|a| a.id == acquisition.id)
            .is_some_and(|a| same(acquisition, a).unwrap_or(false))
        {
            violations.push(format!(
                "Original acquisition {} changed its inputs, source contract or operation",
                acquisition.id
            ));
        }
    }
    // Freshness, coverage and source-to-requirement mappings cannot be weakened
    // independently of the requirement declaration they implement.
    for evaluation in &prior.evaluations {
        if !discharged.contains(evaluation.requirement_id.as_str())
            && !current
                .evaluations
                .iter()
                .find(|e| e.id == evaluation.id)
                .is_some_and(|e| same(evaluation, e).unwrap_or(false))
        {
            violations.push(format!(
                "Original requirement evaluation {} changed",
                evaluation.id
            ));
        }
    }
    for derivation in &prior.derivations {
        if !current
            .derivations
            .iter()
            .find(|d| d.id == derivation.id)
            .is_some_and(|d| same(derivation, d).unwrap_or(false))
        {
            violations.push(format!("Original derivation {} changed", derivation.id));
        }
    }
    for assumption in &before.assumptions {
        if assumption.critical
            && baseline.contains(assumption.id.as_str())
            && (!selected.contains(assumption.id.as_str())
                || !after
                    .assumptions
                    .iter()
                    .find(|a| a.id == assumption.id)
                    .is_some_and(|a| same(assumption, a).unwrap_or(false)))
        {
            violations.push(format!(
                "Critical condition {} must remain unchanged and relied upon",
                assumption.id
            ));
        }
    }
    for defeater in &before.defeaters {
        if defeater.material
            && baseline.contains(defeater.id.as_str())
            && (!selected.contains(defeater.id.as_str())
                || !after
                    .defeaters
                    .iter()
                    .find(|d| d.id == defeater.id)
                    .is_some_and(|d| same(defeater, d).unwrap_or(false)))
        {
            violations.push(format!(
                "Material challenge {} must remain unchanged and attached to selected support",
                defeater.id
            ));
        }
    }
    for id in &baseline {
        if *id == "current_claim" || selected.contains(id) {
            continue;
        }
        let obligations: Vec<_> = prior
            .requirements
            .iter()
            .filter(|r| r.target_id == *id)
            .collect();
        if obligations.is_empty()
            || !obligations
                .iter()
                .all(|r| discharged.contains(r.id.as_str()))
        {
            violations.push(format!("Required dependency {id} became unreachable without checked discharge of every associated obligation"));
        }
    }
    for id in &selected {
        if matches!(
            before_index.node(id),
            Some(ArgumentNode::Ground(_) | ArgumentNode::Claim(_))
        ) && let (Some(old), Some(new)) = (node_value(before, id), node_value(after, id))
            && !same(&old, &new)?
        {
            let specified = plan.checks.iter().any(|check| {
                let CheckProcedure::Value { expected, .. } = &check.procedure else {
                    return false;
                };
                check.role == CheckRole::Formalisation
                    && check.target_id == *id
                    && same(expected, &new).unwrap_or(false)
                    && receipt
                        .checks
                        .iter()
                        .any(|result| result.id == check.id && result.status == CheckStatus::Pass)
            });
            if !specified {
                violations.push(format!("Rewritten premise {id} changes its proposition without an independent complete formalisation reference"));
            }
        }
        let unchanged = node_value(before, id)
            .zip(node_value(after, id))
            .is_some_and(|(a, b)| same(&a, &b).unwrap_or(false));
        if unchanged {
            continue;
        }
        let role_coverage = plan.checks.iter().any(|check| {
            (check.target_id == *id
                || after_index
                    .warrant(id)
                    .is_some_and(|w| check.target_id == w.conclusion_id))
                && match after_index.node(id) {
                    Some(
                        ArgumentNode::Ground(_)
                        | ArgumentNode::Assumption(_)
                        | ArgumentNode::Defeater(_),
                    ) => matches!(check.procedure, CheckProcedure::Requirement { .. }),
                    Some(
                        ArgumentNode::Claim(_) | ArgumentNode::Root(_) | ArgumentNode::Warrant(_),
                    ) => check.role == CheckRole::Formalisation,
                    None => false,
                }
        });
        if !role_coverage {
            violations.push(format!("New or rewritten selected dependency {id} lacks an independent premise/formalisation check"));
        }
    }
    for warrant in after
        .warrants
        .iter()
        .filter(|w| selected.contains(w.id.as_str()))
    {
        let original_bindings: Vec<_> = before_index
            .composition_for(&warrant.conclusion_id)
            .into_iter()
            .flat_map(|composition| &composition.selected_warrant_ids)
            .filter_map(|id| before_index.warrant(id))
            .map(|w| &w.reasoning_method)
            .collect();
        let inherited_binding = original_bindings
            .iter()
            .any(|binding| same(*binding, &warrant.reasoning_method).unwrap_or(false));
        let explicit_binding = plan.checks.iter().any(|check| {
            let CheckProcedure::Value { pointer, expected } = &check.procedure else {
                return false;
            };
            check.role == CheckRole::Formalisation
                && check.target_id == warrant.conclusion_id
                && pointer.ends_with("/reasoning_method")
                && same(expected, &warrant.reasoning_method).unwrap_or(false)
                && passed(receipt, &check.id)
        });
        let checked_binding = plan.checks.iter().any(|check| {
            check.target_id == warrant.conclusion_id
                && matches!(
                    check.procedure,
                    CheckProcedure::Method { .. } | CheckProcedure::LogicalEquivalence { .. }
                )
                && passed(receipt, &check.id)
        });
        // A reference to an unrelated field cannot authorise changing the
        // executable formalisation. Check this before extracting a bound
        // method, so removing or downgrading a native binding is covered too.
        if !inherited_binding && !explicit_binding && !checked_binding {
            violations.push(format!("Selected warrant {} changes its reasoning method binding without a successful native formal check or independent complete method reference",warrant.id));
        }
        let Some(bound) = warrant.reasoning_method.as_ref().and_then(|m| m.bound()) else {
            continue;
        };
        let original_targets: Vec<_> = before_index
            .composition_for(&warrant.conclusion_id)
            .into_iter()
            .flat_map(|composition| &composition.selected_warrant_ids)
            .filter_map(|id| before_index.warrant(id))
            .filter_map(|w| w.reasoning_method.as_ref().and_then(|m| m.bound()))
            .map(target_metadata)
            .collect::<Result<_, _>>()?;
        let actual_target = target_metadata(bound)?;
        let inherited_target = !original_targets.is_empty()
            && original_targets
                .iter()
                .all(|value| same(value, &actual_target).unwrap_or(false));
        let explicit_target = plan.checks.iter().any(|check| {
            let CheckProcedure::Value { pointer, expected } = &check.procedure else {
                return false;
            };
            check.role == CheckRole::Formalisation
                && check.target_id == warrant.conclusion_id
                && pointer.ends_with("/conclusion")
                && same(expected, &bound.conclusion).unwrap_or(false)
                && receipt
                    .checks
                    .iter()
                    .any(|result| result.id == check.id && result.status == CheckStatus::Pass)
        });
        if !inherited_target && !explicit_target && !explicit_binding {
            violations.push(format!("Selected typed warrant {} changes subject, quantity, unit, scope or validity interval without an independent complete conclusion reference",warrant.id));
        }
        let original_results: Vec<_> = original_bindings
            .iter()
            .filter_map(|binding| binding.as_ref().and_then(|binding| binding.bound()))
            .map(|binding| &binding.conclusion.result)
            .collect();
        let inherited_result = !original_results.is_empty()
            && original_results
                .iter()
                .all(|result| same(*result, &bound.conclusion.result).unwrap_or(false));
        let checked_result = plan.checks.iter().any(|check| {
            check.target_id == warrant.conclusion_id
                && matches!(check.procedure, CheckProcedure::Method { .. })
                && passed(receipt, &check.id)
        });
        // Formula equivalence alone says nothing about whether the candidate
        // now asks the native method to establish truth or falsity of it.
        if !inherited_result && !checked_result && !explicit_target && !explicit_binding {
            violations.push(format!("Selected typed warrant {} changes its declared result predicate without an independent exact result or complete conclusion reference",warrant.id));
        }
        let original_bindings = before_index
            .composition_for(&warrant.conclusion_id)
            .into_iter()
            .flat_map(|composition| &composition.selected_warrant_ids)
            .filter_map(|id| before_index.warrant(id))
            .filter_map(|w| w.reasoning_method.as_ref().and_then(|m| m.bound()));
        let inherited = original_bindings.into_iter().any(|original| {
            same(&original.formal_premises, &bound.formal_premises).unwrap_or(false)
        });
        let explicit = plan.checks.iter().any(|check| {
            let CheckProcedure::Value { pointer, expected } = &check.procedure else {
                return false;
            };
            check.role == CheckRole::Formalisation
                && check.target_id == warrant.conclusion_id
                && pointer.ends_with("/formal_premises")
                && same(expected, &bound.formal_premises).unwrap_or(false)
                && receipt
                    .checks
                    .iter()
                    .any(|result| result.id == check.id && result.status == CheckStatus::Pass)
        });
        if !inherited && !explicit && !explicit_binding {
            violations.push(format!("Selected typed warrant {} changes formal input correspondences (symbol-to-premise bindings) without an independent complete binding reference",warrant.id));
        }
    }
    receipt.checks.push(guard("meaning_and_conditions", if violations.is_empty(){CheckStatus::Pass}else{CheckStatus::Fail},
        if violations.is_empty(){vec!["Fixed target, empirical criteria, required conditions and checked discharge boundaries preserved".into()]}else{violations}));
    Ok(())
}
