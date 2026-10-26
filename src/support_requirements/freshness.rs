//! Admit the evidence ages required by both support planning and EAL/3 export.
use super::{SupportElaboration, SupportRequirement};
use eal_collector_contracts::execution::EvaluationCriterion;
use std::collections::BTreeMap;

/// Machine evidence needs an explicit bounded age. Pending stubs emit no
/// observations, so their absent bound is represented by EAL's zero-age guard.
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
        ) || matches!(&requirement.acceptance, EvaluationCriterion::Judgement { scope, .. }
            if scope.is_none() || !has_inputs) =>
        {
            Ok(0)
        }
        _ => Err(format!(
            "SUPPORT_FRESHNESS_CLARIFICATION: requirement {} needs an explicit positive bounded freshness contract before EAL/3 can collect it; set the same max_age_seconds in its requirement contract and evaluation, or repair retained support with eal repair",
            requirement.id
        )),
    }
}

/// Check freshness without requiring provider discovery or a generated package.
pub fn validate_freshness(elaboration: &SupportElaboration) -> Vec<String> {
    let mut errors = Vec::new();
    let requirements = elaboration
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect::<BTreeMap<_, _>>();
    for evaluation in &elaboration.evaluations {
        let Some(requirement) = requirements.get(evaluation.requirement_id.as_str()) else {
            // Full graph admission diagnoses unknown requirement references.
            continue;
        };
        if evaluation.max_age_seconds
            != requirement
                .contract
                .as_ref()
                .and_then(|contract| contract.time.max_age_seconds)
        {
            errors.push(format!(
                "evaluation {} changed its requirement freshness",
                evaluation.id
            ));
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
