//! Check formal correspondences without treating plans as observed evidence.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(case: &ArgumentCase, errors: &mut Vec<String>) {
    let needs: BTreeMap<_, _> = case
        .evidence_plan
        .iter()
        .map(|need| (need.id.as_str(), need))
        .collect();
    let registry = eal_core::methods::default_registry();
    for warrant in &case.warrants {
        let Some(binding) = &warrant.reasoning_method else {
            continue;
        };
        let label = format!("warrant {} reasoning_method", warrant.id);
        if binding.kind().map(ReasoningMethodKind::inference) != Some(warrant.inference) {
            errors.push(format!("{label} method does not match its inference mode"));
        }
        let ReasoningMethodBinding::Bound(bound) = binding else {
            if let ReasoningMethodBinding::Unresolved {
                reasons,
                evidence_need_ids,
                ..
            } = binding
            {
                if reasons.is_empty() || reasons.len() > 32 {
                    errors.push(format!("{label} unresolved needs 1–32 explicit reasons"));
                }
                for reason in reasons {
                    nonempty(errors, &format!("{label} unresolved reason"), reason);
                }
                if evidence_need_ids.len() > 128 {
                    errors.push(format!("{label} unresolved exceeds 128 evidence needs"));
                }
                unique_needs(&label, evidence_need_ids, &needs, errors);
            }
            continue;
        };
        if bound.conclusion.claim_id != warrant.conclusion_id {
            errors.push(format!(
                "{label} conclusion.claim_id must equal the warrant conclusion"
            ));
        }
        let method = bound.method.identifier();
        if bound.formal_premises.is_empty()
            || bound.formal_premises.len() > 128
            || bound.evidence_requirements.is_empty()
            || bound.evidence_requirements.len() > 128
            || bound.assumption_ids.len() > 128
        {
            errors.push(format!(
                "{label} exceeds bounded premise, evidence or assumption lists"
            ));
        }
        let proposition = bound
            .conclusion
            .proposition(&bound.method)
            .and_then(|value| {
                serde_json::from_value::<eal_core::semantic::Proposition>(value)
                    .map_err(|error| error.to_string())
            });
        match proposition {
            Ok(proposition) => {
                errors.extend(
                    eal_core::propositions::proposition_errors(
                        &proposition,
                        Some(method),
                        registry,
                    )
                    .into_iter()
                    .map(|message| format!("{label} {message}")),
                );
            }
            Err(error) => errors.push(format!("{label} invalid formal proposition: {error}")),
        }
        errors.extend(
            eal_core::methods::validate_query(method, &bound.method.query(), registry)
                .into_iter()
                .map(|message| format!("{label} {message}")),
        );
        let known_premises: BTreeSet<_> = warrant.premise_ids.iter().map(String::as_str).collect();
        let mut mapped_premises = BTreeSet::new();
        let mut correspondences = BTreeSet::new();
        for premise in &bound.formal_premises {
            if !known_premises.contains(premise.premise_id.as_str()) {
                errors.push(format!(
                    "{label} formal_premises references a premise outside this warrant: {:?}",
                    premise.premise_id
                ));
            }
            mapped_premises.insert(premise.premise_id.as_str());
            if !correspondences.insert((&premise.premise_id, &premise.input_pointer)) {
                errors.push(format!("{label} repeats a formal premise correspondence"));
            }
            if !needs.contains_key(premise.evidence_need_id.as_str()) {
                errors.push(format!(
                    "{label} formal premise {} references an unknown evidence need",
                    premise.premise_id
                ));
            } else if needs[premise.evidence_need_id.as_str()].target_id != premise.premise_id {
                errors.push(format!(
                    "{label} formal premise {} evidence need must target that exact premise",
                    premise.premise_id
                ));
            }
            nonempty(
                errors,
                &format!("{label} premise interpretation"),
                &premise.interpretation,
            );
            if !valid_pointer(&premise.input_pointer, &bound.method) {
                errors.push(format!(
                    "{label} formal premise {} input_pointer is not a declared method input path",
                    premise.premise_id
                ));
            }
            match &premise.correspondence {
                FormalInputCorrespondence::AuthoredDefinition => {
                    if bound.method.kind() != Some(ReasoningMethodKind::FiniteProbability)
                        || bound.method.query.pointer(&premise.input_pointer).is_none()
                    {
                        errors.push(format!("{label} authored_definition must identify an explicit finite_probability/1 query definition"));
                    }
                }
                FormalInputCorrespondence::IdentityValue {
                    source_input_index,
                    source_pointer,
                } => {
                    if *source_input_index >= 4096 || !json_pointer(source_pointer, true) {
                        errors.push(format!("{label} formal premise source correspondence needs a bounded input index and valid JSON pointer"));
                    }
                }
                FormalInputCorrespondence::AcceptedPremise => {
                    let index = premise
                        .input_pointer
                        .strip_prefix("/premises/")
                        .and_then(|index| index.parse::<usize>().ok());
                    if bound.method.kind() != Some(ReasoningMethodKind::Deductive)
                        || index.is_none()
                        || premise.input_pointer[1..].split('/').count() != 2
                    {
                        errors.push(format!("{label} accepted_premise correspondence is limited to a deductive premise formula"));
                    }
                }
            }
        }
        if mapped_premises != known_premises {
            errors.push(format!(
                "{label} formal_premises must cover every warrant premise"
            ));
        }
        let mapped_inputs: BTreeSet<_> = bound
            .formal_premises
            .iter()
            .map(|premise| premise.input_pointer.as_str())
            .collect();
        let required_inputs = required_inputs(&bound.method);
        if required_inputs
            .iter()
            .any(|pointer| !mapped_inputs.contains(pointer.as_str()))
        {
            errors.push(format!("{label} formal correspondences must cover every formal premise and every measured or supplied-model input field"));
        }
        if bound.method.kind() == Some(ReasoningMethodKind::Deductive)
            && mapped_inputs
                .iter()
                .any(|pointer| !required_inputs.iter().any(|required| required == pointer))
        {
            errors.push(format!(
                "{label} deductive correspondence must name complete /premises/N formulae"
            ));
        }
        let mut requirement_roles = BTreeSet::new();
        let mut requirement_ids = BTreeSet::new();
        for requirement in &bound.evidence_requirements {
            if !needs.contains_key(requirement.evidence_need_id.as_str()) {
                errors.push(format!(
                    "{label} evidence requirement references an unknown need"
                ));
            } else {
                let target = &needs[requirement.evidence_need_id.as_str()].target_id;
                let appropriate_target = match requirement.role {
                    MethodEvidenceRole::PremiseObservation => warrant.premise_ids.contains(target),
                    _ => bound.assumption_ids.contains(target),
                };
                if !appropriate_target {
                    errors.push(format!("{label} evidence role {:?} must target the corresponding premise or named model/design assumption", requirement.role));
                }
            }
            if !requirement_roles.insert((
                requirement.evidence_need_id.as_str(),
                format!("{:?}", requirement.role),
            )) {
                errors.push(format!("{label} repeats an evidence requirement role"));
            }
            requirement_ids.insert(requirement.evidence_need_id.as_str());
            nonempty(
                errors,
                &format!("{label} evidence requirement"),
                &requirement.requirement,
            );
        }
        if let Some(kind) = bound.method.kind()
            && !bound
                .evidence_requirements
                .iter()
                .any(|requirement| requirement.role == kind.required_evidence_role())
        {
            errors.push(format!(
                "{label} needs an explicit {:?} evidence requirement",
                kind.required_evidence_role()
            ));
        }
        if bound.formal_premises.iter().any(|premise| {
            !requirement_ids.contains(premise.evidence_need_id.as_str())
                || !bound.evidence_requirements.iter().any(|requirement| {
                    requirement.evidence_need_id == premise.evidence_need_id
                        && requirement.role == MethodEvidenceRole::PremiseObservation
                })
        }) {
            errors.push(format!(
                "{label} every formal premise source needs an evidence requirement"
            ));
        }
        let mut assumptions = BTreeSet::new();
        for id in &bound.assumption_ids {
            if !assumptions.insert(id) {
                errors.push(format!("{label} repeats assumption {id:?}"));
            }
            match case
                .assumptions
                .iter()
                .find(|assumption| assumption.id == *id)
            {
                Some(assumption) if assumption.applies_to.contains(&warrant.id) => {
                    if !bound.evidence_requirements.iter().any(|requirement| {
                        needs
                            .get(requirement.evidence_need_id.as_str())
                            .is_some_and(|need| need.target_id == *id)
                    }) {
                        errors.push(format!(
                            "{label} assumption {id:?} needs a named evidence requirement"
                        ));
                    }
                }
                _ => errors.push(format!(
                    "{label} assumption {id:?} must exist and apply to this warrant"
                )),
            }
        }
        if !matches!(
            bound.method.kind(),
            Some(ReasoningMethodKind::Deductive | ReasoningMethodKind::FiniteProbability)
        ) && bound.assumption_ids.is_empty()
        {
            errors.push(format!(
                "{label} statistical or causal model applicability needs named assumptions"
            ));
        }
    }
}

fn unique_needs(
    label: &str,
    ids: &[String],
    needs: &BTreeMap<&str, &EvidenceNeed>,
    errors: &mut Vec<String>,
) {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) || !needs.contains_key(id.as_str()) {
            errors.push(format!(
                "{label} repeats or references an unknown evidence need {id:?}"
            ));
        }
    }
}

/// Required empirical fields come from EAL's input contract. Authored query
/// fields are already program data; a deductive premise additionally needs an
/// explicit licence from its evidence obligation.
fn required_inputs(method: &MethodSpecification) -> Vec<String> {
    if method.kind() == Some(ReasoningMethodKind::Deductive) {
        return method.query["premises"]
            .as_array()
            .map(|premises| {
                (0..premises.len())
                    .map(|index| format!("/premises/{index}"))
                    .collect()
            })
            .unwrap_or_default();
    }
    let Some(contract) = method.contract() else {
        return vec![];
    };
    contract.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|field| !contract.query_fields.iter().any(|query| query == field))
        .map(|field| format!("/{}", field.replace('~', "~0").replace('/', "~1")))
        .collect()
}

fn valid_pointer(pointer: &str, method: &MethodSpecification) -> bool {
    let Some(tokens) = pointer_tokens(pointer, false) else {
        return false;
    };
    if method.query.pointer(pointer).is_some() {
        return true;
    }
    let Some(contract) = method.contract() else {
        return false;
    };
    let mut schema = &contract.input_schema;
    for token in tokens {
        match schema["type"].as_str() {
            Some("object") => {
                if let Some(child) = schema["properties"].get(&token) {
                    schema = child;
                } else if schema["additionalProperties"].is_object() {
                    schema = &schema["additionalProperties"];
                } else {
                    return false;
                }
            }
            Some("array") if token.parse::<usize>().is_ok() => schema = &schema["items"],
            // The EAL contract deliberately accepts a mathematical JSON value;
            // its method validator supplies the semantic constraints.
            Some("json") => return true,
            _ => return false,
        }
    }
    true
}

fn pointer_tokens(pointer: &str, root_allowed: bool) -> Option<Vec<String>> {
    if pointer.is_empty() {
        return root_allowed.then(Vec::new);
    }
    if !pointer.starts_with('/') || pointer.len() > 1024 {
        return None;
    }
    pointer[1..]
        .split('/')
        .map(|token| {
            let mut chars = token.chars();
            let mut decoded = String::new();
            while let Some(character) = chars.next() {
                if character == '~' {
                    decoded.push(match chars.next() {
                        Some('0') => '~',
                        Some('1') => '/',
                        _ => return None,
                    });
                } else {
                    decoded.push(character);
                }
            }
            Some(decoded)
        })
        .collect()
}

fn json_pointer(pointer: &str, root_allowed: bool) -> bool {
    pointer_tokens(pointer, root_allowed).is_some()
}
