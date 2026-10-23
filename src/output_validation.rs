//! Generation checks and retained-dissent checks used by adjudication and repair.

use serde::Serialize;

use crate::{
    argument_depth::ClaimDepths,
    domain::{ArgumentCase, Request, validate_case},
    provider::Role,
};

#[derive(Debug, Serialize)]
pub(crate) struct RetainedObjection {
    pub(crate) id: String,
    pub(crate) challenge: String,
    pub(crate) source_role: Role,
    /// Repairs cannot evade an existing obligation by downgrading materiality.
    pub(crate) must_remain_material: bool,
}

/// No corrective data is invented here. Errors go back to the model, whose full
/// replacement must pass exactly the same checks before it can be reported.
pub(crate) fn errors(
    request: &Request,
    case: &ArgumentCase,
    objections: &[RetainedObjection],
    max_depth: Option<u8>,
) -> Vec<String> {
    let mut errors = validate_case(request, case).err().unwrap_or_default();
    if errors.is_empty()
        && let Some(limit) = max_depth
    {
        let actual = ClaimDepths::new(case).maximum();
        if actual > usize::from(limit) {
            errors.push(format!(
                "argument depth is {actual} warrant layers, exceeding argument-depth {limit}; simplify decomposition while retaining conditions and evidence obligations"
            ));
        }
    }
    if case.grounds.is_empty()
        || !case.warrants.iter().any(|warrant| {
            matches!(
                warrant.conclusion_id.as_str(),
                "current_claim" | "stronger_target"
            )
        })
    {
        errors.push("generation must construct at least one argument from represented grounds through a warrant to current_claim or stronger_target; an assessment and evidence plan alone are insufficient".into());
    }
    for objection in objections {
        let retained = case.defeaters.iter().find(|defeater| {
            defeater.id == objection.id && defeater.challenge == objection.challenge
        });
        if retained.is_none() {
            errors.push(format!(
                "adjudicator omitted or altered retained objection {}",
                objection.id
            ));
        } else if objection.must_remain_material
            && retained.is_some_and(|defeater| !defeater.material)
        {
            errors.push(format!(
                "repair downgraded materiality of retained objection {}",
                objection.id
            ));
        }
    }
    errors
}
