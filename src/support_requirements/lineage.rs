//! Typed logical shapes and calculation lineage shared by validation and implementation contracts.
use super::SupportElaboration;
use eal_collector_contracts::{
    capability::{LogicalStrength, SupportContract, SupportType},
    execution::{DependencyRole, DerivationOperation, OutputReference},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ClosedMethod {
    Sum,
    Mean,
    Count,
    Minimum,
    Maximum,
    IdentityEqual,
    AnyIdentityEqual,
    Scale,
}
impl ClosedMethod {
    fn for_operation(operation: &DerivationOperation) -> Option<Self> {
        Some(match operation {
            DerivationOperation::Sum => Self::Sum,
            DerivationOperation::Mean => Self::Mean,
            DerivationOperation::Count => Self::Count,
            DerivationOperation::Minimum => Self::Minimum,
            DerivationOperation::Maximum => Self::Maximum,
            DerivationOperation::IdentityEqual => Self::IdentityEqual,
            DerivationOperation::AnyIdentityEqual => Self::AnyIdentityEqual,
            DerivationOperation::Scale { .. } => Self::Scale,
            _ => return None,
        })
    }
    pub(super) fn identifier(self) -> &'static str {
        match self {
            Self::Sum => "engarg.sum/1",
            Self::Mean => "engarg.mean/1",
            Self::Count => "engarg.count/1",
            Self::Minimum => "engarg.minimum/1",
            Self::Maximum => "engarg.maximum/1",
            Self::IdentityEqual => "engarg.identity-equal/1",
            Self::AnyIdentityEqual => "engarg.any-identity-equal/1",
            Self::Scale => "engarg.scale/1",
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct CalculationLineage {
    ancestors: BTreeSet<ClosedMethod>,
    terminal: BTreeSet<ClosedMethod>,
}
impl CalculationLineage {
    fn merge<'a>(values: impl Iterator<Item = &'a Self>) -> Self {
        let mut result = Self::default();
        for value in values {
            result.ancestors.extend(&value.ancestors);
            result.terminal.extend(&value.terminal);
        }
        result.normalise();
        result
    }
    fn normalise(&mut self) {
        if self
            .terminal
            .iter()
            .any(|method| *method != ClosedMethod::Scale)
        {
            self.terminal.remove(&ClosedMethod::Scale);
        }
    }
    fn apply(&mut self, method: ClosedMethod) {
        self.ancestors.insert(method);
        if method == ClosedMethod::Scale {
            if self.terminal.is_empty() {
                self.terminal.insert(method);
            }
        } else {
            self.terminal = BTreeSet::from([method]);
        }
        self.normalise();
    }
    pub(super) fn requires_calculation(&self) -> bool {
        self.ancestors
            .iter()
            .any(|method| *method != ClosedMethod::Scale)
    }
    pub(super) fn is_empty(&self) -> bool {
        self.ancestors.is_empty()
    }
    pub(super) fn methods(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.terminal.iter().map(|method| method.identifier())
    }
}

#[derive(Clone)]
pub(super) enum Representation {
    Unknown,
    Boolean,
    Integer,
    ExactNumber { unit_required: bool },
}

#[derive(Clone)]
pub(super) enum Shape {
    Object(BTreeMap<String, Shape>),
    Quantity {
        name: String,
        unit: String,
        representation: Representation,
    },
    Collection(Box<Shape>),
}

#[derive(Clone)]
pub(super) struct ValueSemantics<'a> {
    pub(super) origins: BTreeMap<&'a str, &'a SupportContract>,
    /// Sources selecting the measured records remain a separate lineage: their
    /// methods need not equal a detail read's method, but their scope and coverage
    /// cannot disappear when the detail read is evaluated.
    pub(super) selection: BTreeMap<&'a str, &'a SupportContract>,
    /// Witness comparison controls execution but does not relabel source scope.
    pub(super) controls: BTreeMap<&'a str, &'a SupportContract>,
    /// Qualified discovery records configure a retrieval without selecting its
    /// measured population. Their own clocks and provenance remain material.
    pub(super) discovery: BTreeMap<&'a str, &'a SupportContract>,
    /// Discovery may pass through single-input projections, never aggregation.
    pub(super) metadata_projection: bool,
    pub(super) shape: Shape,
    pub(super) calculation: CalculationLineage,
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
            Self::Quantity { name, unit, .. } => Some((name, unit)),
            Self::Collection(inner) => match inner.as_ref() {
                Self::Quantity { name, unit, .. } => Some((name, unit)),
                _ => None,
            },
            Self::Object(_) => None,
        }
    }
    pub(super) fn quantities<'a>(&'a self, output: &mut BTreeSet<(&'a str, &'a str)>) {
        match self {
            Self::Quantity { name, unit, .. } => {
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

pub(super) fn selected<'a>(
    reference: &OutputReference,
    outputs: &BTreeMap<&str, ValueSemantics<'a>>,
) -> Result<ValueSemantics<'a>, String> {
    let value = outputs
        .get(reference.step_id.as_str())
        .ok_or("upstream semantic contract is unavailable")?;
    Ok(ValueSemantics {
        origins: value.origins.clone(),
        selection: value.selection.clone(),
        controls: value.controls.clone(),
        discovery: value.discovery.clone(),
        metadata_projection: value.metadata_projection,
        calculation: value.calculation.clone(),
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
    let controls = values
        .iter()
        .flat_map(|value| value.controls.iter().map(|(id, contract)| (*id, *contract)))
        .collect();
    let discovery = values
        .iter()
        .flat_map(|value| {
            value
                .discovery
                .iter()
                .map(|(id, contract)| (*id, *contract))
        })
        .collect();
    let mut calculation = CalculationLineage::merge(values.iter().map(|value| &value.calculation));
    if let Some(method) = ClosedMethod::for_operation(operation) {
        calculation.apply(method);
    }
    let shape = match operation {
        DerivationOperation::Extract { pointer } => match values {
            [value] => value
                .shape
                .select(pointer)
                .ok_or("extraction must select a declared logical quantity")?,
            _ => return Err("extraction requires exactly one input".into()),
        },
        DerivationOperation::Project { fields } | DerivationOperation::ProjectValue { fields } => {
            match values {
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
            }
        }
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
                representation: Representation::Boolean,
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
                representation: Representation::Boolean,
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
                representation: Representation::ExactNumber {
                    unit_required: true,
                },
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
                representation: if matches!(operation, DerivationOperation::Count) {
                    Representation::Integer
                } else {
                    Representation::ExactNumber {
                        unit_required: false,
                    }
                },
            }
        }
    };
    Ok(ValueSemantics {
        origins,
        selection,
        controls,
        discovery,
        metadata_projection: values.len() == 1
            && values[0].metadata_projection
            && matches!(
                operation,
                DerivationOperation::Extract { .. }
                    | DerivationOperation::Project { .. }
                    | DerivationOperation::ProjectValue { .. }
            ),
        shape,
        calculation,
    })
}

impl Shape {
    fn schema(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Object(fields) => {
                json!({"type":"object","properties":fields.iter().map(|(name,shape)|(name.clone(),shape.schema())).collect::<BTreeMap<_,_>>(),"required":fields.keys().collect::<Vec<_>>(),"additionalProperties":false})
            }
            Self::Collection(item) => json!({"type":"array","items":item.schema()}),
            Self::Quantity {
                name,
                unit,
                representation,
            } => {
                let mut schema = match representation {
                    Representation::Unknown => json!({}),
                    Representation::Boolean => json!({"type":"boolean"}),
                    Representation::Integer => json!({"type":"integer","minimum":0}),
                    Representation::ExactNumber { unit_required } => {
                        let mut required = vec!["schema", "numerator", "denominator"];
                        if *unit_required {
                            required.push("unit");
                        }
                        json!({"type":"object","properties":{"schema":{"const":"eal.exact-number/1"},"numerator":{"type":"string","pattern":"^-?[0-9]+$"},"denominator":{"type":"string","pattern":"^[1-9][0-9]*$"},"unit":{"const":unit}},"required":required,"additionalProperties":false})
                    }
                };
                schema["x-engarg-quantity"] = json!({"name":name,"unit":unit});
                schema
            }
        }
    }
}

pub(super) fn infer_outputs(
    elaboration: &SupportElaboration,
) -> (BTreeMap<&str, ValueSemantics<'_>>, Vec<String>) {
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
            let witness = acquisition
                .for_each
                .as_ref()
                .and_then(|binding| binding.stop_on_witness.as_ref());
            if witness
                .and_then(|predicate| predicate.equals_output.as_ref())
                .is_some_and(|reference| !outputs.contains_key(reference.step_id.as_str()))
            {
                continue;
            }
            processed.insert(acquisition.id.as_str());
            if let Some(predicate) = witness {
                let quantity =
                    acquisition.contract.quantities.iter().find(|quantity| {
                        logical_quantity_pointer(&quantity.name) == predicate.pointer
                    });
                let Some(quantity) = quantity else {
                    errors.push(format!("logical acquisition {} witness must select one complete declared logical quantity",acquisition.id));
                    continue;
                };
                if let Some(reference) = &predicate.equals_output {
                    match selected(reference, &outputs) {
                        Ok(value)
                            if matches!(value.shape, Shape::Quantity { .. })
                                && value
                                    .shape
                                    .quantity()
                                    .is_some_and(|(_, unit)| unit == quantity.unit) => {}
                        _ => {
                            errors.push(format!("logical acquisition {} witness comparison requires a scalar logical quantity with matching units",acquisition.id));
                            continue;
                        }
                    }
                }
            }
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
            let mut controls = upstream
                .iter()
                .flat_map(|value| value.controls.iter().map(|(id, contract)| (*id, *contract)))
                .collect::<BTreeMap<_, _>>();
            let mut discovery = upstream
                .iter()
                .flat_map(|value| {
                    value
                        .discovery
                        .iter()
                        .map(|(id, contract)| (*id, *contract))
                })
                .collect::<BTreeMap<_, _>>();
            let mut selection = BTreeMap::new();
            let mut invalid_discovery = false;
            for (index, value) in upstream.iter().enumerate() {
                match acquisition.bindings.get(index).map(|binding| &binding.role) {
                    Some(role @ DependencyRole::Discovery { .. }) => {
                        // A population selector cannot acquire weaker semantics by
                        // being labelled as discovery metadata in a later binding.
                        if !value.metadata_projection
                            || !value.selection.is_empty()
                            || !value.calculation.is_empty()
                            || value.origins.len() != 1
                            || matches!(value.shape, Shape::Collection(_))
                            || value.origins.values().any(|contract| {
                                contract.support_type != SupportType::SourceRetrieval
                                    || contract.logical_strength != LogicalStrength::Clarification
                            })
                        {
                            errors.push(format!(
                                "logical acquisition {} bindings[{index}].role discovery requires SourceRetrieval/Clarification metadata from one scalar source without measurement or population-selection ancestors",
                                acquisition.id
                            ));
                            invalid_discovery = true;
                            continue;
                        }
                        for (source, contract) in &value.origins {
                            if let Err(error) = role.validate_scopes(
                                &serde_json::json!(contract.scope.selectors),
                                &serde_json::json!(acquisition.contract.scope.selectors),
                            ) {
                                errors.push(format!(
                                    "logical acquisition {} bindings[{index}].role discovery scope from {source}: {error}",
                                    acquisition.id
                                ));
                                invalid_discovery = true;
                            }
                        }
                        discovery
                            .extend(value.origins.iter().map(|(id, contract)| (*id, *contract)));
                    }
                    _ => {
                        // Bindings without a role and fan-out selectors retain
                        // the existing population-selection interpretation.
                        selection.extend(
                            value
                                .origins
                                .iter()
                                .chain(&value.selection)
                                .map(|(id, contract)| (*id, *contract)),
                        );
                    }
                }
            }
            if invalid_discovery {
                continue;
            }
            if let Some(reference) = witness.and_then(|predicate| predicate.equals_output.as_ref())
                && let Ok(value) = selected(reference, &outputs)
            {
                controls.extend(
                    value
                        .origins
                        .iter()
                        .chain(&value.selection)
                        .chain(&value.controls)
                        .map(|(id, contract)| (*id, *contract)),
                );
                discovery.extend(
                    value
                        .discovery
                        .iter()
                        .map(|(id, contract)| (*id, *contract)),
                );
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
                            representation: Representation::Unknown,
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
                    controls,
                    discovery,
                    metadata_projection: acquisition.contract.support_type
                        == SupportType::SourceRetrieval
                        && acquisition.contract.logical_strength == LogicalStrength::Clarification
                        && acquisition.for_each.is_none()
                        && acquisition
                            .bindings
                            .iter()
                            .all(|binding| !binding.role.is_selection()),
                    shape,
                    calculation: CalculationLineage::default(),
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
                Ok(value) => {
                    outputs.insert(step.id.as_str(), value);
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
    (outputs, errors)
}

/// Logical shape and units, with raw-provider cardinality left unspecified.
/// No consumer identifiers, credentials or provider execution details are included.
pub fn logical_output_schema(
    elaboration: &SupportElaboration,
    reference: &OutputReference,
) -> Result<serde_json::Value, String> {
    let (outputs, errors) = infer_outputs(elaboration);
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    selected(reference, &outputs).map(|value| value.shape.schema())
}

pub fn logical_quantity_pointer(name: &str) -> String {
    format!("/{}", name.replace('~', "~0").replace('/', "~1"))
}
