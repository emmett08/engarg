//! Qualified finite identifier-audit composition over one selected enumeration.
use super::schema_at;
use crate::{
    Error,
    support_requirements::{AcquisitionBlueprint, SupportElaboration},
};
use eal_collector_contracts::{
    capability::{
        CapabilityDescriptor, LogicalStrength, MeasurementQuantity, SupportContract, SupportType,
        compatible,
    },
    execution::{EvaluationCriterion, RequiredCoverage},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The recorded proof names the selected qualified source and exact logical/physical
/// bindings. It does not assert that the calculator itself is the source authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentifierAuditProof {
    pub source_step_id: String,
    pub source_capability_id: String,
    pub source_qualification_sha256: String,
    pub calculation_capability_id: String,
    pub calculation_qualification_sha256: String,
    pub source_items_pointer: String,
    pub source_raw_items_pointer: String,
    pub source_complete_pointer: String,
    pub source_raw_complete_pointer: String,
    pub source_identity_pointer: String,
    pub argument_items_pointer: String,
    pub argument_complete_pointer: String,
    pub argument_identity_pointer: String,
    pub result_count_pointer: String,
    pub result_distinct_pointer: String,
    pub result_complete_pointer: String,
    pub result_identity_pointer: String,
    pub item_id_pointer: String,
    pub identity_scope_key: String,
    pub population: String,
    pub population_quantity: String,
    pub echoes: BTreeMap<String, String>,
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn logical_pointer(quantity: &MeasurementQuantity) -> String {
    format!("/{}", quantity.name.replace('~', "~0").replace('/', "~1"))
}
fn physical_pointer(logical: &str, bindings: &BTreeMap<String, String>) -> String {
    bindings
        .iter()
        .find(|(_, source)| source.as_str() == logical)
        .map(|(target, _)| target.clone())
        .unwrap_or_else(|| logical.to_owned())
}
fn stripped(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                // These two annotations are checked separately against the
                // qualified quantity and unit before schema equivalence.
                .filter(|(key, _)| {
                    !matches!(key.as_str(), "x-engarg-unit" | "x-engarg-source-contract")
                })
                .map(|(key, value)| (key.clone(), stripped(value)))
                .collect(),
        ),
        Value::Array(array) => Value::Array(array.iter().map(stripped).collect()),
        _ => value.clone(),
    }
}

/// Prove a bounded operation-specific calculation over an independently
/// qualified, single complete-enumeration source. The returned local contract is
/// used only to match the calculator; the original requirement is unchanged.
pub fn identifier_audit_proof(
    elaboration: &SupportElaboration,
    calculation: &AcquisitionBlueprint,
    source: &AcquisitionBlueprint,
    calculator: &CapabilityDescriptor,
    producer: &CapabilityDescriptor,
    input_bindings: &BTreeMap<String, String>,
) -> Result<(IdentifierAuditProof, SupportContract), Error> {
    let fail = || {
        invalid(
            "identifier audit composition lacks a qualified exact source, binding or completeness condition",
        )
    };
    let contract = &calculation.contract;
    let source_contract = &source.contract;
    if contract.support_type != SupportType::Calculation
        || contract.logical_strength != LogicalStrength::ExactCalculation
        || contract.acceptable_methods.len() != 1
        || contract.population.is_none()
        || contract.scope.identity_dimensions.len() != 1
        || source_contract.support_type != SupportType::SourceRetrieval
        || source_contract.logical_strength != LogicalStrength::CompleteEnumeration
        || source_contract.population != contract.population
        || source_contract.scope != contract.scope
        || source_contract.time.historical_interval != contract.time.historical_interval
        || calculation.for_each.is_some()
        || source.for_each.is_some()
        || !source.bindings.is_empty()
        || calculation.bindings.len() != 3
        || calculation
            .bindings
            .iter()
            .any(|binding| binding.output.step_id != source.id || !binding.role.is_selection())
        || !compatible(source_contract, producer).adequate
        || input_bindings
            .values()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != input_bindings.len()
    {
        return Err(fail());
    }
    let source_sem = producer.semantics.as_ref().ok_or_else(fail)?;
    let calc_sem = calculator.semantics.as_ref().ok_or_else(fail)?;
    let audit = calc_sem.identifier_audit.as_ref().ok_or_else(fail)?;
    // The typed audit qualification applies to the one method selected here,
    // rather than being transferred to another method on a multi-method tool.
    if calc_sem.methods != contract.acceptable_methods {
        return Err(fail());
    }
    let source_coverage = source_sem
        .coverage
        .iter()
        .find(|coverage| {
            coverage.strength == LogicalStrength::CompleteEnumeration
                && coverage.population == contract.population
        })
        .ok_or_else(fail)?;
    let source_complete = source_sem
        .quantities
        .iter()
        .find(|quantity| {
            Some(&quantity.output_pointer) == source_coverage.completeness_pointer.as_ref()
                && quantity.unit == "boolean"
        })
        .ok_or_else(fail)?;
    let calc_coverage = calc_sem
        .coverage
        .iter()
        .find(|coverage| {
            coverage.strength == LogicalStrength::ExactCalculation && coverage.population.is_none()
        })
        .ok_or_else(fail)?;
    let calc_complete = calc_sem
        .quantities
        .iter()
        .find(|quantity| {
            Some(&quantity.output_pointer) == calc_coverage.completeness_pointer.as_ref()
                && quantity.output_pointer == audit.result_complete_pointer
                && quantity.unit == "boolean"
        })
        .ok_or_else(fail)?;
    let identity = &contract.scope.identity_dimensions[0];
    let source_identity = source_sem
        .quantities
        .iter()
        .find(|quantity| &quantity.name == identity)
        .ok_or_else(fail)?;
    let calc_identity = calc_sem
        .quantities
        .iter()
        .find(|quantity| {
            &quantity.name == identity
                && quantity.output_pointer == audit.result_identity_pointer
                && quantity.unit == source_identity.unit
        })
        .ok_or_else(fail)?;
    let count = calc_sem
        .quantities
        .iter()
        .find(|quantity| quantity.output_pointer == audit.result_count_pointer)
        .ok_or_else(fail)?;
    let distinct = calc_sem
        .quantities
        .iter()
        .find(|quantity| {
            quantity.output_pointer == audit.result_distinct_pointer && quantity.unit == "boolean"
        })
        .ok_or_else(fail)?;
    if [
        &audit.result_count_pointer,
        &audit.result_distinct_pointer,
        &audit.result_complete_pointer,
        &audit.result_identity_pointer,
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>()
    .len()
        != 4
        || schema_at(&calculator.output_schema, &audit.result_count_pointer)
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str)
            != Some("integer")
        || schema_at(&calculator.output_schema, &audit.result_distinct_pointer)
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str)
            != Some("boolean")
        || schema_at(&calculator.output_schema, &audit.result_complete_pointer)
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str)
            != Some("boolean")
        || schema_at(&calculator.output_schema, &audit.result_identity_pointer)
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str)
            != Some("string")
    {
        return Err(fail());
    }
    let find_binding = |quantity: &MeasurementQuantity| -> Result<String, Error> {
        let logical = logical_pointer(quantity);
        let mut matches = calculation
            .bindings
            .iter()
            .filter(|binding| binding.output.pointer == logical);
        let binding = matches.next().ok_or_else(fail)?;
        if matches.next().is_some() {
            return Err(fail());
        }
        Ok(physical_pointer(&binding.argument_pointer, input_bindings))
    };
    let argument_complete = find_binding(source_complete)?;
    let argument_identity = find_binding(source_identity)?;
    if calc_sem.scope_bindings.get(identity) != Some(&argument_identity) {
        return Err(fail());
    }
    let mut remaining = calculation.bindings.iter().filter(|binding| {
        let pointer = physical_pointer(&binding.argument_pointer, input_bindings);
        pointer != argument_complete && pointer != argument_identity
    });
    let items_binding = remaining.next().ok_or_else(fail)?;
    if remaining.next().is_some() {
        return Err(fail());
    }
    let source_items = source_sem
        .quantities
        .iter()
        .find(|quantity| logical_pointer(quantity) == items_binding.output.pointer)
        .ok_or_else(fail)?;
    if source_coverage.population_quantity.as_deref() != Some(source_items.name.as_str()) {
        return Err(fail());
    }
    let argument_items = physical_pointer(&items_binding.argument_pointer, input_bindings);
    if argument_items != audit.argument_items_pointer
        || argument_complete != audit.argument_complete_pointer
        || argument_identity != audit.argument_identity_pointer
    {
        return Err(fail());
    }
    for (quantity, destination, expected_type) in [
        (source_items, &argument_items, "array"),
        (source_complete, &argument_complete, "boolean"),
        (source_identity, &argument_identity, "string"),
    ] {
        let input = schema_at(&calculator.input_schema, destination).ok_or_else(fail)?;
        let output =
            schema_at(&producer.output_schema, &quantity.output_pointer).ok_or_else(fail)?;
        if input.get("type").and_then(Value::as_str) != Some(expected_type)
            || input.get("x-engarg-unit").and_then(Value::as_str) != Some(quantity.unit.as_str())
            || input
                .pointer("/x-engarg-source-contract/x-engarg-quantity/name")
                .and_then(Value::as_str)
                != Some(quantity.name.as_str())
            || input
                .pointer("/x-engarg-source-contract/x-engarg-quantity/unit")
                .and_then(Value::as_str)
                != Some(quantity.unit.as_str())
            || stripped(input) != stripped(output)
        {
            return Err(fail());
        }
    }
    let item_schema = schema_at(&calculator.input_schema, &argument_items)
        .and_then(|schema| schema.get("items"))
        .ok_or_else(fail)?;
    if schema_at(item_schema, &audit.item_id_pointer)
        .and_then(|schema| schema.get("type"))
        .and_then(Value::as_str)
        != Some("string")
    {
        return Err(fail());
    }
    let source_complete_ref = logical_pointer(source_complete);
    let calc_complete_ref = logical_pointer(calc_complete);
    // This per-observation witness establishes only the audited count and
    // distinctness. It cannot qualify unrelated derived or downstream values.
    if elaboration.acquisitions.iter().any(|other| {
        other.id != calculation.id
            && (other
                .bindings
                .iter()
                .any(|binding| binding.output.step_id == calculation.id)
                || other
                    .for_each
                    .as_ref()
                    .is_some_and(|binding| binding.output.step_id == calculation.id))
    }) || elaboration.derivations.iter().any(|derivation| {
        derivation
            .inputs
            .iter()
            .any(|reference| reference.step_id == calculation.id)
    }) {
        return Err(fail());
    }
    let mut relevant = 0;
    for evaluation in &elaboration.evaluations {
        if matches!(
            &evaluation.criterion,
            EvaluationCriterion::Judgement { scope: None, .. }
                | EvaluationCriterion::Unavailable { .. }
        ) {
            // These remain pending regardless of source coverage. They cannot
            // turn a validated calculation into an independent judgement.
            continue;
        }
        if evaluation.required_coverage != RequiredCoverage::Unspecified
            && (evaluation.inputs.iter().any(|reference| {
                reference.step_id == calculation.id
                    && reference.pointer != logical_pointer(count)
                    && reference.pointer != logical_pointer(distinct)
            }) || evaluation.applicability.iter().any(|reference| {
                reference.step_id == calculation.id && reference.pointer != calc_complete_ref
            }) || evaluation
                .challenges
                .iter()
                .any(|reference| reference.step_id == calculation.id))
        {
            return Err(fail());
        }
        if !evaluation.inputs.iter().any(|input| {
            input.step_id == calculation.id
                && (input.pointer == logical_pointer(count)
                    || input.pointer == logical_pointer(distinct))
        }) {
            continue;
        }
        relevant += 1;
        if evaluation.required_coverage != RequiredCoverage::CompleteEnumeration
            || !evaluation.applicability.iter().any(|reference| {
                reference.step_id == source.id && reference.pointer == source_complete_ref
            })
            || !evaluation.applicability.iter().any(|reference| {
                reference.step_id == calculation.id && reference.pointer == calc_complete_ref
            })
        {
            return Err(fail());
        }
    }
    if relevant == 0 {
        return Err(fail());
    }
    // Only the source qualification can supply the producer's authorities.
    // The locally matched calculation must satisfy every remaining obligation.
    let mut local = contract.clone();
    local.population = None;
    local
        .provenance
        .required_authorities
        .retain(|authority| !source_sem.provenance.authorities.contains(authority));
    if local.provenance.required_authorities.is_empty() || !compatible(&local, calculator).adequate
    {
        return Err(fail());
    }
    Ok((
        IdentifierAuditProof {
            source_step_id: source.id.clone(),
            source_capability_id: producer.id.clone(),
            source_qualification_sha256: producer
                .measurement_identity()
                .map_err(Error::InvalidInput)?,
            calculation_capability_id: calculator.id.clone(),
            calculation_qualification_sha256: calculator
                .measurement_identity()
                .map_err(Error::InvalidInput)?,
            source_items_pointer: logical_pointer(source_items),
            source_raw_items_pointer: source_items.output_pointer.clone(),
            source_complete_pointer: source_complete_ref,
            source_raw_complete_pointer: source_complete.output_pointer.clone(),
            source_identity_pointer: logical_pointer(source_identity),
            argument_items_pointer: argument_items,
            argument_complete_pointer: argument_complete,
            argument_identity_pointer: argument_identity,
            result_count_pointer: count.output_pointer.clone(),
            result_distinct_pointer: distinct.output_pointer.clone(),
            result_complete_pointer: calc_complete.output_pointer.clone(),
            result_identity_pointer: calc_identity.output_pointer.clone(),
            item_id_pointer: audit.item_id_pointer.clone(),
            identity_scope_key: identity.clone(),
            population: contract.population.clone().ok_or_else(fail)?,
            population_quantity: source_items.name.clone(),
            echoes: audit.echoes.clone(),
        },
        local,
    ))
}
