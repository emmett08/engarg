//! Bind operator applicability expectations to the original typed source authority.
use super::*;

pub(super) fn validate(
    original: &RunReport,
    contract: &eal_core::verification::Contract,
) -> Result<(), Error> {
    let support = original.support_elaboration.as_ref().unwrap();
    for expected in &contract.applicability {
        let source = support
            .acquisitions
            .iter()
            .find(|a| a.id == expected.observation_id)
            .ok_or_else(|| invalid("applicability references no original acquisition"))?;
        let authority = &source.contract;
        if expected.identity != json!(authority.scope.subject)
            || digest(&expected.scope)? != digest(&authority.scope.selectors)?
            || !authority.quantities.iter().any(|q| q.unit == expected.unit)
            || authority
                .time
                .max_age_seconds
                .is_some_and(|age| expected.max_age_seconds > age)
        {
            return Err(invalid(format!(
                "{}: applicability identity, scope, unit or maximum age differs from original source authority {}",
                contract.target_id, source.id
            )));
        }
    }
    Ok(())
}

/// Compare with the admitted candidate so an independently authorized complete
/// conclusion reference can narrow a horizon without rewriting source authority.
pub(super) fn preserves_target(
    proposal: &SupportedArgument,
    contract: &eal_core::verification::Contract,
) -> bool {
    let index = crate::domain::ArgumentIndex::new(&proposal.argument);
    let warrants: Vec<_> = if let Some(warrant) = index.warrant(&contract.target_id) {
        vec![warrant]
    } else {
        index
            .composition_for(&contract.target_id)
            .into_iter()
            .flat_map(|c| &c.selected_warrant_ids)
            .filter_map(|id| index.warrant(id))
            .collect()
    };
    contract.applicability.iter().all(|expected| {
        warrants
            .iter()
            .filter_map(|w| w.reasoning_method.as_ref())
            .filter_map(|m| m.bound())
            .all(|bound| {
                bound.is_mathematical()
                    || (expected.identity == json!(bound.conclusion.subject)
                        && expected.valid_from == bound.conclusion.valid_from
                        && expected.valid_until == bound.conclusion.valid_until)
            })
    })
}
