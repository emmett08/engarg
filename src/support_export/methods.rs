//! Bind retained formal inputs to native methods without converting plans into evidence.
use super::{PendingSupport, ReasoningInputProjection, SupportRequirementProjection};
use crate::{
    Error,
    domain::{
        ArgumentIndex, BoundReasoningMethod, DefeaterStatus, FormalInputCorrespondence,
        FormalPremiseBinding, InferenceKind, ReasoningMethodKind, SupportRelationship, Warrant,
    },
    eal_export::{EalExport, EalSourceMapping, ReviewRequest, name, quoted},
    support_requirements::{SupportElaboration, SupportRequirement},
};
use eal_collector_contracts::execution::{EvaluationCriterion, EvidenceExecutionPlan};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const INPUT_SCHEMA: &str = "engarg.reasoning-input/1";
const SCOPE_TRANSFER_REQUIRED: &str = "scope_transfer_required";

pub(super) struct MethodProjection<'a> {
    pub authored: &'a EalExport,
    pub elaboration: &'a SupportElaboration,
    pub execution: &'a EvidenceExecutionPlan,
    pub plan_digest: &'a str,
    pub context: &'a Value,
}

pub(super) fn project_inputs(
    projection: &MethodProjection<'_>,
    source: &mut String,
    requests: &mut [ReviewRequest],
    pending: &mut Vec<PendingSupport>,
) -> Result<Vec<ReasoningInputProjection>, Error> {
    let MethodProjection {
        authored,
        elaboration,
        execution,
        plan_digest,
        context,
    } = projection;
    let mut projections = Vec::new();
    let typed_case = authored
        .original_case
        .warrants
        .iter()
        .any(|warrant| warrant.reasoning_method.is_some());
    for warrant in &authored.original_case.warrants {
        if typed_case
            && warrant.reasoning_method.is_none()
            && matches!(
                warrant.inference,
                InferenceKind::Deductive
                    | InferenceKind::Inductive
                    | InferenceKind::Abductive
                    | InferenceKind::Causal
                    | InferenceKind::Counterfactual
            )
        {
            pending.push(PendingSupport {
                kind:"missing_reasoning_binding".into(),requirement_id:None,target_id:warrant.id.clone(),
                description:"This mixed typed case has no explicit bound or unresolved method declaration for the warrant. Its authored structured route is retained for review without a computational certificate.".into(),
            });
        }
        let Some(bound) = warrant
            .reasoning_method
            .as_ref()
            .and_then(|binding| binding.bound())
        else {
            continue;
        };
        let mut projection = ReasoningInputProjection {
            warrant_id: warrant.id.clone(),
            evidence_need_id: bound.input_evidence_need_id.clone(),
            eal_evidence_name: name("reasoning_input", &warrant.id),
            method: bound.method.identifier().into(),
            requirement_id: None,
            output: None,
        };
        let selected = (|| -> Result<_, String> {
            let requirement = requirement_for_need(elaboration, &bound.input_evidence_need_id)?;
            let evaluation = execution
                .evaluations
                .iter()
                .find(|evaluation| evaluation.requirement_id == requirement.id)
                .ok_or_else(|| {
                    "The formal input requirement has no execution evaluation".to_owned()
                })?;
            if !matches!(
                &evaluation.criterion,
                EvaluationCriterion::MethodInput { method } if method == bound.method.identifier()
            ) {
                return Err("The formal input needs a matching method_input admission criterion; formatting admission is separate from premise truth or model adequacy".into());
            }
            let [output] = evaluation.inputs.as_slice() else {
                return Err(
                    "A method_input admission must select exactly one whole typed-input envelope"
                        .into(),
                );
            };
            ensure_method_scope(bound, requirement)?;
            let premise_bindings = bound.formal_premises.iter().map(|premise| {
                let required = requirement_for_need(elaboration, &premise.evidence_need_id)?;
                ensure_evidence_requirement(required, execution)?;
                ensure_method_scope(bound, required)?;
                ensure_identity_units(premise, bound, required, execution)?;
                Ok(json!({"premise_id":premise.premise_id,"input_pointer":premise.input_pointer,"requirement_id":required.id,"correspondence":premise.correspondence}))
            }).collect::<Result<Vec<_>,String>>()?;
            for evidence in &bound.evidence_requirements {
                if evidence.evidence_need_id == bound.input_evidence_need_id {
                    // A formal-input role describes this selector's format only.
                    continue;
                }
                ensure_evidence_requirement(
                    requirement_for_need(elaboration, &evidence.evidence_need_id)?,
                    execution,
                )?;
            }
            Ok((requirement.id.clone(), output.clone(), premise_bindings))
        })();
        let pending_selector = crate::eal_export::reasoning_input_selector(
            &authored.revision,
            &warrant.id,
            &bound.input_evidence_need_id,
        );
        if let Ok((requirement_id, output, premise_bindings)) = &selected {
            let selector = json!({
                "schema":INPUT_SCHEMA,"case_digest":authored.revision,
                "execution_plan_sha256":plan_digest,"requirement_id":requirement_id,
                "output":output,"premise_bindings":premise_bindings,
            });
            let old = format!("  input {}\n", serde_json::to_string(&pending_selector)?);
            let new = format!("  input {}\n", serde_json::to_string(&selector)?);
            if source.matches(&old).count() != 1 {
                return Err(Error::InvalidInput(
                    "REASONING_INPUT_PROJECTION: authored method selector is absent or ambiguous"
                        .into(),
                ));
            }
            *source = source.replacen(&old, &new, 1);
            for request in requests
                .iter_mut()
                .filter(|request| request.evidence_id == projection.eal_evidence_name)
            {
                request.input = selector.clone();
                request.context = (*context).clone();
            }
            projection.requirement_id = Some(requirement_id.clone());
            projection.output = Some(output.clone());
        } else if let Err(reason) = selected {
            pending.push(PendingSupport {
                kind: if reason.starts_with(SCOPE_TRANSFER_REQUIRED) {
                    SCOPE_TRANSFER_REQUIRED.into()
                } else {
                    "reasoning_method_input".into()
                },
                requirement_id: None,
                target_id: warrant.id.clone(),
                description: reason,
            });
        }
        projections.push(projection);
    }
    Ok(projections)
}

/// Bind the formal target to declared measurement identity. A source may add a
/// sample/run selector while retaining the same target population. Changing a
/// target identity needs a separately implemented scope-transfer argument.
fn ensure_method_scope(
    bound: &BoundReasoningMethod,
    requirement: &SupportRequirement,
) -> Result<(), String> {
    let target = &bound.conclusion.scope;
    let required = requirement.contract.as_ref().ok_or_else(|| {
        format!(
            "{SCOPE_TRANSFER_REQUIRED}: requirement {} has no structured source scope for the formal target",
            requirement.id
        )
    })?;
    let source = &required.scope;
    if source.kind != target.kind {
        return Err(format!(
            "{SCOPE_TRANSFER_REQUIRED}: requirement {} source kind {:?} differs from formal target kind {:?}; native scope transfer is unavailable",
            requirement.id, source.kind, target.kind
        ));
    }
    if target.identity_dimensions.iter().any(|dimension| {
        !source.identity_dimensions.contains(dimension) || !source.selectors.contains_key(dimension)
    }) {
        return Err(format!(
            "{SCOPE_TRANSFER_REQUIRED}: requirement {} does not preserve every formal target identity dimension; native scope transfer is unavailable",
            requirement.id
        ));
    }
    for selector in &target.selectors {
        let expected = serde_json::to_value(&selector.value)
            .map_err(|_| "cannot encode the checked method scope selector".to_owned())?;
        if source.selectors.get(&selector.name).is_none_or(|actual| {
            !eal_core::canonical::same_json(
                actual,
                &expected,
                &eal_core::ExecutionLimits::default(),
            )
        }) {
            return Err(format!(
                "{SCOPE_TRANSFER_REQUIRED}: requirement {} has a missing or conflicting formal target selector {:?}; native scope transfer is unavailable",
                requirement.id, selector.name
            ));
        }
    }
    Ok(())
}

fn requirement_for_need<'a>(
    elaboration: &'a SupportElaboration,
    need: &str,
) -> Result<&'a SupportRequirement, String> {
    let mut matching = elaboration
        .requirements
        .iter()
        .filter(|requirement| requirement.evidence_need_ids.iter().any(|id| id == need));
    let selected = matching
        .next()
        .ok_or_else(|| format!("No retained support requirement covers evidence need {need}"))?;
    if matching.next().is_some() {
        return Err(format!(
            "Evidence need {need} has ambiguous support requirements"
        ));
    }
    Ok(selected)
}

/// Admission of a formal envelope cannot establish its empirical premises or
/// the adequacy of an experimental, hypothesis or structural model.
fn ensure_evidence_requirement(
    requirement: &SupportRequirement,
    execution: &EvidenceExecutionPlan,
) -> Result<(), String> {
    let evaluation = execution
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .ok_or_else(|| {
            format!(
                "Evidence requirement {} has no execution evaluation",
                requirement.id
            )
        })?;
    if matches!(
        evaluation.criterion,
        EvaluationCriterion::MethodInput { .. }
    ) {
        return Err(format!(
            "Evidence requirement {} only admits formal input and cannot discharge a premise, design, model or applicability obligation",
            requirement.id
        ));
    }
    // Unavailable or judgement-without-observation criteria remain real EAL
    // gates. They cannot become usable merely because the method input exists.
    Ok(())
}

fn ensure_identity_units(
    premise: &FormalPremiseBinding,
    bound: &BoundReasoningMethod,
    requirement: &SupportRequirement,
    execution: &EvidenceExecutionPlan,
) -> Result<(), String> {
    let FormalInputCorrespondence::IdentityValue {
        source_input_index, ..
    } = &premise.correspondence
    else {
        return Ok(());
    };
    let evaluation = execution
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == requirement.id)
        .ok_or_else(|| {
            format!(
                "Premise requirement {} has no selected source input",
                requirement.id
            )
        })?;
    if evaluation.inputs.get(*source_input_index).is_none() {
        return Err(format!(
            "Premise requirement {} has no source input at index {source_input_index}",
            requirement.id
        ));
    }
    let contract = requirement.contract.as_ref().ok_or_else(|| {
        format!(
            "Premise requirement {} has no explicit quantity/unit contract",
            requirement.id
        )
    })?;
    let units: BTreeSet<_> = contract
        .quantities
        .iter()
        .map(|quantity| quantity.unit.as_str())
        .collect();
    let Some(unit) = units.first().copied().filter(|_| units.len() == 1) else {
        return Err(format!(
            "Premise requirement {} has heterogeneous or ambiguous units; this native profile needs one explicit input basis",
            requirement.id
        ));
    };
    let supported = match bound.method.kind() {
        ReasoningMethodKind::Deductive => matches!(unit, "json" | "1" | "proposition"),
        ReasoningMethodKind::Inductive
            if matches!(premise.input_pointer.as_str(), "/successes" | "/trials") =>
        {
            matches!(unit, "1" | "count")
        }
        ReasoningMethodKind::Inductive | ReasoningMethodKind::Abductive => unit == "1",
        ReasoningMethodKind::Causal | ReasoningMethodKind::Counterfactual => {
            unit == bound.conclusion.unit
        }
    };
    if !supported {
        return Err(format!(
            "Premise requirement {} unit {unit:?} does not match the method's declared input basis {:?}; implicit unit conversion and heterogeneous structural-model units are not supported",
            requirement.id, bound.conclusion.unit
        ));
    }
    Ok(())
}

pub(super) fn native_routes(
    projection: &MethodProjection<'_>,
    inputs: &[ReasoningInputProjection],
    projections: &[SupportRequirementProjection],
    source: &mut String,
    source_map: &mut BTreeMap<String, EalSourceMapping>,
    pending: &mut Vec<PendingSupport>,
) {
    let MethodProjection {
        authored,
        elaboration,
        execution,
        ..
    } = projection;
    let index = ArgumentIndex::new(&authored.original_case);
    let mut native = BTreeSet::new();
    let mut native_grounds = BTreeSet::new();
    for warrant in &authored.original_case.warrants {
        let Some(bound) = warrant
            .reasoning_method
            .as_ref()
            .and_then(|binding| binding.bound())
        else {
            continue;
        };
        let Some(input) = inputs
            .iter()
            .find(|input| input.warrant_id == warrant.id && input.output.is_some())
        else {
            continue;
        };
        let selected = native_evidence(&index, warrant, bound, elaboration, execution, projections);
        let mut evidence = match selected {
            Ok(evidence) => evidence,
            Err(description) => {
                pending.push(PendingSupport {
                    kind: "reasoning_evidence_requirement".into(),
                    requirement_id: None,
                    target_id: warrant.id.clone(),
                    description,
                });
                continue;
            }
        };
        evidence.insert(input.eal_evidence_name.clone());
        evidence.extend(
            warrant
                .backing_evidence_ids
                .iter()
                .map(|id| name("source_review", id)),
        );
        for premise in &warrant.premise_ids {
            if index.claim(premise).is_some() || !native_grounds.insert(premise.clone()) {
                continue;
            }
            let ground_cone = index.attachment_dependency(&[premise]);
            let mut ground_evidence: BTreeSet<_> = projections
                .iter()
                .filter(|projection| ground_cone.contains(projection.target_id.as_str()))
                .map(|projection| projection.eal_evidence_name.clone())
                .collect();
            ground_evidence.extend(
                index
                    .case()
                    .defeaters
                    .iter()
                    .filter(|defeater| {
                        ground_cone.contains(defeater.id.as_str()) && unresolved_material(defeater)
                    })
                    .map(|defeater| name("resolution_review", &defeater.id)),
            );
            let reasoning = name("native_ground_reasoning", premise);
            let argument = name("native_ground_argument", premise);
            source.push_str(&format!("reasoning {reasoning} {{\n  method \"structured/1\"\n  rationale {}\n}}\n\nargument {argument} = [evidence {}] via {reasoning} => {}\n\n",quoted("The acquired support evaluations establish the declared bounded ground under its authored interpretation. This preserves the ground identity and its objections without proving arbitrary prose."),ground_evidence.into_iter().collect::<Vec<_>>().join(", "),name("ground",premise)));
            super::projection::record(
                source_map,
                premise,
                "argument",
                &argument,
                "native_acquired_ground",
            );
            super::projection::record(
                source_map,
                premise,
                "reasoning",
                &reasoning,
                "native_acquired_ground",
            );
        }
        let premises: Vec<_> = warrant
            .premise_ids
            .iter()
            .map(|id| {
                if index.claim(id).is_some() {
                    name("support_claim", id)
                } else {
                    name("ground", id)
                }
            })
            .collect();
        let support = if premises.is_empty() {
            format!(
                "[evidence {}]",
                evidence.into_iter().collect::<Vec<_>>().join(", ")
            )
        } else {
            format!(
                "[evidence {}, premises {}]",
                evidence.into_iter().collect::<Vec<_>>().join(", "),
                premises.join(", ")
            )
        };
        let declaration = name("native_method_argument", &warrant.id);
        source.push_str(&format!("// Acquired native method route. Prose correspondence is explicitly authored.\nargument {declaration} = {support} via {} => {} binding {}\n\n",name("warrant",&warrant.id),name("route",&warrant.id),input.eal_evidence_name));
        super::projection::record(
            source_map,
            &warrant.id,
            "argument",
            &declaration,
            "native_method_support",
        );
        native.insert(warrant.id.as_str());
    }
    for composition in &authored.original_case.compositions {
        if composition.relationship == SupportRelationship::Cumulative
            || composition.selected_warrant_ids.is_empty()
            || !composition
                .selected_warrant_ids
                .iter()
                .all(|id| native.contains(id.as_str()))
        {
            continue;
        }
        let declaration = name("native_composition_argument", &composition.conclusion_id);
        let premises = composition
            .selected_warrant_ids
            .iter()
            .map(|id| name("route", id))
            .collect::<Vec<_>>();
        let reasoning = name("native_composition_reasoning", &composition.conclusion_id);
        source.push_str(&format!("reasoning {reasoning} {{\n  method \"structured/1\"\n  rationale {}\n}}\n\nargument {declaration} = [premises {}] via {reasoning} => {}\n\n",quoted("Compose only explicitly selected typed routes for this same qualified conclusion. This propagates their checked computations and acquired support gates; prose/formal correspondence remains authored."),premises.join(", "),authored.claim_names[&composition.conclusion_id]));
        super::projection::record(
            source_map,
            &composition.conclusion_id,
            "argument",
            &declaration,
            "native_typed_composition",
        );
        super::projection::record(
            source_map,
            &composition.conclusion_id,
            "reasoning",
            &reasoning,
            "native_typed_composition",
        );
    }
}

fn native_evidence(
    index: &ArgumentIndex<'_>,
    warrant: &Warrant,
    bound: &BoundReasoningMethod,
    elaboration: &SupportElaboration,
    execution: &EvidenceExecutionPlan,
    projections: &[SupportRequirementProjection],
) -> Result<BTreeSet<String>, String> {
    let cone = index.warrant_dependency(&warrant.id, true);
    if index.case().defeaters.iter().any(|defeater| {
        cone.contains(defeater.target_id.as_str())
            && index
                .case()
                .assumptions
                .iter()
                .any(|assumption| assumption.id == defeater.target_id)
    }) {
        return Err("A challenge targets an applicability assumption. The native profile retains this route as unresolved until that assumption's review and challenge semantics can be preserved together; support predicates alone do not dismiss an objection".into());
    }
    let mut selected: BTreeSet<String> = projections
        .iter()
        .filter(|projection| cone.contains(projection.target_id.as_str()))
        .map(|projection| projection.eal_evidence_name.clone())
        .collect();
    selected.extend(
        index
            .case()
            .defeaters
            .iter()
            .filter(|defeater| cone.contains(defeater.id.as_str()) && unresolved_material(defeater))
            .map(|defeater| name("resolution_review", &defeater.id)),
    );
    for need in bound
        .formal_premises
        .iter()
        .map(|premise| premise.evidence_need_id.as_str())
        .chain(
            bound
                .evidence_requirements
                .iter()
                .map(|required| required.evidence_need_id.as_str()),
        )
    {
        if need == bound.input_evidence_need_id {
            continue;
        }
        let requirement = requirement_for_need(elaboration, need)?;
        ensure_evidence_requirement(requirement, execution)?;
        let projection = projections
            .iter()
            .find(|projection| projection.requirement_id == requirement.id)
            .ok_or_else(|| format!("Evidence requirement {} has no EAL gate", requirement.id))?;
        selected.insert(projection.eal_evidence_name.clone());
    }
    for assumption in &bound.assumption_ids {
        let required: Vec<_> = elaboration
            .requirements
            .iter()
            .filter(|requirement| requirement.target_id == *assumption)
            .collect();
        if required.is_empty() {
            return Err(format!(
                "Named assumption {assumption} has no acquired support requirement"
            ));
        }
        for requirement in required {
            ensure_evidence_requirement(requirement, execution)?;
            selected.insert(name("support_evaluation", &requirement.id));
        }
    }
    Ok(selected)
}

fn unresolved_material(defeater: &crate::domain::Defeater) -> bool {
    defeater.material
        && matches!(
            defeater.status,
            DefeaterStatus::Open | DefeaterStatus::Sustained
        )
}
