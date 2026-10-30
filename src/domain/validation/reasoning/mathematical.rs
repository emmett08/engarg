//! Admit only explicitly closed native mathematical consequences without sources.
use crate::domain::{
    BoundReasoningMethod, MethodComparison, MethodScalar, ReasoningMethodKind, Warrant,
};

pub(super) fn validate(warrant: &Warrant, bound: &BoundReasoningMethod, errors: &mut Vec<String>) {
    let label = format!("warrant {} mathematical consequence", warrant.id);
    if bound.conclusion.scope.kind != "mathematical_model" {
        errors.push(format!(
            "{label} needs explicit mathematical_model scope; a physical scope cannot become a mathematical proof"
        ));
    }
    if !warrant.premise_ids.is_empty()
        || !warrant.backing_evidence_ids.is_empty()
        || !bound.formal_premises.is_empty()
        || !bound.evidence_requirements.is_empty()
        || !bound.assumption_ids.is_empty()
    {
        errors.push(format!(
            "{label} must be a closed authored query without ground premises, source observations or empirical applicability assumptions"
        ));
    }
    let path = match bound.method.kind() {
        Some(ReasoningMethodKind::Deductive) => {
            if !bound.method.query["premises"]
                .as_array()
                .is_some_and(Vec::is_empty)
            {
                errors.push(format!(
                    "{label} deductive query must have zero premises; express a model consequence as a closed implication theorem"
                ));
            }
            "entailed"
        }
        Some(ReasoningMethodKind::FiniteProbability) => "satisfies",
        _ => {
            errors.push(format!(
                "{label} requires native deductive/1 or finite_probability/1, never structured or empirical method support"
            ));
            return;
        }
    };
    if bound.conclusion.result.path != path
        || !matches!(bound.conclusion.result.operator, MethodComparison::Equal)
        || !matches!(
            bound.conclusion.result.expected,
            MethodScalar::Boolean(true)
        )
    {
        errors.push(format!(
            "{label} must require the exact native {path} == true result"
        ));
    }
}
