//! Conservative admission of semantic information through the closed graph algebra.
use super::SupportElaboration;
use eal_collector_contracts::{
    capability::{LogicalStrength, SupportContract, SupportType},
    execution::{DerivationOperation, OutputReference},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
enum Shape {
    Object(BTreeMap<String, Shape>),
    Quantity { name: String, unit: String },
    Collection(Box<Shape>),
}

#[derive(Clone)]
struct ValueSemantics<'a> {
    origins: BTreeMap<&'a str, &'a SupportContract>,
    /// Sources selecting the measured records remain a separate lineage: their
    /// methods need not equal a detail read's method, but their scope and coverage
    /// cannot disappear when the detail read is evaluated.
    selection: BTreeMap<&'a str, &'a SupportContract>,
    shape: Shape,
    calculation: Option<&'static str>,
}

impl Shape {
    fn select(&self, pointer: &str) -> Option<Self> {
        if pointer.is_empty() {
            return Some(self.clone());
        }
        // A quantity may itself contain a complete sample. Selecting an element
        // would silently discard coverage, so only logical quantity fields are admitted.
        let mut selected = self;
        for segment in pointer.strip_prefix('/')?.split('/') {
            let name = segment.replace("~1", "/").replace("~0", "~");
            selected = match selected {
                Self::Object(fields) => fields.get(&name)?,
                _ => return None,
            };
        }
        Some(selected.clone())
    }
    fn quantity(&self) -> Option<(&str, &str)> {
        match self {
            Self::Quantity { name, unit } => Some((name, unit)),
            Self::Collection(inner) => match inner.as_ref() {
                Self::Quantity { name, unit } => Some((name, unit)),
                _ => None,
            },
            Self::Object(_) => None,
        }
    }
    fn quantities<'a>(&'a self, output: &mut BTreeSet<(&'a str, &'a str)>) {
        match self {
            Self::Quantity { name, unit } => {
                output.insert((name, unit));
            }
            Self::Object(fields) => {
                for shape in fields.values() {
                    shape.quantities(output);
                }
            }
            Self::Collection(inner) => inner.quantities(output),
        }
    }
}

fn selected<'a>(
    reference: &OutputReference,
    outputs: &BTreeMap<&str, ValueSemantics<'a>>,
) -> Result<ValueSemantics<'a>, String> {
    let value = outputs
        .get(reference.step_id.as_str())
        .ok_or("upstream semantic contract is unavailable")?;
    Ok(ValueSemantics {
        origins: value.origins.clone(),
        selection: value.selection.clone(),
        calculation: value.calculation,
        shape: value.shape.select(&reference.pointer).ok_or_else(|| {
            format!(
                "{}{} does not select a complete declared logical quantity",
                reference.step_id, reference.pointer
            )
        })?,
    })
}

fn same_quantity<'a>(values: &'a [ValueSemantics<'_>]) -> Result<(&'a str, &'a str), String> {
    let first = values
        .first()
        .and_then(|value| value.shape.quantity())
        .ok_or("operation requires declared scalar or sample quantities")?;
    if values
        .iter()
        .any(|value| value.shape.quantity() != Some(first))
    {
        return Err("operation inputs must preserve the same logical quantity and unit".into());
    }
    Ok(first)
}

fn scale_valid(numerator: &str, denominator: &str, from: &str, to: &str) -> bool {
    // These are multiplicative conversions only. Offset or domain-dependent
    // conversions require an explicitly qualified calculation source.
    fn canonical(unit: &str) -> Option<(&'static str, u128, u128)> {
        Some(match unit {
            "s" => ("time", 1, 1),
            "ms" => ("time", 1, 1000),
            "us" | "µs" => ("time", 1, 1_000_000),
            "ns" => ("time", 1, 1_000_000_000),
            "B" => ("bytes", 1, 1),
            "kB" => ("bytes", 1000, 1),
            "MB" => ("bytes", 1_000_000, 1),
            "GB" => ("bytes", 1_000_000_000, 1),
            "KiB" => ("bytes", 1024, 1),
            "MiB" => ("bytes", 1_048_576, 1),
            "GiB" => ("bytes", 1_073_741_824, 1),
            "m" => ("length", 1, 1),
            "mm" => ("length", 1, 1000),
            "cm" => ("length", 1, 100),
            "km" => ("length", 1000, 1),
            "1" => ("ratio", 1, 1),
            "%" => ("ratio", 1, 100),
            _ => return None,
        })
    }
    let (Ok(n), Ok(d)) = (numerator.parse::<u128>(), denominator.parse::<u128>()) else {
        return false;
    };
    if n == 0 || d == 0 {
        return false;
    }
    if from == to {
        return n == d;
    }
    let (Some((from_kind, from_n, from_d)), Some((to_kind, to_n, to_d))) =
        (canonical(from), canonical(to))
    else {
        return false;
    };
    from_kind == to_kind
        && n.checked_mul(from_d)
            .and_then(|value| value.checked_mul(to_n))
            .zip(
                d.checked_mul(from_n)
                    .and_then(|value| value.checked_mul(to_d)),
            )
            .is_some_and(|(left, right)| left == right)
}

fn derive<'a>(
    operation: &DerivationOperation,
    values: &[ValueSemantics<'a>],
) -> Result<ValueSemantics<'a>, String> {
    let origins = values
        .iter()
        .flat_map(|value| value.origins.iter().map(|(id, contract)| (*id, *contract)))
        .collect();
    let selection = values
        .iter()
        .flat_map(|value| {
            value
                .selection
                .iter()
                .map(|(id, contract)| (*id, *contract))
        })
        .collect();
    let calculation = match operation {
        DerivationOperation::Sum => Some("engarg.sum/1"),
        DerivationOperation::Mean => Some("engarg.mean/1"),
        DerivationOperation::Count => Some("engarg.count/1"),
        DerivationOperation::Minimum => Some("engarg.minimum/1"),
        DerivationOperation::Maximum => Some("engarg.maximum/1"),
        DerivationOperation::IdentityEqual => Some("engarg.identity-equal/1"),
        DerivationOperation::AnyIdentityEqual => Some("engarg.any-identity-equal/1"),
        DerivationOperation::Scale { .. } => Some("engarg.scale/1"),
        _ => values.first().and_then(|value| value.calculation),
    };
    let shape = match operation {
        DerivationOperation::Extract { pointer } => match values {
            [value] => value
                .shape
                .select(pointer)
                .ok_or("extraction must select a declared logical quantity")?,
            _ => return Err("extraction requires exactly one input".into()),
        },
        DerivationOperation::Project { fields } => match values {
            [value] => Shape::Object(
                fields
                    .iter()
                    .map(|(name, pointer)| {
                        value
                            .shape
                            .select(pointer)
                            .map(|shape| (name.clone(), shape))
                            .ok_or_else(|| {
                                "projection must preserve declared logical quantity identities"
                                    .to_owned()
                            })
                    })
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err("projection requires exactly one input".into()),
        },
        DerivationOperation::Collect => {
            same_quantity(values)?;
            Shape::Collection(Box::new(values[0].shape.clone()))
        }
        DerivationOperation::IdentityEqual => {
            if values.len() < 2 || values.iter().any(|value| value.shape.quantity().is_none()) {
                return Err("identity comparison requires at least two declared quantities".into());
            }
            let unit = values[0].shape.quantity().expect("checked quantity").1;
            if values.iter().any(|value| {
                value
                    .shape
                    .quantity()
                    .is_none_or(|(_, selected)| selected != unit)
            }) {
                return Err("identity comparison requires the same identity unit".into());
            }
            let collection = matches!(values[0].shape, Shape::Collection(_));
            if values
                .iter()
                .any(|value| matches!(value.shape, Shape::Collection(_)) != collection)
            {
                return Err("IdentityEqual requires the same scalar/collection shape for every operand; use AnyIdentityEqual for scalar membership in a collection".into());
            }
            Shape::Quantity {
                name: "identity_equal".into(),
                unit: "boolean".into(),
            }
        }
        DerivationOperation::AnyIdentityEqual => {
            let [identity, collection] = values else {
                return Err("AnyIdentityEqual requires exactly two inputs: scalar identity then identity collection".into());
            };
            // A declared quantity may itself be the complete array returned by
            // an existing batch operation. Its physical array shape is checked
            // by the runtime; for_each makes that cardinality explicit here.
            if !matches!(identity.shape, Shape::Quantity { .. })
                || !matches!(
                    collection.shape,
                    Shape::Collection(_) | Shape::Quantity { .. }
                )
            {
                return Err("AnyIdentityEqual requires a scalar identity followed by an identity collection or a declared batch quantity; runtime checks the collection shape".into());
            }
            let Some((_, unit)) = identity.shape.quantity() else {
                return Err("AnyIdentityEqual needs a declared scalar identity".into());
            };
            if collection
                .shape
                .quantity()
                .is_none_or(|(_, selected)| selected != unit)
            {
                return Err("AnyIdentityEqual requires matching explicit identity units".into());
            }
            Shape::Quantity {
                name: "any_identity_equal".into(),
                unit: "boolean".into(),
            }
        }
        DerivationOperation::Scale {
            numerator,
            denominator,
            from_unit,
            to_unit,
        } => {
            let [value] = values else {
                return Err("unit scaling requires exactly one quantity".into());
            };
            let (name, unit) = value
                .shape
                .quantity()
                .ok_or("unit scaling requires a declared scalar quantity")?;
            if matches!(value.shape, Shape::Collection(_))
                || unit != from_unit
                || !scale_valid(numerator, denominator, from_unit, to_unit)
            {
                return Err("unit scaling requires an exact supported conversion factor matching its declared input unit".into());
            }
            Shape::Quantity {
                name: name.into(),
                unit: to_unit.clone(),
            }
        }
        operation => {
            let (name, unit) = same_quantity(values)?;
            let (prefix, unit) = match operation {
                DerivationOperation::Count => ("count", "1"),
                DerivationOperation::Sum => ("sum", unit),
                DerivationOperation::Mean => ("mean", unit),
                DerivationOperation::Minimum => ("minimum", unit),
                DerivationOperation::Maximum => ("maximum", unit),
                _ => unreachable!("closed derivation operations"),
            };
            Shape::Quantity {
                name: format!("{prefix}_{name}"),
                unit: unit.into(),
            }
        }
    };
    Ok(ValueSemantics {
        origins,
        selection,
        shape,
        calculation,
    })
}

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
        .filter_map(|value| value.calculation)
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
    if !identity && all.iter().any(|contract| time_errors(required, contract)) {
        errors.push("derived input historical interval or observation-time uncertainty differs from the result contract".into());
    }
    if identity
        && all.iter().any(|contract| {
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
    let authorities = all
        .iter()
        .flat_map(|contract| contract.provenance.required_authorities.iter())
        .collect::<BTreeSet<_>>();
    if required
        .provenance
        .required_authorities
        .iter()
        .any(|authority| !authorities.contains(authority))
        || required.provenance.transformation_history
            && all
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
    let mut errors = Vec::new();
    let mut outputs = BTreeMap::<&str, ValueSemantics<'_>>::new();
    let mut processed = BTreeSet::new();
    loop {
        let before = processed.len();
        for acquisition in &elaboration.acquisitions {
            if processed.contains(acquisition.id.as_str()) {
                continue;
            }
            let references = acquisition
                .bindings
                .iter()
                .map(|binding| &binding.output)
                .chain(acquisition.for_each.iter().map(|binding| &binding.output))
                .collect::<Vec<_>>();
            if references
                .iter()
                .any(|reference| !outputs.contains_key(reference.step_id.as_str()))
            {
                continue;
            }
            processed.insert(acquisition.id.as_str());
            let upstream = match references
                .iter()
                .map(|reference| selected(reference, &outputs))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(values) => values,
                Err(error) => {
                    errors.push(format!(
                        "logical acquisition {} cannot establish selector lineage: {error}",
                        acquisition.id
                    ));
                    continue;
                }
            };
            let selection = upstream
                .iter()
                .flat_map(|value| {
                    value
                        .origins
                        .iter()
                        .chain(&value.selection)
                        .map(|(id, contract)| (*id, *contract))
                })
                .collect::<BTreeMap<_, _>>();
            let mut invalid = false;
            for (source, contract) in &selection {
                let mismatches = selection_errors(&acquisition.contract, contract);
                if !mismatches.is_empty() {
                    invalid = true;
                    errors.push(format!(
                        "logical acquisition {} selector ancestor {} does not preserve {}",
                        acquisition.id,
                        source,
                        mismatches.join(", ")
                    ));
                }
            }
            if invalid {
                continue;
            }
            let shape = Shape::Object(
                acquisition
                    .contract
                    .quantities
                    .iter()
                    .map(|quantity| {
                        let value = Shape::Quantity {
                            name: quantity.name.clone(),
                            unit: quantity.unit.clone(),
                        };
                        (
                            quantity.name.clone(),
                            if acquisition.for_each.is_some() {
                                Shape::Collection(Box::new(value))
                            } else {
                                value
                            },
                        )
                    })
                    .collect(),
            );
            outputs.insert(
                acquisition.id.as_str(),
                ValueSemantics {
                    origins: BTreeMap::from([(acquisition.id.as_str(), &acquisition.contract)]),
                    selection,
                    shape,
                    calculation: None,
                },
            );
        }
        for step in &elaboration.derivations {
            if processed.contains(step.id.as_str())
                || step
                    .inputs
                    .iter()
                    .any(|reference| !outputs.contains_key(reference.step_id.as_str()))
            {
                continue;
            }
            processed.insert(step.id.as_str());
            match step
                .inputs
                .iter()
                .map(|reference| selected(reference, &outputs))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|values| derive(&step.operation, &values))
            {
                Ok(semantic) => {
                    outputs.insert(step.id.as_str(), semantic);
                }
                Err(error) => errors.push(format!(
                    "logical derivation {} cannot establish semantic flow: {error}",
                    step.id
                )),
            }
        }
        if processed.len() == before {
            break;
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
        let calculated = values.iter().any(|value| {
            value
                .calculation
                .is_some_and(|method| method != "engarg.scale/1")
        }) || required.support_type == SupportType::Calculation
            && values.iter().any(|value| value.calculation.is_some());
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
