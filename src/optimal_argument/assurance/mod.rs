//! Operator-owned checks bind proposed arguments to fixed evidence and formal targets.
mod checks;
mod guards;

use crate::{
    Error, domain::MethodResultPredicate, engine::RunReport,
    support_requirements::SupportedArgument,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const PLAN_SCHEMA: &str = "engarg.assurance-plan/1";
pub const RECEIPT_SCHEMA: &str = "engarg.assurance-receipt/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssurancePlan {
    pub schema: String,
    pub checks: Vec<VerificationCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    pub id: String,
    pub role: CheckRole,
    pub target_id: String,
    pub procedure: CheckProcedure,
    #[serde(default)]
    pub discharges: Vec<Discharge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckRole {
    Premise,
    Formalisation,
    Inference,
    Applicability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discharge {
    pub requirement_id: String,
    pub requirement_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckProcedure {
    /// Collection and native predicate assessment, retaining original clocks.
    Requirement { requirement_id: String },
    /// A typed reference value supplied independently of the proposal.
    Value { pointer: String, expected: Value },
    /// Check candidate programme data against an independent intended query,
    /// then execute that exact query with the installed native method.
    Method {
        conclusion_id: String,
        identifier: String,
        reference_query: Value,
        result: MethodResultPredicate,
    },
    /// Check premise conditions and conclusions separately. This avoids the
    /// vacuous equivalence of two already-valid implication formulae.
    LogicalEquivalence {
        conclusion_id: String,
        reference_query: Value,
    },
}

impl<'de> Deserialize<'de> for CheckProcedure {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        // Internally tagged enum buffering uses ContentDeserializer, which
        // cannot carry RawValue's exact-number transport. Admit the whole
        // procedure once, rejecting duplicate keys, then decode its fields
        // through the ordinary serde_json Value deserializer instead.
        let mut value = eal_collector_contracts::deserialize_value_exact(decoder)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| serde::de::Error::custom("assurance procedure must be an object"))?;
        let kind = object
            .remove("kind")
            .ok_or_else(|| serde::de::Error::custom("assurance procedure kind is required"))?;

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Requirement {
            requirement_id: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ReferenceValue {
            pointer: String,
            #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
            expected: Value,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Method {
            conclusion_id: String,
            identifier: String,
            #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
            reference_query: Value,
            result: MethodResultPredicate,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Equivalence {
            conclusion_id: String,
            #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
            reference_query: Value,
        }
        match kind.as_str() {
            Some("requirement") => serde_json::from_value::<Requirement>(value)
                .map(|r| Self::Requirement {
                    requirement_id: r.requirement_id,
                })
                .map_err(serde::de::Error::custom),
            Some("value") => serde_json::from_value::<ReferenceValue>(value)
                .map(|r| Self::Value {
                    pointer: r.pointer,
                    expected: r.expected,
                })
                .map_err(serde::de::Error::custom),
            Some("method") => serde_json::from_value::<Method>(value)
                .map(|r| Self::Method {
                    conclusion_id: r.conclusion_id,
                    identifier: r.identifier,
                    reference_query: r.reference_query,
                    result: r.result,
                })
                .map_err(serde::de::Error::custom),
            Some("logical_equivalence") => serde_json::from_value::<Equivalence>(value)
                .map(|r| Self::LogicalEquivalence {
                    conclusion_id: r.conclusion_id,
                    reference_query: r.reference_query,
                })
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom("unknown assurance procedure kind")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Unknown,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub id: String,
    pub role: CheckRole,
    pub target_id: String,
    pub status: CheckStatus,
    pub reasons: Vec<String>,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssuranceReceipt {
    pub schema: String,
    pub plan_sha256: String,
    pub source_report_sha256: String,
    pub candidate_sha256: String,
    pub checks: Vec<CheckResult>,
    pub discharged_requirement_ids: Vec<String>,
    pub limitations: Vec<String>,
}

impl AssuranceReceipt {
    pub fn passed(&self) -> bool {
        !self.checks.is_empty()
            && self
                .checks
                .iter()
                .all(|check| check.status == CheckStatus::Pass)
    }

    /// Only actual collection checks may remain deferred at this boundary.
    pub fn ready_for_execution(&self) -> bool {
        !self.checks.is_empty()
            && self.checks.iter().all(|check| {
                check.status == CheckStatus::Pass
                    || (check.status == CheckStatus::Unknown
                        && check.details["deferred_requirement"] == true)
            })
    }
}

pub fn digest(value: &impl Serialize) -> Result<String, Error> {
    eal_collector_contracts::execution::canonical_digest(value).map_err(Error::InvalidInput)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

impl AssurancePlan {
    /// Admit operator policy before any paid model call or evidence collection.
    pub fn validate(&self, original: &RunReport) -> Result<(), Error> {
        crate::report::validate(original)?;
        if self.schema != PLAN_SCHEMA
            || !(1..=128).contains(&self.checks.len())
            || serde_json::to_vec(self)?.len() > 1_048_576
        {
            return Err(invalid(
                "assurance plan requires its supported schema, 1..128 checks and at most 1 MiB",
            ));
        }
        let support = original
            .support_elaboration
            .as_ref()
            .ok_or_else(|| invalid("assurance requires complete original support elaboration"))?;
        let index = crate::domain::ArgumentIndex::new(&original.argument);
        let selected = index.selected_dependency("current_claim");
        let mut ids = BTreeSet::new();
        let mut covered = BTreeSet::new();
        let mut formalisation = false;
        for check in &self.checks {
            if check.id.len() > 128
                || !eal_collector_contracts::identifier(&check.id)
                || !ids.insert(check.id.as_str())
                || index.node(&check.target_id).is_none()
            {
                return Err(invalid(
                    "assurance checks require unique bounded IDs and known original targets",
                ));
            }
            formalisation |= check.role == CheckRole::Formalisation;
            match &check.procedure {
                CheckProcedure::Requirement { requirement_id } => {
                    let requirement = support
                        .requirements
                        .iter()
                        .find(|r| &r.id == requirement_id)
                        .ok_or_else(|| {
                            invalid(format!(
                                "assurance check {} names unknown requirement {requirement_id}",
                                check.id
                            ))
                        })?;
                    if requirement.target_id != check.target_id {
                        return Err(invalid(
                            "assurance requirement check must name its exact authored target",
                        ));
                    }
                    if !check.discharges.is_empty() {
                        return Err(invalid(
                            "an evidence check cannot discharge another obligation",
                        ));
                    }
                    covered.insert(requirement_id.as_str());
                }
                CheckProcedure::Value { pointer, .. } => {
                    if !pointer.starts_with('/')
                        || pointer.len() > 2048
                        || !check.discharges.is_empty()
                    {
                        return Err(invalid(
                            "value checks need a bounded JSON pointer and cannot discharge obligations",
                        ));
                    }
                    if matches!(check.role, CheckRole::Premise | CheckRole::Applicability) {
                        return Err(invalid(
                            "empirical premise and applicability checks require actual Requirement evidence",
                        ));
                    }
                }
                CheckProcedure::Method {
                    conclusion_id,
                    identifier,
                    reference_query,
                    result,
                } => {
                    validate_conclusion(check, conclusion_id, &index)?;
                    let registry = eal_core::methods::default_registry();
                    let errors =
                        eal_core::methods::validate_query(identifier, reference_query, registry);
                    if !errors.is_empty()
                        || registry
                            .get(identifier)
                            .and_then(|c| c.output_type(&result.path))
                            .is_none()
                    {
                        return Err(invalid(format!(
                            "assurance method/check predicate is invalid: {}",
                            errors.join("; ")
                        )));
                    }
                }
                CheckProcedure::LogicalEquivalence {
                    conclusion_id,
                    reference_query,
                } => {
                    validate_conclusion(check, conclusion_id, &index)?;
                    let errors = eal_core::methods::validate_query(
                        "deductive/1",
                        reference_query,
                        eal_core::methods::default_registry(),
                    );
                    if !errors.is_empty() {
                        return Err(invalid(errors.join("; ")));
                    }
                }
            }
            let mut discharged = BTreeSet::new();
            for discharge in &check.discharges {
                if !discharged.insert(discharge.requirement_id.as_str()) {
                    return Err(invalid("one check cannot repeat a discharged requirement"));
                }
                guards::validate_discharge(original, check, discharge)?;
                covered.insert(discharge.requirement_id.as_str());
            }
        }
        if !formalisation {
            return Err(invalid(
                "assurance plan requires an explicit formalisation check",
            ));
        }
        for requirement in &support.requirements {
            if selected.contains(requirement.target_id.as_str())
                && !covered.contains(requirement.id.as_str())
            {
                return Err(invalid(format!(
                    "assurance plan omits selected original obligation {}",
                    requirement.id
                )));
            }
        }
        Ok(())
    }
}

fn validate_conclusion(
    check: &VerificationCheck,
    conclusion_id: &str,
    index: &crate::domain::ArgumentIndex<'_>,
) -> Result<(), Error> {
    if index.claim(conclusion_id).is_none()
        || check.target_id != conclusion_id
        || !matches!(check.role, CheckRole::Formalisation | CheckRole::Inference)
    {
        return Err(invalid(
            "native formal checks require their exact claim target and a formalisation/inference role",
        ));
    }
    Ok(())
}

/// Run all immutable-reference checks and dependency guards before collection.
pub fn prepare(
    original: &RunReport,
    proposal: &SupportedArgument,
    plan: &AssurancePlan,
) -> Result<AssuranceReceipt, Error> {
    plan.validate(original)?;
    let mut receipt = AssuranceReceipt {
        schema: RECEIPT_SCHEMA.into(), plan_sha256: digest(plan)?, source_report_sha256: digest(original)?,
        candidate_sha256: digest(proposal)?, checks: Vec::new(), discharged_requirement_ids: Vec::new(),
        limitations: vec![
            "Formal checks establish their declared programme-data relation. The independent operator reference still defines the intended interpretation.".into(),
            "Empirical premise truth and applicability are assessed through the actual scoped source evidence; no LLM assertion creates an observation.".into(),
            "Native immutable-query checks are recomputed without invented observation clocks; empirical records retain their own freshness conditions.".into(),
        ],
    };
    if let Err(errors) = crate::domain::validate_case(&original.request, &proposal.argument) {
        receipt.checks.push(guards::guard(
            "argument_admission",
            CheckStatus::Fail,
            errors,
        ));
        return Ok(receipt);
    }
    for check in &plan.checks {
        let result = checks::prepare_check(proposal, check)?;
        if result.status == CheckStatus::Pass {
            receipt
                .discharged_requirement_ids
                .extend(check.discharges.iter().map(|d| d.requirement_id.clone()));
        }
        receipt.checks.push(result);
    }
    receipt.discharged_requirement_ids.sort();
    receipt.discharged_requirement_ids.dedup();
    guards::check(original, proposal, plan, &mut receipt)?;
    Ok(receipt)
}

/// Fulfil deferred evidence checks from a retained native execution envelope.
/// The caller also performs the existing exact-source replay verification.
pub fn finish(
    receipt: &mut AssuranceReceipt,
    proposal: &SupportedArgument,
    execution: &Value,
    plan: &AssurancePlan,
) -> Result<(), Error> {
    if receipt.schema != RECEIPT_SCHEMA
        || receipt.plan_sha256 != digest(plan)?
        || receipt.candidate_sha256 != digest(proposal)?
    {
        return Err(invalid(
            "assurance receipt, policy or proposed source changed before evidence assessment",
        ));
    }
    for check in &plan.checks {
        if let CheckProcedure::Requirement { requirement_id } = &check.procedure {
            let result = checks::finish_requirement(proposal, execution, check, requirement_id);
            let Some(slot) = receipt.checks.iter_mut().find(|slot| slot.id == check.id) else {
                return Err(invalid("assurance receipt omits a declared check"));
            };
            *slot = result;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
