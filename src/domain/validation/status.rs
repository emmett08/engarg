//! Conservative reported status for the selected current-claim dependency.

use super::*;

pub(super) fn validate(
    case: &ArgumentCase,
    context: &Context<'_>,
    index: &ArgumentIndex<'_>,
    errors: &mut Vec<String>,
) {
    let current_closure = index.selected_dependency("current_claim");
    if case.status == CaseStatus::ProvisionallySupported {
        if index.warrants_for("current_claim").is_empty() {
            errors.push("provisional support requires a warrant concluding current_claim".into());
        }
        for claim_id in context
            .claim_ids
            .iter()
            .filter(|id| current_closure.contains(**id))
        {
            if !index
                .composition_for(claim_id)
                .is_some_and(|c| !c.selected_warrant_ids.is_empty())
            {
                errors.push(format!(
                    "provisional support requires a selected warrant for reached claim {claim_id:?}"
                ));
            }
        }
        let grounds: Vec<&Ground> = case
            .grounds
            .iter()
            .filter(|g| current_closure.contains(g.id.as_str()))
            .collect();
        if grounds.is_empty() {
            errors.push("provisional support requires ground support in the current claim dependency closure".into());
        }
        if grounds.iter().any(|g| g.status != GroundStatus::Reported) {
            errors.push("provisional support cannot depend on missing or assumed grounds".into());
        }
        if !grounds.iter().flat_map(|g| &g.evidence_ids).any(|id| {
            context
                .known_evidence
                .get(id.as_str())
                .is_some_and(|e| e.checked_by_user)
        }) {
            errors.push("provisional support requires at least one current ground backed by user-checked evidence".into());
        }
        if case.assumptions.iter().any(|a| {
            current_closure.contains(a.id.as_str())
                && a.critical
                && a.status != AssumptionStatus::Supported
        }) {
            errors.push("provisional support cannot retain untested or contradicted critical assumptions in its dependency closure".into());
        }
        if case.defeaters.iter().any(|d| {
            current_closure.contains(d.id.as_str())
                && d.material
                && matches!(d.status, DefeaterStatus::Open | DefeaterStatus::Sustained)
        }) {
            errors.push("provisional support cannot retain open or sustained material defeaters in its dependency closure, including hypothetical concerns".into());
        }
    }
    if case.status == CaseStatus::Refuted
        && !case.defeaters.iter().any(|d| {
            d.kind == DefeaterKind::Rebutting
                && d.status == DefeaterStatus::Sustained
                && d.target_id == "current_claim"
                && d.material
                && !d.evidence_ids.is_empty()
        })
    {
        errors.push("refuted requires a sustained, evidence-backed rebuttal targeting current_claim exactly; missing support alone is insufficient".into());
    }
    if matches!(
        case.status,
        CaseStatus::Unsupported | CaseStatus::Conditional
    ) && case.evidence_plan.is_empty()
    {
        errors.push("unsupported or conditional cases need an evidence plan".into());
    }
}
