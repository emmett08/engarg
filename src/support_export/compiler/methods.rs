//! Bind measured premise fields while closed mathematical queries remain source-free.
use super::{
    CompiledEvidence, DerivedSpec, ProgramPlan, invalid, values, write_bind, write_derived,
};
use crate::{
    Error,
    domain::{ArgumentCase, FormalInputCorrespondence},
    eal_export::name,
    export_policy::ExportPolicy,
    support_requirements::SupportElaboration,
};
use eal_collector_contracts::execution::OutputReference;
use serde_json::json;
pub(super) fn compile(
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    plan: &ProgramPlan,
    policy: &ExportPolicy,
    compiled: &mut CompiledEvidence,
) -> Result<(), Error> {
    for warrant in &case.warrants {
        let Some(bound) = warrant
            .reasoning_method
            .as_ref()
            .and_then(|method| method.bound())
        else {
            continue;
        };
        if bound.is_mathematical() {
            // Domain admission requires a closed native theorem. Its literal
            // definitions are programme data, never an acquired observation.
            continue;
        }
        for premise in &bound.formal_premises {
            let requirement = elaboration
                .requirements
                .iter()
                .find(|requirement| {
                    requirement
                        .evidence_need_ids
                        .contains(&premise.evidence_need_id)
                })
                .ok_or_else(|| invalid("formal premise lacks an explicit support requirement"))?;
            let contract = requirement
                .contract
                .as_ref()
                .ok_or_else(|| invalid("formal premise lacks an explicit subject scope"))?;
            let target = &bound.conclusion.scope;
            if contract.scope.kind != target.kind
                || target
                    .identity_dimensions
                    .iter()
                    .any(|dimension| !contract.scope.identity_dimensions.contains(dimension))
                || target.selectors.iter().any(|selector| {
                    contract.scope.selectors.get(&selector.name)
                        != serde_json::to_value(&selector.value).ok().as_ref()
                })
            {
                return Err(invalid(format!(
                    "{}: source scope does not preserve the method's target identity; an explicit scope-transfer argument is required",
                    premise.evidence_need_id
                )));
            }
        }
        let identity: Vec<_> = bound
            .formal_premises
            .iter()
            .filter(|premise| {
                matches!(
                    premise.correspondence,
                    FormalInputCorrespondence::IdentityValue { .. }
                )
            })
            .collect();
        if identity.is_empty() {
            continue;
        }
        let mut input = json!({"payload":{}});
        let mut binds = String::new();
        for premise in identity {
            let FormalInputCorrespondence::IdentityValue {
                source_input_index,
                source_pointer,
            } = &premise.correspondence
            else {
                unreachable!()
            };
            let requirements: Vec<_> = elaboration
                .requirements
                .iter()
                .filter(|requirement| {
                    requirement
                        .evidence_need_ids
                        .contains(&premise.evidence_need_id)
                })
                .collect();
            let [requirement] = requirements.as_slice() else {
                return Err(invalid(format!(
                    "{} has ambiguous empirical correspondence",
                    premise.evidence_need_id
                )));
            };
            let evaluation = plan
                .evaluations
                .iter()
                .find(|evaluation| evaluation.requirement_id == requirement.id)
                .ok_or_else(|| invalid("premise evaluation absent"))?;
            let source = evaluation
                .inputs
                .get(*source_input_index)
                .ok_or_else(|| invalid("premise source input index is out of range"))?;
            let destination = format!("/payload{}", premise.input_pointer);
            values::insert_slot(&mut input, &destination)?;
            let reference = OutputReference {
                step_id: source.step_id.clone(),
                pointer: format!("{}{}", source.pointer, source_pointer),
            };
            write_bind(&mut binds, &destination, &reference, plan);
        }
        let declaration = name("method_input", &warrant.id);
        write_derived(
            &mut compiled.declarations,
            DerivedSpec {
                name: &declaration,
                kind: bound.method.evidence_kind().unwrap_or("calculation"),
                max_age: policy.acquisition_max_age_seconds,
                input: &input,
                bindings: &binds,
                expression: "payload",
                predicates: &[],
            },
        )?;
        compiled
            .method_inputs
            .insert(warrant.id.clone(), declaration);
    }
    Ok(())
}
