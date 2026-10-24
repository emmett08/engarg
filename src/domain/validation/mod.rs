//! Staged admission keeps references, narrative and evidential judgements distinct.

mod evidence;
mod narrative;
mod status;
mod structure;

use super::diagnostics::record;
use super::identifiers::valid_id;
use super::*;
use std::collections::{HashMap, HashSet};

pub(super) fn nonempty(errors: &mut Vec<String>, label: &str, text: &str) {
    if text.trim().is_empty() {
        errors.push(format!("{label} must contain text"));
    }
}

fn register_id(errors: &mut Vec<String>, ids: &mut HashSet<String>, id: &str) {
    if !valid_id(id) {
        errors.push(format!(
            "invalid identifier {id:?}: use 1–64 ASCII letters, digits, '_' or '-'"
        ));
    }
    if !ids.insert(id.to_owned()) {
        errors.push(format!("duplicate or reserved identifier {id:?}"));
    }
}

fn finish(mut errors: Vec<String>) -> Result<(), Vec<String>> {
    errors.sort_unstable();
    errors.dedup();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_request(request: &Request) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    nonempty(&mut errors, "claim", &request.claim);
    let mut ids = HashSet::from(["current_claim".to_owned(), "stronger_target".to_owned()]);
    for evidence in &request.evidence {
        register_id(&mut errors, &mut ids, &evidence.id);
        for (name, value) in [
            ("title", &evidence.title),
            ("source", &evidence.source),
            ("excerpt", &evidence.excerpt),
            ("kind", &evidence.kind),
        ] {
            nonempty(
                &mut errors,
                &format!("evidence {} {name}", evidence.id),
                value,
            );
        }
    }
    finish(errors)
}

pub(super) struct Context<'a> {
    known_evidence: HashMap<EvidenceId<'a>, &'a Evidence>,
    ids: HashSet<String>,
    claim_ids: HashSet<&'a str>,
    premise_ids: HashSet<&'a str>,
    node_ids: HashSet<&'a str>,
    planned_targets: HashSet<&'a str>,
}

impl<'a> Context<'a> {
    fn new(request: &'a Request, case: &'a ArgumentCase) -> Self {
        let known_evidence = request
            .evidence
            .iter()
            .map(|e| (EvidenceId::reference(&e.id), e))
            .collect();
        let claim_ids: HashSet<_> = ["current_claim", "stronger_target"]
            .into_iter()
            .chain(case.subclaims.iter().map(|s| s.id.as_str()))
            .collect();
        let premise_ids = case
            .grounds
            .iter()
            .map(|g| g.id.as_str())
            .chain(case.subclaims.iter().map(|s| s.id.as_str()))
            .collect();
        let node_ids: HashSet<_> = claim_ids
            .iter()
            .copied()
            .chain(case.grounds.iter().map(|g| g.id.as_str()))
            .chain(case.warrants.iter().map(|w| w.id.as_str()))
            .chain(case.assumptions.iter().map(|a| a.id.as_str()))
            .chain(case.defeaters.iter().map(|d| d.id.as_str()))
            .collect();
        let ids = node_ids
            .iter()
            .copied()
            .chain(request.evidence.iter().map(|e| e.id.as_str()))
            .chain(case.evidence_plan.iter().map(|n| n.id.as_str()))
            .map(str::to_owned)
            .collect();
        let planned_targets = case
            .evidence_plan
            .iter()
            .map(|n| n.target_id.as_str())
            .collect();
        Self {
            known_evidence,
            ids,
            claim_ids,
            premise_ids,
            node_ids,
            planned_targets,
        }
    }
}

/// Admission returns a source borrow and its one graph index. Mutation of the
/// source cannot occur while this view is subsequently used.
pub fn validate_argument<'a>(
    request: &'a Request,
    case: &'a ArgumentCase,
) -> Result<ValidatedArgument<'a>, Vec<DomainDiagnostic>> {
    let mut diagnostics = record(
        DiagnosticCode::InvalidRequest,
        "request",
        validate_request(request).err().unwrap_or_default(),
    );
    if case.original_claim != request.claim {
        diagnostics.extend(record(
            DiagnosticCode::InvalidRequest,
            "argument.original_claim",
            vec!["original_claim must exactly preserve the requested claim".into()],
        ));
    }
    if !diagnostics.is_empty() {
        return Err(ordered(diagnostics));
    }
    let context = Context::new(request, case);
    let mut structural = Vec::new();
    structure::references(request, case, &context, &mut structural);
    diagnostics.extend(record(
        DiagnosticCode::ReferenceIntegrity,
        "argument",
        structural,
    ));
    if !diagnostics.is_empty() {
        return Err(ordered(diagnostics));
    }
    let index = ArgumentIndex::new(case);
    if index.has_support_cycle() {
        return Err(record(
            DiagnosticCode::SupportCycle,
            "argument.support",
            vec!["argument support graph contains a cycle".into()],
        ));
    }
    let mut structural = Vec::new();
    structure::routes(case, &context, &index, &mut structural);
    diagnostics.extend(record(
        DiagnosticCode::InvalidStructure,
        "argument.support",
        structural,
    ));
    // References and cycle admission have already established safe indexed
    // traversal. Composition-policy failures still need their status consequences.
    let mut messages = Vec::new();
    narrative::validate(case, &mut messages);
    diagnostics.extend(record(
        DiagnosticCode::NarrativePolicy,
        "argument.reading",
        messages,
    ));
    let mut messages = Vec::new();
    evidence::validate(request, case, &context, &index, &mut messages);
    diagnostics.extend(record(DiagnosticCode::EvidencePolicy, "argument", messages));
    let mut messages = Vec::new();
    status::validate(case, &context, &index, &mut messages);
    diagnostics.extend(record(
        DiagnosticCode::StatusPolicy,
        "argument.status",
        messages,
    ));
    if diagnostics.is_empty() {
        Ok(ValidatedArgument { case, index })
    } else {
        Err(ordered(diagnostics))
    }
}

fn ordered(mut diagnostics: Vec<DomainDiagnostic>) -> Vec<DomainDiagnostic> {
    diagnostics.sort_unstable();
    diagnostics.dedup();
    diagnostics
}

/// Compatible human-readable errors delegate to the staged typed admission.
pub fn validate_case(request: &Request, case: &ArgumentCase) -> Result<(), Vec<String>> {
    validate_argument(request, case)
        .map(|_| ())
        .map_err(|errors| {
            let mut messages: Vec<_> = errors.into_iter().map(|d| d.message).collect();
            messages.sort_unstable();
            messages.dedup();
            messages
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> (Request, ArgumentCase) {
        (
            serde_json::from_str(include_str!("../../../examples/request.json")).unwrap(),
            serde_json::from_str(include_str!("../../../examples/case.json")).unwrap(),
        )
    }

    #[test]
    fn structure_errors_precede_policy_and_have_stable_order_and_locations() {
        let (request, mut case) = example();
        case.warrants[0].premise_ids = vec!["missing".into()];
        case.current_claim.text.clear();
        let first = validate_argument(&request, &case).unwrap_err();
        let second = validate_argument(&request, &case).unwrap_err();
        assert_eq!(first, second);
        assert!(
            first
                .iter()
                .all(|d| d.code == DiagnosticCode::ReferenceIntegrity)
        );
        assert!(first.iter().all(|d| !d.location.is_empty()));
        assert!(first.windows(2).all(|pair| pair[0] <= pair[1]));
        let compatible = validate_case(&request, &case).unwrap_err();
        assert!(
            compatible
                .iter()
                .any(|message| message.contains("unknown premise"))
        );
    }

    #[test]
    fn narrative_evidence_and_status_policies_have_distinct_diagnostic_owners() {
        let (request, mut case) = example();
        case.reading[0].title.clear();
        case.scope.units.clear();
        case.status = CaseStatus::ProvisionallySupported;
        let diagnostics = validate_argument(&request, &case).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::NarrativePolicy)
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::EvidencePolicy)
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::StatusPolicy)
        );
    }
}
