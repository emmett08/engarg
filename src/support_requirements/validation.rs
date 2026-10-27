//! Coverage and logical graph admission preserve the argument's source obligations.
use super::{SupportElaboration, SupportRequirement};
use crate::domain::{ArgumentCase, ArgumentIndex, DefeaterStatus};
use eal_collector_contracts::{
    capability::{LogicalStrength, canonical_identity},
    execution::{
        DerivationOperation, EvaluationCriterion, OutputReference, RequiredCoverage, valid_pointer,
    },
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RECORDS: usize = 4096;
fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
fn id(value: &str) -> bool {
    value.len() <= 256 && eal_collector_contracts::identifier(value)
}

/// Every represented material component has an obligation, including supplied grounds.
pub fn material_targets(case: &ArgumentCase) -> BTreeSet<String> {
    case.grounds
        .iter()
        .map(|value| value.id.clone())
        .chain(case.warrants.iter().map(|value| value.id.clone()))
        .chain(
            case.assumptions
                .iter()
                .filter(|value| value.critical)
                .map(|value| value.id.clone()),
        )
        .chain(
            case.defeaters
                .iter()
                .filter(|value| {
                    value.material && value.status != DefeaterStatus::InapplicableAfterRevision
                })
                .map(|value| value.id.clone()),
        )
        .chain(
            case.evidence_plan
                .iter()
                .map(|value| value.target_id.clone()),
        )
        .collect()
}

fn same(left: &impl serde::Serialize, right: &impl serde::Serialize) -> bool {
    matches!((canonical_identity(left),canonical_identity(right)),(Ok(left),Ok(right)) if left==right)
}

fn execution_authority(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(name, value)| {
            matches!(
                name.as_str(),
                "provider_id"
                    | "capability_id"
                    | "tool_id"
                    | "connection_id"
                    | "executable"
                    | "command"
                    | "argv"
                    | "credentials"
            ) || execution_authority(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(execution_authority),
        _ => false,
    }
}

fn numeric_threshold(value: &serde_json::Value, unit: &str) -> bool {
    if let Some(number) = value.as_number() {
        let encoded = number.to_string();
        let (mantissa, exponent) = encoded
            .split_once(['e', 'E'])
            .map_or((encoded.as_str(), "0"), |(mantissa, exponent)| {
                (mantissa, exponent)
            });
        return mantissa.len() <= 4096
            && exponent
                .parse::<i32>()
                .is_ok_and(|exponent| exponent.unsigned_abs() <= 4096);
    }
    let Some(fields) = value.as_object() else {
        return false;
    };
    fn integer(value: Option<&serde_json::Value>) -> Option<&str> {
        value.and_then(serde_json::Value::as_str).filter(|value| {
            let digits = value.strip_prefix('-').unwrap_or(value);
            !digits.is_empty()
                && value.len() <= 8192
                && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
    }
    fields.get("schema").and_then(serde_json::Value::as_str) == Some("eal.exact-number/1")
        && fields.keys().all(|name| {
            matches!(
                name.as_str(),
                "schema" | "numerator" | "denominator" | "unit"
            )
        })
        && integer(fields.get("numerator")).is_some()
        && integer(fields.get("denominator"))
            .is_some_and(|denominator| denominator.bytes().any(|byte| matches!(byte, b'1'..=b'9')))
        && fields
            .get("unit")
            .is_none_or(|declared| declared.as_str() == Some(unit))
}

fn criterion_errors(requirement: &SupportRequirement) -> Vec<String> {
    let mut errors = Vec::new();
    match &requirement.acceptance {
        EvaluationCriterion::Numeric {
            value_unit,
            threshold_unit,
            threshold,
            ..
        } => {
            if value_unit != threshold_unit
                || !text(value_unit, 128)
                || !numeric_threshold(threshold, threshold_unit)
            {
                errors.push(format!("support requirement {} numerical acceptance needs a numeric threshold and matching explicit units",requirement.id));
            }
            if requirement.contract.as_ref().is_some_and(|contract| {
                !contract
                    .quantities
                    .iter()
                    .any(|quantity| quantity.unit == *value_unit)
            }) {
                errors.push(format!("support requirement {} acceptance unit is absent from its measurement contract",requirement.id));
            }
        }
        EvaluationCriterion::Judgement { criterion, scope } => {
            if !text(criterion, 8192) {
                errors.push(format!(
                    "support requirement {} needs an explicit judgement criterion",
                    requirement.id
                ));
            }
            if let Some(scope) = scope
                && requirement.contract.as_ref().is_none_or(|contract| {
                    contract.scope != *scope
                        || contract.support_type
                            != eal_collector_contracts::capability::SupportType::Judgement
                        || contract.logical_strength != LogicalStrength::Judgement
                })
            {
                errors.push(format!("support requirement {} scoped judgement must preserve its Judgement contract's exact subject scope",requirement.id));
            }
        }
        EvaluationCriterion::Unavailable { reason } if !text(reason, 8192) => errors.push(format!(
            "support requirement {} unavailable support needs an explicit reason",
            requirement.id
        )),
        _ => {}
    }
    if requirement.contract.is_none()
        && (requirement.gaps.is_empty()
            || !matches!(
                requirement.acceptance,
                EvaluationCriterion::Unavailable { .. } | EvaluationCriterion::Judgement { .. }
            ))
    {
        errors.push(format!("support requirement {} unknown contract needs an explicit gap and unavailable or judgement criterion",requirement.id));
    }
    errors
}

pub fn validate_elaboration(case: &ArgumentCase, elaboration: &SupportElaboration) -> Vec<String> {
    let mut errors = Vec::new();
    if !super::supported_schema(&elaboration.schema) {
        errors.push("unsupported support elaboration schema".into());
    }
    if elaboration.requirements.is_empty()
        || elaboration.requirements.len() > MAX_RECORDS
        || elaboration.acquisitions.len() + elaboration.derivations.len() > MAX_RECORDS
        || elaboration.evaluations.len() > MAX_RECORDS
        || elaboration.revisions.len() > MAX_RECORDS
    {
        return vec![
            "support elaboration exceeds its finite record limits or omits requirements".into(),
        ];
    }
    if elaboration.acquisition_revisions.len() > MAX_RECORDS {
        errors.push("acquisition revisions exceed the finite record limit".into());
    }
    let mut source_revisions = BTreeSet::new();
    for revision in &elaboration.acquisition_revisions {
        if !id(&revision.previous_step_id)
            || !eal_collector_contracts::execution::valid_digest(&revision.previous_step_sha256)
            || !text(&revision.reason, 8192)
            || !source_revisions
                .insert((&revision.previous_step_id, &revision.previous_step_sha256))
            || revision.replacement_ids.len() > MAX_RECORDS
            || revision.replacement_ids.iter().any(|value| !id(value))
            || revision
                .replacement_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != revision.replacement_ids.len()
            || matches!(
                revision.disposition,
                super::AcquisitionDisposition::Deferred | super::AcquisitionDisposition::Retired
            ) && !revision.replacement_ids.is_empty()
        {
            errors.push(format!("acquisition revision {} needs a unique exact prior identity, bounded reason and replacement IDs consistent with its disposition", revision.previous_step_id));
        }
    }
    let index = ArgumentIndex::new(case);
    let need_map: BTreeMap<_, _> = case
        .evidence_plan
        .iter()
        .map(|need| (need.id.as_str(), need))
        .collect();
    let mut requirements = BTreeMap::new();
    let mut covered_targets = BTreeSet::new();
    let mut covered_needs = BTreeSet::new();
    for requirement in &elaboration.requirements {
        if requirement.role == super::SupportRequirementRole::Support
            && case
                .defeaters
                .iter()
                .any(|defeater| defeater.id == requirement.target_id)
        {
            errors.push(format!("support requirement {} needs an explicit challenge_activation or challenge_resolution role for its defeater target",requirement.id));
        }
        if requirement.role != super::SupportRequirementRole::Support
            && !case
                .defeaters
                .iter()
                .any(|defeater| defeater.id == requirement.target_id)
        {
            errors.push(format!(
                "support requirement {} challenge role requires a defeater target",
                requirement.id
            ));
        }
        if canonical_identity(requirement).is_err() {
            errors.push(format!(
                "support requirement {} exceeds canonical JSON limits",
                requirement.id
            ));
        }
        if !id(&requirement.id)
            || requirements
                .insert(requirement.id.as_str(), requirement)
                .is_some()
        {
            errors.push("support requirement IDs must be unique and bounded".into());
        }
        if index.node(&requirement.target_id).is_none() {
            errors.push(format!(
                "support requirement {} names unknown target {}",
                requirement.id, requirement.target_id
            ));
        } else {
            covered_targets.insert(requirement.target_id.clone());
        }
        if requirement.evidence_need_ids.len() > MAX_RECORDS
            || requirement
                .evidence_need_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != requirement.evidence_need_ids.len()
        {
            errors.push(format!(
                "support requirement {} repeats evidence need IDs or exceeds limits",
                requirement.id
            ));
        }
        let mut ordered = requirement.evidence_need_ids.iter().collect::<Vec<_>>();
        ordered.sort();
        let mut expected_criteria = Vec::new();
        for need_id in ordered {
            match need_map.get(need_id.as_str()) {
                None => errors.push(format!(
                    "support requirement {} names unknown evidence need {need_id}",
                    requirement.id
                )),
                Some(need) => {
                    if need.target_id != requirement.target_id {
                        errors.push(format!(
                            "support requirement {} changed evidence need {need_id} target",
                            requirement.id
                        ));
                    }
                    covered_needs.insert(need_id.as_str());
                    expected_criteria.push(need.acceptance_criterion.clone());
                }
            }
        }
        if requirement.original_acceptance_criteria != expected_criteria {
            errors.push(format!("support requirement {} must preserve the exact original evidence acceptance clauses",requirement.id));
        }
        if !text(&requirement.acceptance_rationale, 8192)
            || requirement.discriminating_evidence.len() > 128
            || requirement
                .discriminating_evidence
                .iter()
                .any(|value| !text(value, 8192))
            || requirement.gaps.len() > 128
            || requirement
                .gaps
                .iter()
                .any(|gap| !text(&gap.description, 8192))
        {
            errors.push(format!(
                "support requirement {} needs bounded explicit reasoning, discriminators and gaps",
                requirement.id
            ));
        }
        if let Some(contract) = &requirement.contract {
            errors.extend(contract.diagnostics().into_iter().map(|diagnostic| {
                format!(
                    "support requirement {} has invalid measurement contract at {} [{}]: {}",
                    requirement.id, diagnostic.path, diagnostic.code, diagnostic.message,
                )
            }));
        }
        errors.extend(criterion_errors(requirement));
    }
    for target in material_targets(case).difference(&covered_targets) {
        errors.push(format!(
            "material argument target {target} requires a support requirement"
        ));
    }
    for need in case
        .evidence_plan
        .iter()
        .filter(|need| !covered_needs.contains(need.id.as_str()))
    {
        errors.push(format!(
            "evidence need {} requires an explicit support requirement",
            need.id
        ));
    }
    errors.extend(graph_errors(elaboration, &requirements));
    errors.extend(super::alternatives::validate(case, elaboration));
    errors.extend(super::validate_semantic_flow(elaboration));
    let mut revisions = BTreeSet::new();
    for revision in &elaboration.revisions {
        // Removed targets retain a digest/reason record; previous-case admission
        // verifies that history in validate_revision.
        if !id(&revision.requirement_id)
            || !revisions.insert(&revision.requirement_id)
            || !eal_collector_contracts::execution::valid_digest(
                &revision.previous_requirement_sha256,
            )
            || !text(&revision.reason, 8192)
        {
            errors.push("requirement revisions need bounded IDs, unique records, previous digests and reasons".into());
        }
    }
    errors
}

fn graph_errors(
    elaboration: &SupportElaboration,
    requirements: &BTreeMap<&str, &SupportRequirement>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut nodes = BTreeMap::<&str, Vec<&OutputReference>>::new();
    for acquisition in &elaboration.acquisitions {
        if let Some(spec) = &acquisition.operation_spec
            && let Err(error) = spec.validate()
        {
            errors.push(format!(
                "logical acquisition {} has invalid operation specification: {error}",
                acquisition.id
            ));
        }
        if !id(&acquisition.id)
            || !acquisition.arguments.is_object()
            || nodes
                .insert(
                    &acquisition.id,
                    acquisition
                        .bindings
                        .iter()
                        .map(|binding| &binding.output)
                        .chain(acquisition.for_each.iter().map(|binding| &binding.output))
                        .chain(acquisition.for_each.iter().filter_map(|binding| {
                            binding.stop_on_witness.as_ref()?.equals_output.as_ref()
                        }))
                        .collect(),
                )
                .is_some()
        {
            errors.push(
                "logical acquisitions require a unique bounded ID and object arguments".into(),
            );
        }
        for error in acquisition.contract.diagnostics() {
            errors.push(format!(
                "logical acquisition {} has invalid contract at acquisitions.{}.contract: {error}",
                acquisition.id, acquisition.id
            ));
        }
        if canonical_identity(&acquisition.arguments).is_err() {
            errors.push("logical acquisition arguments exceed canonical JSON limits".into());
        }
        if execution_authority(&acquisition.arguments) {
            errors.push(format!(
                "logical acquisition {} cannot select execution authority",
                acquisition.id
            ));
        }
        for (index, binding) in acquisition.bindings.iter().enumerate() {
            if !valid_pointer(&binding.argument_pointer)
                || acquisition
                    .arguments
                    .pointer(&binding.argument_pointer)
                    .is_none()
            {
                errors.push(format!(
                    "logical acquisition {} binding requires an existing argument pointer: acquisitions.{}.bindings[{index}].argument_pointer {:?} does not identify a declared slot in arguments; declare the destination before binding an upstream output",
                    acquisition.id, acquisition.id, binding.argument_pointer
                ));
            }
        }
        if let Some(binding) = &acquisition.for_each {
            if !(1..=1024).contains(&binding.maximum_items) {
                errors.push(format!(
                    "logical acquisition {} requires bounded fan-out: acquisitions.{}.for_each.maximum_items received {}, expected 1..=1024",
                    acquisition.id, acquisition.id, binding.maximum_items
                ));
            }
            if !valid_pointer(&binding.argument_pointer)
                || acquisition
                    .arguments
                    .pointer(&binding.argument_pointer)
                    .is_none()
            {
                errors.push(format!(
                    "logical acquisition {} requires bounded fan-out and explicit pointers: acquisitions.{}.for_each.argument_pointer {:?} does not identify a declared slot in arguments",
                    acquisition.id, acquisition.id, binding.argument_pointer
                ));
            }
            if let Some(predicate) = &binding.stop_on_witness {
                if !valid_pointer(&predicate.pointer) {
                    errors.push(format!(
                        "logical acquisition {} requires bounded fan-out and explicit pointers: acquisitions.{}.for_each.stop_on_witness.pointer {:?} is not a valid JSON pointer",
                        acquisition.id, acquisition.id, predicate.pointer
                    ));
                }
                if predicate.equals.is_some() == predicate.equals_output.is_some() {
                    errors.push(format!(
                        "logical acquisition {} requires bounded fan-out and explicit pointers: acquisitions.{}.for_each.stop_on_witness must specify exactly one of equals and equals_output",
                        acquisition.id, acquisition.id
                    ));
                }
            }
        }
    }
    for derivation in &elaboration.derivations {
        if !id(&derivation.id)
            || derivation.inputs.is_empty()
            || nodes
                .insert(&derivation.id, derivation.inputs.iter().collect())
                .is_some()
        {
            errors.push("logical derivations require inputs and a unique bounded ID".into());
        }
        match &derivation.operation {
            DerivationOperation::Extract { pointer } if !valid_pointer(pointer) => {
                errors.push(format!(
                    "logical derivation {} requires a valid extraction pointer",
                    derivation.id
                ))
            }
            DerivationOperation::Project { fields }
            | DerivationOperation::ProjectValue { fields }
                if derivation.inputs.len() != 1
                    || fields.is_empty()
                    || fields.len() > 256
                    || fields
                        .iter()
                        .any(|(name, pointer)| !id(name) || !valid_pointer(pointer)) =>
            {
                errors.push(format!(
                    "logical derivation {} requires one input and explicit projection fields",
                    derivation.id
                ))
            }
            DerivationOperation::IdentityEqual if derivation.inputs.len() < 2 => {
                errors.push(format!(
                    "logical derivation {} requires at least two identities",
                    derivation.id
                ))
            }
            DerivationOperation::AnyIdentityEqual if derivation.inputs.len() != 2 => {
                errors.push(format!(
                    "logical derivation {} requires a scalar identity and an identity collection",
                    derivation.id
                ))
            }
            DerivationOperation::Scale {
                numerator,
                denominator,
                from_unit,
                to_unit,
            } if !text(numerator, 4096)
                || !text(denominator, 4096)
                || !text(from_unit, 128)
                || !text(to_unit, 128) =>
            {
                errors.push(format!(
                    "logical derivation {} needs explicit scale terms and units",
                    derivation.id
                ))
            }
            _ => {}
        }
    }
    let valid_ref = |reference: &&OutputReference| {
        nodes.contains_key(reference.step_id.as_str()) && valid_pointer(&reference.pointer)
    };
    for (node, refs) in &nodes {
        if refs
            .iter()
            .any(|reference| !valid_ref(reference) || reference.step_id == *node)
        {
            errors.push(format!(
                "logical step {node} has an unknown, self or malformed dependency"
            ));
        }
    }
    let mut ready = BTreeSet::new();
    loop {
        let before = ready.len();
        for (node, refs) in &nodes {
            if refs
                .iter()
                .all(|reference| ready.contains(reference.step_id.as_str()))
            {
                ready.insert(*node);
            }
        }
        if ready.len() == nodes.len() {
            break;
        }
        if ready.len() == before {
            errors.push(
                "logical source graph contains a dependency cycle or unavailable reference".into(),
            );
            break;
        }
    }
    let mut evaluations = BTreeSet::new();
    let mut evaluated_requirements = BTreeSet::new();
    for evaluation in &elaboration.evaluations {
        if !id(&evaluation.id)
            || !evaluations.insert(&evaluation.id)
            || !evaluated_requirements.insert(evaluation.requirement_id.as_str())
        {
            errors.push(
                "support evaluations need unique IDs and one evaluation per requirement".into(),
            );
        }
        match requirements.get(evaluation.requirement_id.as_str()) {
            None => errors.push(format!(
                "evaluation {} names an unknown requirement",
                evaluation.id
            )),
            Some(requirement) => {
                if !same(&evaluation.criterion, &requirement.acceptance) {
                    errors.push(format!(
                        "evaluation {} changed its requirement acceptance criterion",
                        evaluation.id
                    ));
                }
                if evaluation.max_age_seconds
                    != requirement
                        .contract
                        .as_ref()
                        .and_then(|contract| contract.time.max_age_seconds)
                {
                    errors.push(format!(
                        "evaluation {} changed its requirement freshness",
                        evaluation.id
                    ));
                }
                if let Some(contract) = &requirement.contract {
                    let expected = if contract.support_type
                        == eal_collector_contracts::capability::SupportType::Calculation
                        && contract.population.is_some()
                    {
                        RequiredCoverage::CompleteEnumeration
                    } else if contract
                        .acceptable_methods
                        .iter()
                        .any(|method| method == "engarg.any-identity-equal/1")
                    {
                        RequiredCoverage::Witness
                    } else {
                        match contract.logical_strength {
                            LogicalStrength::ExistentialWitness => RequiredCoverage::Witness,
                            LogicalStrength::CompleteEnumeration => {
                                RequiredCoverage::CompleteEnumeration
                            }
                            LogicalStrength::StatisticalEstimate => RequiredCoverage::Statistical,
                            _ => RequiredCoverage::Unspecified,
                        }
                    };
                    if evaluation.required_coverage != expected {
                        errors.push(format!(
                            "evaluation {} changed its requirement logical coverage",
                            evaluation.id
                        ));
                    }
                }
            }
        }
        if evaluation.inputs.is_empty()
            && !matches!(
                evaluation.criterion,
                EvaluationCriterion::Unavailable { .. } | EvaluationCriterion::Judgement { .. }
            )
        {
            errors.push(format!(
                "evaluation {} requires source or derivation inputs",
                evaluation.id
            ));
        }
        if evaluation
            .inputs
            .iter()
            .chain(&evaluation.applicability)
            .chain(&evaluation.challenges)
            .chain(&evaluation.coverage)
            .any(|reference| !valid_ref(&reference))
        {
            errors.push(format!(
                "evaluation {} has an unknown or malformed output reference",
                evaluation.id
            ));
        }
    }
    for id in requirements
        .keys()
        .filter(|id| !evaluated_requirements.contains(**id))
    {
        errors.push(format!(
            "support requirement {id} requires a separate evaluation"
        ));
    }
    let mut used = BTreeSet::new();
    let mut pending = elaboration
        .evaluations
        .iter()
        .flat_map(|evaluation| {
            evaluation
                .inputs
                .iter()
                .chain(&evaluation.applicability)
                .chain(&evaluation.challenges)
                .chain(&evaluation.coverage)
        })
        .map(|reference| reference.step_id.as_str())
        .collect::<Vec<_>>();
    while let Some(step) = pending.pop() {
        if used.insert(step)
            && let Some(references) = nodes.get(step)
        {
            pending.extend(
                references
                    .iter()
                    .map(|reference| reference.step_id.as_str()),
            );
        }
    }
    for step in nodes.keys().filter(|step| !used.contains(**step)) {
        errors.push(format!(
            "unused logical step {step} cannot request evidence acquisition or implementation"
        ));
    }
    errors
}

/// Revisions retain source obligations; changed semantic contracts need an explicit record.
pub fn validate_revision(
    previous_case: &ArgumentCase,
    previous: &SupportElaboration,
    current_case: &ArgumentCase,
    current: &SupportElaboration,
) -> Vec<String> {
    validate_revision_mode(previous_case, previous, current_case, current, true)
}

/// Explicit argument revisions may change an evidence criterion. Its affected
/// requirement still needs the exact previous digest and an authored reason.
pub fn validate_argument_revision(
    previous_case: &ArgumentCase,
    previous: &SupportElaboration,
    current_case: &ArgumentCase,
    current: &SupportElaboration,
) -> Vec<String> {
    validate_revision_mode(previous_case, previous, current_case, current, false)
}

fn validate_revision_mode(
    previous_case: &ArgumentCase,
    previous: &SupportElaboration,
    current_case: &ArgumentCase,
    current: &SupportElaboration,
    preserve_evidence_needs: bool,
) -> Vec<String> {
    let mut errors = validate_elaboration(current_case, current);
    let previous_requirements: BTreeMap<_, _> = previous
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect();
    let current_requirements: BTreeMap<_, _> = current
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect();
    let revisions: BTreeMap<_, _> = current
        .revisions
        .iter()
        .map(|revision| (revision.requirement_id.as_str(), revision))
        .collect();
    let current_needs: BTreeMap<_, _> = current_case
        .evidence_plan
        .iter()
        .map(|need| (need.id.as_str(), need))
        .collect();
    let current_index = ArgumentIndex::new(current_case);
    let changed_alternatives = super::alternatives::changed_requirement_bindings(
        previous_case,
        previous,
        current_case,
        current,
    );
    for need in &previous_case.evidence_plan {
        if preserve_evidence_needs
            && current_index.node(&need.target_id).is_some()
            && current_needs.get(need.id.as_str()).is_none_or(|current| {
                current.target_id != need.target_id
                    || current.acceptance_criterion != need.acceptance_criterion
            })
        {
            errors.push(format!("support revision must preserve evidence need {} and its original acceptance clause for a retained target",need.id));
        }
    }
    for revision in &current.revisions {
        if previous_requirements
            .get(revision.requirement_id.as_str())
            .is_none_or(|requirement| {
                canonical_identity(requirement)
                    .map(|digest| digest != revision.previous_requirement_sha256)
                    .unwrap_or(true)
            })
        {
            errors.push(format!(
                "support revision {} does not name an exact previous requirement identity",
                revision.requirement_id
            ));
        }
    }
    for (id, requirement) in previous_requirements {
        match current_requirements.get(id) {
            None if current_index.node(&requirement.target_id).is_some() && !super::alternatives::retired_alternative_requirement(previous_case,previous,current_case,requirement) =>
            {
                errors.push(format!(
                    "support revision dropped requirement {id} for a retained target"
                ))
            }
            None if !revisions.contains_key(id)=>errors.push(format!("support revision removed requirement {id} without its prior-digest disposition and reason")),
            Some(current)
                if (!same(requirement, *current) || changed_alternatives.contains(id))
                    && canonical_identity(requirement)
                        .map(|digest| {
                            revisions.get(id).is_none_or(|revision| {
                                revision.previous_requirement_sha256 != digest
                            })
                        })
                        .unwrap_or(true) =>
            {
                errors.push(format!("support requirement {id} changed without an exact prior-digest revision and reason"));
            }
            _ => {}
        }
    }
    errors
}
