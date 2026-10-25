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
        if binding.kind().inference() != warrant.inference {
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
        let Some(input_need) = needs.get(bound.input_evidence_need_id.as_str()) else {
            errors.push(format!("{label} input_evidence_need_id is unknown"));
            continue;
        };
        if input_need.target_id != warrant.id {
            errors.push(format!(
                "{label} input_evidence_need_id must target this warrant"
            ));
        }
        let method = bound.method.identifier();
        if bound.formal_premises.is_empty()
            || bound.formal_premises.len() > 128
            || bound.evidence_requirements.len() < 2
            || bound.evidence_requirements.len() > 128
            || bound.assumption_ids.len() > 128
        {
            errors.push(format!(
                "{label} exceeds bounded premise, evidence or assumption lists"
            ));
        }
        let proposition = bound.conclusion.proposition(&bound.method);
        match serde_json::from_value::<eal_core::semantic::Proposition>(proposition) {
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
        validate_model(&label, &bound.method, errors);
        let known_premises: BTreeSet<_> = warrant.premise_ids.iter().map(String::as_str).collect();
        let mut mapped_premises = BTreeSet::new();
        let mut correspondences = BTreeSet::new();
        for premise in &bound.formal_premises {
            if premise.evidence_need_id == bound.input_evidence_need_id {
                errors.push(format!(
                    "{label} formal input admission cannot also establish premise evidence"
                ));
            }
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
                    if bound.method.kind() != ReasoningMethodKind::Deductive
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
        let required_inputs: Vec<String> = match &bound.method {
            MethodSpecification::Deductive { premises, .. } => (0..premises.len())
                .map(|index| format!("/premises/{index}"))
                .collect(),
            MethodSpecification::Inductive { .. } => {
                ["/successes", "/trials"].map(str::to_owned).into()
            }
            MethodSpecification::Abductive { .. } => {
                ["/observed", "/candidates"].map(str::to_owned).into()
            }
            MethodSpecification::Causal { .. } => ["/assignment", "/treatment", "/control"]
                .map(str::to_owned)
                .into(),
            MethodSpecification::Counterfactual { .. } => {
                ["/variables", "/intervention", "/outcome"]
                    .map(str::to_owned)
                    .into()
            }
        };
        if required_inputs
            .iter()
            .any(|pointer| !mapped_inputs.contains(pointer.as_str()))
        {
            errors.push(format!("{label} formal correspondences must cover every formal premise and every measured or supplied-model input field"));
        }
        if bound.method.kind() == ReasoningMethodKind::Deductive
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
            if (requirement.evidence_need_id == bound.input_evidence_need_id)
                != (requirement.role == MethodEvidenceRole::FormalInput)
            {
                errors.push(format!("{label} formal_input admission must be separate from empirical, design and model evidence"));
            }
            if !needs.contains_key(requirement.evidence_need_id.as_str()) {
                errors.push(format!(
                    "{label} evidence requirement references an unknown need"
                ));
            } else {
                let target = &needs[requirement.evidence_need_id.as_str()].target_id;
                let appropriate_target = match requirement.role {
                    MethodEvidenceRole::FormalInput => target == &warrant.id,
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
        if !bound.evidence_requirements.iter().any(|requirement| {
            requirement.evidence_need_id == bound.input_evidence_need_id
                && requirement.role == MethodEvidenceRole::FormalInput
        }) {
            errors.push(format!(
                "{label} input needs an explicit formal_input evidence requirement"
            ));
        }
        if !bound
            .evidence_requirements
            .iter()
            .any(|requirement| requirement.role == bound.method.kind().required_evidence_role())
        {
            errors.push(format!(
                "{label} needs an explicit {:?} evidence requirement",
                bound.method.kind().required_evidence_role()
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
        if bound.method.kind() != ReasoningMethodKind::Deductive && bound.assumption_ids.is_empty()
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

fn valid_pointer(pointer: &str, method: &MethodSpecification) -> bool {
    if !json_pointer(pointer, false) {
        return false;
    }
    let mut tokens = Vec::new();
    for token in pointer[1..].split('/') {
        let mut chars = token.chars();
        let mut decoded = String::new();
        while let Some(character) = chars.next() {
            if character == '~' {
                match chars.next() {
                    Some('0') => decoded.push('~'),
                    Some('1') => decoded.push('/'),
                    _ => return false,
                }
            } else {
                decoded.push(character);
            }
        }
        tokens.push(decoded);
    }
    let Some(first) = tokens.first().map(String::as_str) else {
        return false;
    };
    match method {
        MethodSpecification::Deductive { .. }
        | MethodSpecification::Abductive { .. }
        | MethodSpecification::Counterfactual { .. } => method.query().pointer(pointer).is_some(),
        MethodSpecification::Inductive { .. } => {
            tokens.len() == 1 && ["successes", "trials", "confidence"].contains(&first)
        }
        MethodSpecification::Causal { .. } => {
            ["assignment", "treatment", "control"].contains(&first)
                && (tokens.len() == 1 || tokens.len() == 2 && tokens[1].parse::<usize>().is_ok())
        }
    }
}

fn json_pointer(pointer: &str, root_allowed: bool) -> bool {
    if pointer.is_empty() {
        return root_allowed;
    }
    if !pointer.starts_with('/') || pointer.len() > 1024 {
        return false;
    }
    let mut chars = pointer.chars();
    while let Some(character) = chars.next() {
        if character == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

fn validate_model(label: &str, method: &MethodSpecification, errors: &mut Vec<String>) {
    match method {
        MethodSpecification::Deductive {
            premises,
            conclusion,
        } => {
            let mut nodes = 0usize;
            let mut atoms = BTreeSet::new();
            let mut valid = premises.len() <= 128;
            for formula in premises.iter().chain(std::iter::once(conclusion)) {
                valid &= formula_valid(formula, 0, &mut nodes, &mut atoms);
            }
            if !valid || atoms.len() > 12 {
                errors.push(format!("{label} deduction requires well-formed formulae within 12 atoms, 512 nodes and depth 32"));
            }
        }
        MethodSpecification::Inductive { confidence } => {
            if !confidence.is_finite() || !(0.001..=0.999999).contains(confidence) {
                errors.push(format!("{label} confidence must be in [0.001, 0.999999]"));
            }
        }
        MethodSpecification::Abductive { candidates, .. } => {
            let mut names = BTreeSet::new();
            if candidates.iter().any(|candidate| {
                !names.insert(&candidate.name)
                    || !method_identifier(&candidate.name)
                    || !candidate.prior.is_finite()
                    || !candidate.likelihood.is_finite()
                    || !(0.0..=1.0).contains(&candidate.prior)
                    || !(0.0..=1.0).contains(&candidate.likelihood)
            }) || (candidates
                .iter()
                .map(|candidate| candidate.prior)
                .sum::<f64>()
                - 1.0)
                .abs()
                > 1e-9
            {
                errors.push(format!("{label} hypotheses need distinct names, probabilities in [0,1] and priors summing to one"));
            }
        }
        MethodSpecification::Causal { .. } => {}
        MethodSpecification::Counterfactual {
            variables,
            intervention,
            outcome,
        } => {
            let names: BTreeSet<_> = variables.iter().map(|variable| &variable.name).collect();
            if variables.is_empty()
                || variables.len() > 32
                || names.len() != variables.len()
                || !names.contains(outcome)
                || !names.contains(&intervention.variable)
                || !intervention.value.is_finite()
                || variables.iter().any(|equation| {
                    let coefficient_names: BTreeSet<_> = equation
                        .coefficients
                        .iter()
                        .map(|coefficient| &coefficient.variable)
                        .collect();
                    !method_identifier(&equation.name)
                        || !equation.intercept.is_finite()
                        || !equation.noise.is_finite()
                        || coefficient_names.len() != equation.coefficients.len()
                        || equation.coefficients.iter().any(|coefficient| {
                            !names.contains(&coefficient.variable)
                                || !coefficient.coefficient.is_finite()
                        })
                })
            {
                errors.push(format!("{label} structural model requires 1–32 finite, resolved variables and intervention/outcome names"));
            }
            let mut remaining = names;
            while !remaining.is_empty() {
                let ready: Vec<_> = remaining
                    .iter()
                    .copied()
                    .filter(|variable| {
                        variables
                            .iter()
                            .find(|equation| equation.name.as_str() == variable.as_str())
                            .is_some_and(|equation| {
                                equation
                                    .coefficients
                                    .iter()
                                    .all(|parent| !remaining.contains(&parent.variable))
                            })
                    })
                    .collect();
                if ready.is_empty() {
                    errors.push(format!("{label} structural model must be acyclic"));
                    break;
                }
                for variable in ready {
                    remaining.remove(variable);
                }
            }
        }
    }
}

fn method_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, character)| {
            character.is_ascii_alphabetic()
                || character == b'_'
                || index > 0 && character.is_ascii_digit()
        })
}

fn formula_valid<'a>(
    formula: &'a LogicalFormula,
    depth: usize,
    nodes: &mut usize,
    atoms: &mut BTreeSet<&'a str>,
) -> bool {
    *nodes += 1;
    if depth > 32 || *nodes > 512 {
        return false;
    }
    match formula {
        LogicalFormula::Atom(atom) => {
            atoms.insert(atom);
            method_identifier(atom)
        }
        LogicalFormula::Not { not } => formula_valid(not, depth + 1, nodes, atoms),
        LogicalFormula::And { and: operands }
        | LogicalFormula::Or { or: operands }
        | LogicalFormula::Implies { implies: operands } => {
            let count = if matches!(formula, LogicalFormula::Implies { .. }) {
                operands.len() == 2
            } else {
                (2..=128).contains(&operands.len())
            };
            count
                && operands
                    .iter()
                    .all(|operand| formula_valid(operand, depth + 1, nodes, atoms))
        }
    }
}
