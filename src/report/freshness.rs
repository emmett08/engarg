//! Explicit operator clarification of missing graph freshness contracts.
use super::invalid;
use crate::{Error, support_requirements::SupportElaboration};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Each ID is a deliberate operator selection, never an inferred scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreshnessClarification {
    pub schema: String,
    pub rationale: String,
    pub requirements: BTreeMap<String, u64>,
}

pub fn apply(
    elaboration: &mut SupportElaboration,
    clarification: &FreshnessClarification,
) -> Result<(), Error> {
    if clarification.schema != "engarg.freshness-clarification/1"
        || clarification.rationale.trim().is_empty()
        || clarification.rationale.len() > 4096
        || clarification.requirements.is_empty()
    {
        return Err(invalid(
            "freshness clarification requires its schema, rationale and explicitly selected requirement IDs",
        ));
    }
    let mut revised = elaboration.clone();
    for (id, age) in &clarification.requirements {
        if !(1..=315_360_000).contains(age) {
            return Err(invalid(
                "freshness age must be between 1 and 315360000 seconds",
            ));
        }
        let requirement = revised
            .requirements
            .iter_mut()
            .find(|r| &r.id == id)
            .ok_or_else(|| {
                invalid(&format!(
                    "freshness clarification names unknown requirement {id}"
                ))
            })?;
        let contract = requirement.contract.as_mut().ok_or_else(|| {
            invalid(&format!(
                "freshness clarification requirement {id} has no machine contract"
            ))
        })?;
        let evaluation = revised
            .evaluations
            .iter_mut()
            .find(|e| &e.requirement_id == id)
            .ok_or_else(|| {
                invalid(&format!(
                    "freshness clarification requirement {id} has no evaluation"
                ))
            })?;
        if contract.time.max_age_seconds.is_some() || evaluation.max_age_seconds.is_some() {
            return Err(invalid(&format!(
                "freshness clarification cannot replace an existing bound for requirement {id}"
            )));
        }
        contract.time.max_age_seconds = Some(*age);
        evaluation.max_age_seconds = Some(*age);
    }
    *elaboration = revised;
    Ok(())
}
