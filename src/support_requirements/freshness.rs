//! Shared freshness readiness for EAL/3 evidence evaluations.
use super::{SupportElaboration, SupportRequirement};
use eal_collector_contracts::execution::EvaluationCriterion;

/// The same rule is used by EAL export and the pre-implementation check.
pub fn requirement_age_seconds(
    requirement: &SupportRequirement,
    maximum: Option<u64>,
    has_inputs: bool,
) -> Result<u64, String> {
    match maximum {
        Some(age) if (1..=315_360_000).contains(&age) => Ok(age),
        None if matches!(
            requirement.acceptance,
            EvaluationCriterion::Unavailable { .. }
        ) || matches!(&requirement.acceptance, EvaluationCriterion::Judgement { scope, .. } if scope.is_none() || !has_inputs) =>
        {
            Ok(0)
        }
        _ => Err(format!(
            "SUPPORT_FRESHNESS_CLARIFICATION: requirement {} needs an explicit positive bounded freshness contract before EAL/3 can collect it",
            requirement.id
        )),
    }
}

/// A consumer's age bound also limits every original observation in its lineage.
/// Independently specified source bounds remain in force and can be stricter.
pub fn validate_freshness(elaboration: &SupportElaboration) -> Vec<String> {
    let mut errors = Vec::new();
    for evaluation in &elaboration.evaluations {
        let Some(requirement) = elaboration
            .requirements
            .iter()
            .find(|r| r.id == evaluation.requirement_id)
        else {
            continue;
        };
        if evaluation.max_age_seconds
            != requirement
                .contract
                .as_ref()
                .and_then(|contract| contract.time.max_age_seconds)
        {
            errors.push(format!("SUPPORT_FRESHNESS_CLARIFICATION: evaluation {} does not match requirement {} freshness", evaluation.id, requirement.id));
        }
        if let Err(error) = requirement_age_seconds(
            requirement,
            evaluation.max_age_seconds,
            !evaluation.inputs.is_empty(),
        ) {
            errors.push(error);
        }
    }
    errors
}
