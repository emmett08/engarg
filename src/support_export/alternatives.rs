//! Generated unavailable gates are separate from the authored legacy case/plan.
use super::{AlternativeGuard, AlternativeProjection};
use crate::{
    Error,
    domain::ArgumentCase,
    eal_export::name,
    support_requirements::{SupportElaboration, legacy_alternative_gaps},
};
use eal_collector_contracts::execution::{
    EvaluationCriterion, EvidenceExecutionPlan, RequirementEvaluation, canonical_digest,
};
use std::collections::BTreeSet;

pub(super) fn project(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    original: &EvidenceExecutionPlan,
) -> Result<(EvidenceExecutionPlan, Option<AlternativeProjection>), Error> {
    let gaps = legacy_alternative_gaps(case, elaboration);
    if gaps.is_empty() {
        return Ok((original.clone(), None));
    }
    let mut plan = original.clone();
    let mut ids: BTreeSet<_> = original
        .evaluations
        .iter()
        .flat_map(|evaluation| [evaluation.id.clone(), evaluation.requirement_id.clone()])
        .chain(original.acquisitions.iter().map(|step| step.id.clone()))
        .chain(original.derivations.iter().map(|step| step.id.clone()))
        .chain(
            elaboration
                .requirements
                .iter()
                .map(|requirement| requirement.id.clone()),
        )
        .collect();
    let mut guards = Vec::new();
    for gap in gaps {
        let unique = |base: String, ids: &mut BTreeSet<String>| {
            let mut candidate = base.clone();
            let mut ordinal = 0;
            while !ids.insert(candidate.clone()) {
                ordinal += 1;
                candidate = format!("{base}_{ordinal}");
            }
            candidate
        };
        let requirement_id = unique(
            name("legacy_alternative_requirement", &gap.alternative_id),
            &mut ids,
        );
        let evaluation_id = unique(
            name("legacy_alternative_evaluation", &gap.alternative_id),
            &mut ids,
        );
        let description = format!(
            "Legacy support extraction has no discriminating requirement for {}. Re-extract support requirements before relying on this claim. Distinguishing test: {}",
            gap.alternative_id, gap.distinguishing_test
        );
        plan.evaluations.push(RequirementEvaluation {
            id: evaluation_id.clone(),
            requirement_id: requirement_id.clone(),
            inputs: vec![],
            criterion: EvaluationCriterion::Unavailable {
                reason: description.clone(),
            },
            max_age_seconds: None,
            required_coverage: Default::default(),
            applicability: vec![],
            challenges: vec![],
        });
        guards.push(AlternativeGuard {
            alternative_id: gap.alternative_id.clone(),
            requirement_id,
            evaluation_id,
            evidence_name: name("legacy_alternative", &gap.alternative_id),
            claim_ids: gap.claim_ids,
            description,
        });
    }
    plan.validate().map_err(Error::InvalidInput)?;
    Ok((
        plan,
        Some(AlternativeProjection {
            schema: "engarg.alternative-projection/1".into(),
            original_execution_plan: original.clone(),
            original_execution_plan_sha256: canonical_digest(original)
                .map_err(Error::InvalidInput)?,
            guards,
        }),
    ))
}
