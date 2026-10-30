//! Identifier/reference admission and support-composition integrity.

use super::*;

fn evidence_references(
    errors: &mut Vec<String>,
    label: &str,
    references: &[String],
    known: &HashMap<EvidenceId<'_>, &Evidence>,
) {
    let mut seen = HashSet::new();
    for id in references {
        if !known.contains_key(id.as_str()) {
            errors.push(format!("{label} references unknown evidence {id:?}"));
        }
        if !seen.insert(id) {
            errors.push(format!("{label} repeats evidence {id:?}"));
        }
    }
}

pub(super) fn references(
    request: &Request,
    case: &ArgumentCase,
    context: &Context<'_>,
    errors: &mut Vec<String>,
) {
    let mut ids = HashSet::from(["current_claim".to_owned(), "stronger_target".to_owned()]);
    for evidence in &request.evidence {
        ids.insert(evidence.id.clone());
    }
    for id in case
        .subclaims
        .iter()
        .map(|s| &s.id)
        .chain(case.grounds.iter().map(|g| &g.id))
        .chain(case.warrants.iter().map(|w| &w.id))
        .chain(case.assumptions.iter().map(|a| &a.id))
        .chain(case.defeaters.iter().map(|d| &d.id))
        .chain(case.evidence_plan.iter().map(|n| &n.id))
    {
        register_id(errors, &mut ids, id);
    }
    // Dependent diagnostic labels assume canonical IDs have already admitted
    // their bounded spelling. An invalid long ID must not be copied per edge.
    if !errors.is_empty() {
        return;
    }
    for (section_index, section) in case.reading.iter().enumerate() {
        if !context.claim_ids.contains(section.claim_id.as_str()) {
            errors.push(format!(
                "reading section {} references unknown claim {:?}",
                section_index + 1,
                section.claim_id
            ));
            // This section has no admitted claim label for dependent checks.
            continue;
        }
        for (paragraph_index, paragraph) in section.paragraphs.iter().enumerate() {
            let mut references = HashSet::new();
            for id in &paragraph.reference_ids {
                if !context.ids.contains(id) {
                    errors.push(format!(
                        "reading {} paragraph {} references unknown canonical ID {id:?}",
                        section.claim_id,
                        paragraph_index + 1
                    ));
                }
                if !references.insert(id) {
                    errors.push(format!(
                        "reading {} paragraph {} repeats canonical ID {id:?}",
                        section.claim_id,
                        paragraph_index + 1
                    ));
                }
            }
        }
    }
    for need in &case.evidence_plan {
        if !context.node_ids.contains(need.target_id.as_str()) {
            errors.push(format!(
                "evidence plan {} has unknown target {:?}",
                need.id, need.target_id
            ));
        }
        if !context.claim_ids.contains(need.enables_claim.as_str()) {
            errors.push(format!(
                "evidence plan {} has unknown enables_claim {:?}",
                need.id, need.enables_claim
            ));
        }
    }
    for warrant in &case.warrants {
        if !context.claim_ids.contains(warrant.conclusion_id.as_str()) {
            errors.push(format!(
                "warrant {} has unknown conclusion {:?}",
                warrant.id, warrant.conclusion_id
            ));
        }
        if warrant.premise_ids.is_empty()
            && !warrant
                .reasoning_method
                .as_ref()
                .and_then(|method| method.bound())
                .is_some_and(|bound| bound.is_mathematical())
        {
            errors.push(format!(
                "warrant {} needs at least one ground or subclaim",
                warrant.id
            ));
        }
        let mut seen = HashSet::new();
        for premise in &warrant.premise_ids {
            if !context.premise_ids.contains(premise.as_str()) {
                errors.push(format!(
                    "warrant {} references unknown premise {premise:?}",
                    warrant.id
                ));
            }
            if !seen.insert(premise) {
                errors.push(format!(
                    "warrant {} repeats premise {premise:?}",
                    warrant.id
                ));
            }
        }
    }
    for assumption in &case.assumptions {
        if assumption.applies_to.is_empty() {
            errors.push(format!(
                "assumption {} must identify the nodes it applies to",
                assumption.id
            ));
        }
        let mut seen = HashSet::new();
        for target in &assumption.applies_to {
            if !context.node_ids.contains(target.as_str()) || *target == assumption.id {
                errors.push(format!(
                    "assumption {} has unknown or self target {target:?}",
                    assumption.id
                ));
            }
            if !seen.insert(target) {
                errors.push(format!(
                    "assumption {} repeats target {target:?}",
                    assumption.id
                ));
            }
        }
    }
    for defeater in &case.defeaters {
        if !context.node_ids.contains(defeater.target_id.as_str())
            || defeater.target_id == defeater.id
        {
            errors.push(format!(
                "defeater {} has unknown or self target {:?}",
                defeater.id, defeater.target_id
            ));
        }
    }
    for ground in &case.grounds {
        evidence_references(
            errors,
            &ground.id,
            &ground.evidence_ids,
            &context.known_evidence,
        );
    }
    for warrant in &case.warrants {
        evidence_references(
            errors,
            &warrant.id,
            &warrant.backing_evidence_ids,
            &context.known_evidence,
        );
    }
    for assumption in &case.assumptions {
        evidence_references(
            errors,
            &assumption.id,
            &assumption.evidence_ids,
            &context.known_evidence,
        );
    }
    for defeater in &case.defeaters {
        evidence_references(
            errors,
            &defeater.id,
            &defeater.evidence_ids,
            &context.known_evidence,
        );
    }
    for alternative in &case.alternatives {
        evidence_references(
            errors,
            "alternative",
            &alternative.evidence_ids,
            &context.known_evidence,
        );
    }
}

pub(super) fn routes(
    case: &ArgumentCase,
    context: &Context<'_>,
    index: &ArgumentIndex<'_>,
    errors: &mut Vec<String>,
) {
    let mut seen = HashSet::new();
    for composition in &case.compositions {
        let conclusion = composition.conclusion_id.as_str();
        if !context.claim_ids.contains(conclusion) {
            errors.push(format!("composition has unknown conclusion {conclusion:?}"));
        }
        if !seen.insert(conclusion) {
            errors.push(format!("duplicate composition for claim {conclusion:?}"));
        }
        nonempty(
            errors,
            &format!("composition {conclusion} rationale"),
            &composition.rationale,
        );
        let expected: HashSet<_> = index
            .warrants_for(conclusion)
            .iter()
            .map(|w| w.id.as_str())
            .collect();
        if expected.is_empty() {
            errors.push(format!(
                "composition {conclusion:?} has no attached warrants"
            ));
            continue;
        }
        let mut selected = HashSet::new();
        for warrant_id in &composition.selected_warrant_ids {
            if !selected.insert(warrant_id.as_str()) {
                errors.push(format!(
                    "composition {conclusion:?} repeats selected warrant {warrant_id:?}"
                ));
            }
            if !expected.contains(warrant_id.as_str()) {
                errors.push(format!("composition {conclusion:?} selects an unknown or wrong-claim warrant {warrant_id:?}"));
            }
        }
        match composition.relationship {
            SupportRelationship::JointlyRequired | SupportRelationship::Cumulative => {
                if selected != expected {
                    errors.push(format!("composition {conclusion:?} must select all attached warrants for jointly_required or cumulative support"));
                }
            }
            SupportRelationship::AlternativeRoutes => {
                if composition.selected_warrant_ids.len() > 1 {
                    errors.push(format!("alternative_routes composition {conclusion:?} must select zero or exactly one warrant"));
                }
            }
        }
    }
    for conclusion in &context.claim_ids {
        if !index.warrants_for(conclusion).is_empty() && !seen.contains(conclusion) {
            errors.push(format!(
                "claim {conclusion:?} needs exactly one support composition"
            ));
        }
    }
    for subclaim in &case.subclaims {
        if index.warrants_for(&subclaim.id).is_empty() {
            errors.push(format!(
                "subclaim {} needs an attached warrant",
                subclaim.id
            ));
        }
    }
    let mut all_attached = index.candidate_dependency("current_claim");
    all_attached.extend(index.candidate_dependency("stronger_target"));
    for node in &context.node_ids {
        if !all_attached.contains(*node) {
            errors.push(format!(
                "unattached argument node {node:?}: connect it to current_claim or stronger_target"
            ));
        }
    }
}
