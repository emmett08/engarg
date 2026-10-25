//! Operator-qualified equivalence between one-item and whole-list interfaces.
use super::{invalid, set_pointer};
use crate::{Error, support_requirements::AcquisitionBlueprint};
use eal_collector_contracts::{
    capability::{CapabilityDescriptor, compatible},
    execution::{DependencyBinding, valid_pointer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The referenced qualification asserts that each named batch quantity is the
/// ordered array of corresponding item quantities, including duplicate inputs,
/// with the same scope, method, observation-time and coverage meaning.
/// Schema similarity alone cannot establish this correspondence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedBatchCorrespondence {
    pub id: String,
    pub qualification_reference: String,
    pub item_capability_id: String,
    pub item_measurement_sha256: String,
    pub batch_capability_id: String,
    pub batch_measurement_sha256: String,
    /// The same logical slot accepts one item or the complete input array.
    /// Operator input projections can map it to different physical pointers.
    pub logical_argument_pointer: String,
    pub item_argument_pointer: String,
    pub batch_argument_pointer: String,
    pub maximum_items: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRouteProof {
    pub correspondence: QualifiedBatchCorrespondence,
    pub item_capability: CapabilityDescriptor,
}

impl QualifiedBatchCorrespondence {
    pub fn validate(&self) -> Result<(), Error> {
        let digest =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !eal_collector_contracts::identifier(&self.id)
            || self.id.len() > 256
            || self.qualification_reference.trim().is_empty()
            || self.qualification_reference.len() > 8192
            || self.qualification_reference.chars().any(char::is_control)
            || self.item_capability_id == self.batch_capability_id
            || !digest(&self.item_measurement_sha256)
            || !digest(&self.batch_measurement_sha256)
            || self.logical_argument_pointer.is_empty()
            || !valid_pointer(&self.logical_argument_pointer)
            || self.item_argument_pointer.is_empty()
            || !valid_pointer(&self.item_argument_pointer)
            || self.batch_argument_pointer.is_empty()
            || !valid_pointer(&self.batch_argument_pointer)
            || !(1..=1024).contains(&self.maximum_items)
        {
            return Err(invalid("invalid qualified batch correspondence"));
        }
        Ok(())
    }
}

pub(crate) fn effective_blueprint(
    original: &AcquisitionBlueprint,
    batch: &CapabilityDescriptor,
    proof: Option<&BatchRouteProof>,
) -> Result<AcquisitionBlueprint, Error> {
    let Some(proof) = proof else {
        return Ok(original.clone());
    };
    let qualification = &proof.correspondence;
    qualification.validate()?;
    let item = &proof.item_capability;
    let fanout = original
        .for_each
        .as_ref()
        .ok_or_else(|| invalid("batch correspondence requires an authored fan-out"))?;
    if item.id != qualification.item_capability_id
        || batch.id != qualification.batch_capability_id
        || item.measurement_identity().map_err(Error::InvalidInput)?
            != qualification.item_measurement_sha256
        || batch.measurement_identity().map_err(Error::InvalidInput)?
            != qualification.batch_measurement_sha256
        || fanout.argument_pointer != qualification.logical_argument_pointer
        || fanout.maximum_items != qualification.maximum_items
        || !compatible(&original.contract, item).adequate
        || !compatible(&original.contract, batch).adequate
    {
        return Err(invalid(
            "batch qualification does not match the item operation and its bounded contract",
        ));
    }
    // A batch has one observation clock. Its qualification must warrant that
    // clock for every returned quantity and preserve the item's source claims.
    let item_semantics = item.semantics.as_ref().expect("qualified item");
    let batch_semantics = batch.semantics.as_ref().expect("qualified batch");
    if item_semantics.time != batch_semantics.time
        || item_semantics.coverage != batch_semantics.coverage
        || item_semantics.provenance != batch_semantics.provenance
        || item_semantics.scope_constraints != batch_semantics.scope_constraints
        || item_semantics.identity_dimensions != batch_semantics.identity_dimensions
        || !original.contract.acceptable_methods.iter().any(|method| {
            item_semantics.methods.contains(method) && batch_semantics.methods.contains(method)
        })
    {
        return Err(invalid(
            "batch and item qualifications differ in time, coverage, provenance, scope or method",
        ));
    }
    if original.contract.quantities.is_empty() {
        return Err(invalid("batch correspondence requires named quantities"));
    }
    let item_input = schema_at(&item.input_schema, &qualification.item_argument_pointer)
        .ok_or_else(|| invalid("item input schema does not expose its qualified item slot"))?;
    let batch_input = schema_at(&batch.input_schema, &qualification.batch_argument_pointer)
        .ok_or_else(|| invalid("batch input schema does not expose its qualified array slot"))?;
    if batch_input.get("type").and_then(Value::as_str) != Some("array")
        || batch_input.get("maxItems").and_then(Value::as_u64)
            != Some(qualification.maximum_items as u64)
        || batch_input.get("items") != Some(item_input)
    {
        return Err(invalid(
            "batch input must be a bounded array of the qualified item input schema",
        ));
    }
    for quantity in &original.contract.quantities {
        let item_quantity = item_semantics
            .quantities
            .iter()
            .find(|q| q.name == quantity.name)
            .expect("compatible item quantity");
        let batch_quantity = batch_semantics
            .quantities
            .iter()
            .find(|q| q.name == quantity.name)
            .expect("compatible batch quantity");
        let item_output = schema_at(&item.output_schema, &item_quantity.output_pointer)
            .ok_or_else(|| invalid("item quantity schema is unavailable"))?;
        let batch_output = schema_at(&batch.output_schema, &batch_quantity.output_pointer)
            .ok_or_else(|| invalid("batch quantity schema is unavailable"))?;
        if batch_output.get("type").and_then(Value::as_str) != Some("array")
            || batch_output.get("items") != Some(item_output)
        {
            return Err(invalid(
                "batch quantities must expose arrays of the corresponding item quantity schema",
            ));
        }
    }
    let mut result = original.clone();
    result.for_each = None;
    set_pointer(&mut result.arguments, &fanout.argument_pointer, Value::Null)?;
    result.bindings.push(DependencyBinding {
        role: Default::default(),
        output: fanout.output.clone(),
        argument_pointer: fanout.argument_pointer.clone(),
    });
    Ok(result)
}

// Deliberately finite schema profile. References/unions need a separately
// qualified resolved schema before this transformation can be admitted.
pub(crate) fn schema_at<'a>(schema: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut schema = schema;
    for component in pointer.split('/').skip(1) {
        let key = component.replace("~1", "/").replace("~0", "~");
        schema = schema.get("properties")?.get(&key)?;
    }
    Some(schema)
}

/// The producer's admitted output schema must bound the complete list before
/// replacing prefix enumeration with a single batch. This preserves behaviour
/// when a real response exceeds the bound: the producer rejects it in both graphs.
pub(crate) fn validate_batch_population(
    original: &AcquisitionBlueprint,
    proof: &BatchRouteProof,
    upstream: &AcquisitionBlueprint,
    producer: &CapabilityDescriptor,
) -> Result<(), Error> {
    let fanout = original
        .for_each
        .as_ref()
        .ok_or_else(|| invalid("batch input requires fan-out"))?;
    if fanout.output.step_id != upstream.id
        || upstream.for_each.is_some()
        || !compatible(&upstream.contract, producer).adequate
    {
        return Err(invalid(
            "batch input requires a directly qualified bounded list source",
        ));
    }
    let quantity = producer
        .semantics
        .as_ref()
        .and_then(|semantics| {
            semantics.quantities.iter().find(|quantity| {
                super::logical_quantity_pointer(&quantity.name) == fanout.output.pointer
            })
        })
        .ok_or_else(|| invalid("batch list input must select one qualified logical quantity"))?;
    let output = schema_at(&producer.output_schema, &quantity.output_pointer)
        .ok_or_else(|| invalid("batch list source schema is unresolved"))?;
    let item = schema_at(
        &proof.item_capability.input_schema,
        &proof.correspondence.item_argument_pointer,
    )
    .ok_or_else(|| invalid("qualified item input schema is unresolved"))?;
    if output.get("type").and_then(Value::as_str) != Some("array")
        || !output
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|maximum| maximum <= fanout.maximum_items as u64)
        || output.get("items") != Some(item)
    {
        return Err(invalid(
            "batch list source must enforce maxItems within the authored fan-out bound and the qualified item schema",
        ));
    }
    Ok(())
}

pub(super) fn evaluation_depends_on(
    elaboration: &crate::support_requirements::SupportElaboration,
    evaluation: &eal_collector_contracts::execution::RequirementEvaluation,
    source: &str,
) -> bool {
    let mut descendants = std::collections::BTreeSet::from([source]);
    loop {
        let before = descendants.len();
        for step in &elaboration.acquisitions {
            if step
                .bindings
                .iter()
                .map(|binding| &binding.output)
                .chain(step.for_each.iter().map(|binding| &binding.output))
                .chain(
                    step.for_each.iter().filter_map(|binding| {
                        binding.stop_on_witness.as_ref()?.equals_output.as_ref()
                    }),
                )
                .any(|reference| descendants.contains(reference.step_id.as_str()))
            {
                descendants.insert(&step.id);
            }
        }
        for step in &elaboration.derivations {
            if step
                .inputs
                .iter()
                .any(|reference| descendants.contains(reference.step_id.as_str()))
            {
                descendants.insert(&step.id);
            }
        }
        if before == descendants.len() {
            break;
        }
    }
    evaluation
        .inputs
        .iter()
        .chain(&evaluation.applicability)
        .chain(&evaluation.challenges)
        .any(|reference| descendants.contains(reference.step_id.as_str()))
}

/// Preserve comparator validity and retain a runtime one-result-per-item check.
/// The latter becomes an applicability gate for every dependent evaluation.
pub(super) fn projection_checks(
    original: &AcquisitionBlueprint,
    projection_id: &str,
) -> (
    Vec<eal_collector_contracts::execution::DerivationStep>,
    Vec<eal_collector_contracts::execution::DerivationStep>,
) {
    use eal_collector_contracts::execution::{
        DerivationOperation, DerivationStep, OutputReference,
    };
    let reference = |step_id: String| OutputReference {
        step_id,
        pointer: String::new(),
    };
    let fanout = original.for_each.as_ref().expect("validated batch");
    let expected_id = format!("{projection_id}_input_count");
    let mut cardinality = vec![DerivationStep {
        id: expected_id.clone(),
        inputs: vec![fanout.output.clone()],
        operation: DerivationOperation::Count,
    }];
    let mut counts = vec![reference(expected_id)];
    for (index, quantity) in original.contract.quantities.iter().enumerate() {
        let id = format!("{projection_id}_output_count_{index}");
        cardinality.push(DerivationStep {
            id: id.clone(),
            inputs: vec![OutputReference {
                step_id: projection_id.into(),
                pointer: super::logical_quantity_pointer(&quantity.name),
            }],
            operation: DerivationOperation::Count,
        });
        counts.push(reference(id));
    }
    cardinality.push(DerivationStep {
        id: format!("{projection_id}_cardinality"),
        inputs: counts,
        operation: DerivationOperation::IdentityEqual,
    });
    let Some(comparison) = fanout
        .stop_on_witness
        .as_ref()
        .and_then(|predicate| predicate.equals_output.as_ref())
    else {
        return (cardinality, vec![]);
    };
    // Membership in a singleton containing the comparison itself validates
    // exactly the same known-individual domain as the runtime witness resolver.
    // Collect/Extract then retains that control's source clocks without changing
    // the batch value. A null/array comparison makes the guard unavailable.
    let singleton_id = format!("{projection_id}_singleton");
    let guard_id = format!("{projection_id}_known_identity");
    let collect_id = format!("{projection_id}_comparison");
    let controls = vec![
        DerivationStep {
            id: singleton_id.clone(),
            inputs: vec![comparison.clone()],
            operation: DerivationOperation::Collect,
        },
        DerivationStep {
            id: guard_id.clone(),
            inputs: vec![comparison.clone(), reference(singleton_id)],
            operation: DerivationOperation::AnyIdentityEqual,
        },
        DerivationStep {
            id: collect_id.clone(),
            inputs: vec![reference(projection_id.into()), reference(guard_id)],
            operation: DerivationOperation::Collect,
        },
        DerivationStep {
            id: format!("{projection_id}_value"),
            inputs: vec![reference(collect_id)],
            operation: DerivationOperation::Extract {
                pointer: "/0".into(),
            },
        },
    ];
    (cardinality, controls)
}
