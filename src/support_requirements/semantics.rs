//! Requirement adequacy preserves source scope, methods and population guarantees.
use super::{
    SupportElaboration,
    lineage::{ValueSemantics, infer_outputs, selected},
};
use eal_collector_contracts::capability::{LogicalStrength, SupportContract, SupportType};
use std::collections::{BTreeMap, BTreeSet};

fn clock_errors(required: &SupportContract, offered: &SupportContract) -> bool {
    required.time.require_original_observation_time
        && !offered.time.require_original_observation_time
        || required
            .time
            .max_uncertainty_seconds
            .is_some_and(|maximum| {
                offered
                    .time
                    .max_uncertainty_seconds
                    .is_none_or(|uncertainty| uncertainty > maximum)
            })
}

fn time_errors(required: &SupportContract, offered: &SupportContract) -> bool {
    required.time.historical_interval != offered.time.historical_interval
        || clock_errors(required, offered)
}

fn contract_errors(required: &SupportContract, offered: &SupportContract) -> Vec<&'static str> {
    let mut errors = Vec::new();
    if required.support_type != offered.support_type {
        errors.push("support type");
    }
    if required.scope.kind != offered.scope.kind
        || required.scope.subject != offered.scope.subject
        || required.scope.selectors != offered.scope.selectors
        || required
            .scope
            .identity_dimensions
            .iter()
            .any(|dimension| !offered.scope.identity_dimensions.contains(dimension))
    {
        errors.push("subject scope or identity dimensions");
    }
    if offered
        .acceptable_methods
        .iter()
        .any(|method| !required.acceptable_methods.contains(method))
    {
        errors.push("qualified measurement method");
    }
    if required.logical_strength != offered.logical_strength
        && !(required.logical_strength == LogicalStrength::ExistentialWitness
            && offered.logical_strength == LogicalStrength::CompleteEnumeration)
    {
        errors.push("logical strength");
    }
    if required
        .population
        .as_ref()
        .is_some_and(|population| offered.population.as_ref() != Some(population))
    {
        errors.push("population");
    }
    if time_errors(required, offered) {
        errors.push("historical interval or observation-time uncertainty");
    }
    if required
        .provenance
        .required_authorities
        .iter()
        .any(|authority| !offered.provenance.required_authorities.contains(authority))
        || required.provenance.transformation_history && !offered.provenance.transformation_history
    {
        errors.push("provenance");
    }
    if required
        .expected_output_schema_sha256
        .as_ref()
        .is_some_and(|digest| offered.expected_output_schema_sha256.as_ref() != Some(digest))
    {
        errors.push("qualified output schema");
    }
    errors
}

/// Selecting a population is itself an evidential operation. Detail reads may
/// change entity kind and method, but cannot relabel its upstream selectors.
fn selection_errors(required: &SupportContract, offered: &SupportContract) -> Vec<&'static str> {
    let mut errors = Vec::new();
    if required
        .scope
        .selectors
        .iter()
        .any(|(name, value)| offered.scope.selectors.get(name) != Some(value))
    {
        errors.push("selection subject scope");
    }
    match required.logical_strength {
        LogicalStrength::CompleteEnumeration | LogicalStrength::StatisticalEstimate => {
            if required.scope.selectors != offered.scope.selectors {
                errors.push("selection population scope");
            }
            if required.logical_strength != offered.logical_strength
                || required.population != offered.population
            {
                errors.push("selection population or logical strength");
            }
        }
        _ => {}
    }
    if required.logical_strength == LogicalStrength::ExactCalculation
        && required.population.is_some()
        && (offered.logical_strength != LogicalStrength::CompleteEnumeration
            || offered.population != required.population
            || offered.scope.selectors != required.scope.selectors)
    {
        errors.push("selection population calculation requires complete enumeration");
    }
    if required.time.historical_interval.is_some()
        && required.time.historical_interval != offered.time.historical_interval
        || clock_errors(required, offered)
    {
        errors.push("selection historical interval or observation-time uncertainty");
    }
    errors
}

fn calculation_errors(required: &SupportContract, values: &[ValueSemantics<'_>]) -> Vec<String> {
    let mut errors = Vec::new();
    if required.support_type != SupportType::Calculation
        || required.logical_strength != LogicalStrength::ExactCalculation
    {
        errors.push("closed arithmetic and identity outputs require Calculation/ExactCalculation; they do not qualify a statistical estimator or empirical measurement".into());
    }
    let methods = values
        .iter()
        .flat_map(|value| value.calculation.methods())
        .collect::<BTreeSet<_>>();
    if methods.is_empty()
        || methods.iter().any(|method| {
            !required
                .acceptable_methods
                .iter()
                .any(|accepted| accepted == method)
        })
    {
        errors.push("derived result must name its explicit versioned closed method (engarg.sum/1, engarg.mean/1, engarg.count/1, engarg.minimum/1, engarg.maximum/1, engarg.scale/1, engarg.identity-equal/1 or engarg.any-identity-equal/1)".into());
    }
    let origins = values
        .iter()
        .flat_map(|value| value.origins.iter().map(|(id, contract)| (*id, *contract)))
        .collect::<BTreeMap<_, _>>();
    let selection = values
        .iter()
        .flat_map(|value| {
            value
                .selection
                .iter()
                .map(|(id, contract)| (*id, *contract))
        })
        .collect::<BTreeMap<_, _>>();
    let controls = values
        .iter()
        .flat_map(|value| value.controls.iter().map(|(id, contract)| (*id, *contract)))
        .collect::<BTreeMap<_, _>>();
    let identity = methods.contains("engarg.identity-equal/1")
        || methods.contains("engarg.any-identity-equal/1");
    let mut selectors = required.scope.selectors.clone();
    let operands = selectors.remove("operands");
    let operand_times = selectors.remove("operand_times");
    let sample_sources = selectors.remove("sample_sources");
    if identity {
        let expected = origins
            .iter()
            .map(|(id, contract)| {
                (
                    (*id).to_owned(),
                    serde_json::to_value(&contract.scope).expect("subject scope serialises"),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        if required.scope.kind != "identity_relation"
            || operands.as_ref() != Some(&serde_json::Value::Object(expected))
        {
            errors.push("identity relation requires scope.kind identity_relation and scope.selectors.operands mapping each logical source ID to its exact original SubjectScope".into());
        }
        let expected_times = origins
            .iter()
            .chain(&selection)
            .chain(&controls)
            .map(|(id, contract)| {
                (
                    (*id).to_owned(),
                    serde_json::to_value(&contract.time).expect("time contract serialises"),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        if operand_times.as_ref() != Some(&serde_json::Value::Object(expected_times)) {
            errors.push("identity relation requires scope.selectors.operand_times mapping every logical source ID to its exact original TimeRequirement; identity does not establish lifecycle or temporal causation".into());
        }
        if sample_sources.is_some() {
            errors.push("identity relation uses explicit operands and operand_times rather than numeric sample_sources".into());
        }
        if origins.values().any(|contract| {
            selectors
                .iter()
                .any(|(name, value)| contract.scope.selectors.get(name) != Some(value))
        }) {
            errors
                .push("identity relation common selectors do not match every operand scope".into());
        }
        if required.population.is_some() {
            errors.push(
                "a scalar identity comparison cannot qualify a population-wide relation".into(),
            );
        }
    } else {
        if operands.is_some()
            || operand_times.is_some()
            || origins
                .values()
                .any(|contract| contract.scope.selectors != selectors)
        {
            errors.push(
                "arithmetic result selectors must preserve the exact measured input scope".into(),
            );
        }
        if required.population.is_none() {
            let expected = serde_json::Value::Array(
                origins
                    .keys()
                    .map(|id| serde_json::Value::String((*id).into()))
                    .collect(),
            );
            if sample_sources.as_ref() != Some(&expected) {
                errors.push("sample calculation requires scope.selectors.sample_sources listing the exact logical measurement source IDs in lexical order; it computes the retained sample, not a population total".into());
            }
        } else if sample_sources.is_some() {
            errors.push("population calculation cannot substitute a retained sample identity for complete enumeration".into());
        }
    }
    let input_methods = required
        .acceptable_methods
        .iter()
        .filter(|method| {
            !matches!(
                method.as_str(),
                "engarg.sum/1"
                    | "engarg.mean/1"
                    | "engarg.count/1"
                    | "engarg.minimum/1"
                    | "engarg.maximum/1"
                    | "engarg.scale/1"
                    | "engarg.identity-equal/1"
                    | "engarg.any-identity-equal/1"
            )
        })
        .collect::<BTreeSet<_>>();
    if !input_methods.is_empty()
        && origins.values().any(|contract| {
            contract
                .acceptable_methods
                .iter()
                .any(|method| !input_methods.contains(method))
        })
    {
        errors.push("derived result input methods differ from its explicit measurement protocol constraints".into());
    }
    let all = origins
        .iter()
        .chain(&selection)
        .map(|(_, contract)| *contract)
        .collect::<Vec<_>>();
    let provenance_sources = all
        .iter()
        .copied()
        .chain(controls.values().copied())
        .collect::<Vec<_>>();
    if !identity && all.iter().any(|contract| time_errors(required, contract)) {
        errors.push("derived input historical interval or observation-time uncertainty differs from the result contract".into());
    }
    if identity
        && provenance_sources.iter().any(|contract| {
            required.time.require_original_observation_time
                && !contract.time.require_original_observation_time
                || required
                    .time
                    .max_uncertainty_seconds
                    .is_some_and(|maximum| {
                        contract
                            .time
                            .max_uncertainty_seconds
                            .is_none_or(|value| value > maximum)
                    })
        })
    {
        errors.push("identity inputs do not preserve the result's original observation-time and uncertainty requirements".into());
    }
    if let Some(population) = &required.population
        && all.iter().any(|contract| {
            contract.logical_strength != LogicalStrength::CompleteEnumeration
                || contract.population.as_ref() != Some(population)
                || contract.scope.selectors != selectors
        })
    {
        errors.push("population calculation requires complete enumeration of the same population and selectors through every measurement and selection ancestor".into());
    }
    if !identity
        && controls
            .values()
            .any(|contract| clock_errors(required, contract))
    {
        errors.push("calculation witness controls do not preserve original observation clocks and uncertainty".into());
    }
    let authorities = provenance_sources
        .iter()
        .flat_map(|contract| contract.provenance.required_authorities.iter())
        .collect::<BTreeSet<_>>();
    if required
        .provenance
        .required_authorities
        .iter()
        .any(|authority| !authorities.contains(authority))
        || required.provenance.transformation_history
            && provenance_sources
                .iter()
                .any(|contract| !contract.provenance.transformation_history)
    {
        errors.push("derived result does not preserve its required source authorities and transformation provenance".into());
    }
    if required.expected_output_schema_sha256.is_some() {
        errors.push("closed calculated values require their logical quantity contract rather than an acquired source output-schema identity".into());
    }
    errors
}

/// Validate qualified measurements, explicit closed calculations and selection
/// ancestry without promoting a measured sample into a new empirical method.
pub fn validate_semantic_flow(elaboration: &SupportElaboration) -> Vec<String> {
    let (outputs, mut errors) = infer_outputs(elaboration);
    for acquisition in &elaboration.acquisitions {
        if let Some(value) = outputs.get(acquisition.id.as_str()) {
            for (source, contract) in &value.selection {
                let mismatches = selection_errors(&acquisition.contract, contract);
                if !mismatches.is_empty() {
                    errors.push(format!(
                        "logical acquisition {} selector ancestor {} does not preserve {}",
                        acquisition.id,
                        source,
                        mismatches.join(", ")
                    ));
                }
            }
        }
    }
    let requirements = elaboration
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect::<BTreeMap<_, _>>();
    for evaluation in &elaboration.evaluations {
        let Some(requirement) = requirements.get(evaluation.requirement_id.as_str()) else {
            continue;
        };
        let Some(required) = &requirement.contract else {
            continue;
        };
        if evaluation.inputs.is_empty() {
            continue;
        }
        let values = match evaluation
            .inputs
            .iter()
            .map(|reference| selected(reference, &outputs))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(values) => values,
            Err(error) => {
                errors.push(format!(
                    "evaluation {} cannot establish semantic flow: {error}",
                    evaluation.id
                ));
                continue;
            }
        };
        let calculated = values
            .iter()
            .any(|value| value.calculation.requires_calculation())
            || required.support_type == SupportType::Calculation
                && values.iter().any(|value| !value.calculation.is_empty());
        if calculated {
            errors.extend(
                calculation_errors(required, &values)
                    .into_iter()
                    .map(|error| format!("evaluation {}: {error}", evaluation.id)),
            );
        } else {
            for (source, offered) in values
                .iter()
                .flat_map(|value| &value.origins)
                .collect::<BTreeMap<_, _>>()
            {
                let mismatches = contract_errors(required, offered);
                if !mismatches.is_empty() {
                    errors.push(format!(
                        "evaluation {} source {} does not preserve requirement {}: {}",
                        evaluation.id,
                        source,
                        requirement.id,
                        mismatches.join(", ")
                    ));
                }
            }
            for (source, offered) in values
                .iter()
                .flat_map(|value| &value.controls)
                .collect::<BTreeMap<_, _>>()
            {
                if clock_errors(required, offered)
                    || required.provenance.transformation_history
                        && !offered.provenance.transformation_history
                {
                    errors.push(format!("evaluation {} witness control {} does not preserve observation clocks or transformation provenance",evaluation.id,source));
                }
            }
            for (source, offered) in values
                .iter()
                .flat_map(|value| &value.selection)
                .collect::<BTreeMap<_, _>>()
            {
                let mismatches = selection_errors(required, offered);
                if !mismatches.is_empty() {
                    errors.push(format!(
                        "evaluation {} selection ancestor {} does not preserve {}",
                        evaluation.id,
                        source,
                        mismatches.join(", ")
                    ));
                }
            }
        }
        let mut quantities = BTreeSet::new();
        for value in &values {
            value.shape.quantities(&mut quantities);
        }
        for quantity in &required.quantities {
            if !quantities.contains(&(quantity.name.as_str(), quantity.unit.as_str())) {
                errors.push(format!("evaluation {} does not provide logical quantity {} in unit {}; use the explicit closed derivation quantity name and a justified unit conversion",evaluation.id,quantity.name,quantity.unit));
            }
        }
    }
    errors
}
