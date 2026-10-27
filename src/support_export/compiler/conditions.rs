//! Retain authored conditions even when no observation route was elaborated.
//!
//! Missing conditions stay unavailable. Only explicit canonical requirements may
//! introduce a scoped review request; the argument emitter never invents one.
use super::{CompiledEvidence, DerivedSpec, name, write_derived};
use crate::{
    Error,
    domain::{ArgumentCase, DefeaterStatus, SupportRelationship},
    export_policy::ExportPolicy,
};
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn complete(
    case: &ArgumentCase,
    policy: &ExportPolicy,
    compiled: &mut CompiledEvidence,
) -> Result<(), Error> {
    for assumption in &case.assumptions {
        missing(
            "assumption_validation",
            &assumption.id,
            &mut compiled.targets,
            &mut compiled.declarations,
            &mut compiled.pending,
            policy,
        )?;
    }
    for defeater in &case.defeaters {
        missing(
            "challenge_activation",
            &defeater.id,
            &mut compiled.challenge_activation,
            &mut compiled.declarations,
            &mut compiled.pending,
            policy,
        )?;
        if defeater.material
            && matches!(
                defeater.status,
                DefeaterStatus::Open | DefeaterStatus::Sustained
            )
        {
            missing(
                "challenge_resolution",
                &defeater.id,
                &mut compiled.challenge_resolution,
                &mut compiled.declarations,
                &mut compiled.pending,
                policy,
            )?;
        }
    }
    for composition in &case.compositions {
        if composition.relationship == SupportRelationship::Cumulative {
            missing(
                "composition_adequacy",
                &composition.conclusion_id,
                &mut compiled.targets,
                &mut compiled.declarations,
                &mut compiled.pending,
                policy,
            )?;
        }
    }
    Ok(())
}

fn missing(
    role: &str,
    target: &str,
    targets: &mut BTreeMap<String, Vec<String>>,
    declarations: &mut String,
    pending: &mut Vec<String>,
    policy: &ExportPolicy,
) -> Result<(), Error> {
    if targets.get(target).is_some_and(|gates| !gates.is_empty()) {
        return Ok(());
    }
    let declaration = name(&format!("pending_{role}"), target);
    let reason = format!("{target}: no canonical {role} observation requirement was elaborated");
    write_derived(
        declarations,
        DerivedSpec {
            name: &declaration,
            kind: "unavailable_condition",
            max_age: policy.review_max_age_seconds,
            input: &json!({"target":target,"role":role,"reason":reason}),
            bindings: "",
            expression: "object(\"accepted\", pending.required_observation)",
            predicates: &["accepted == true".into()],
        },
    )?;
    targets.insert(target.into(), vec![declaration]);
    pending.push(reason);
    Ok(())
}
