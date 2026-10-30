//! Deterministic beam expansion, assessment reuse and finite-family accounting.

use super::{Assessment, Candidate, CandidateResult, SearchConfig, SearchResult};
use crate::{
    Error,
    domain::{ArgumentCase, ArgumentIndex, Request, SupportRelationship, validate_argument},
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    future::Future,
};

const MAX_CHOICE_POINTS: usize = 64;
// Match the paired EAL defaults while applying an independent admission bound
// before graph validation or construction. Oversized families are rejected,
// never silently truncated into a smaller purported optimum.
const MAX_INPUT_NODES: usize = 4096;
const MAX_INPUT_EDGES: usize = 131_072;

struct Dimension {
    conclusion: String,
    options: Vec<Vec<String>>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn bound_input(request: &Request, case: &ArgumentCase) -> Result<(), Error> {
    let nodes = [
        2,
        case.subclaims.len(),
        case.grounds.len(),
        case.warrants.len(),
        case.assumptions.len(),
        case.defeaters.len(),
        request.evidence.len(),
        case.evidence_plan.len(),
    ]
    .into_iter()
    .fold(0usize, usize::saturating_add);
    if nodes > MAX_INPUT_NODES {
        return Err(Error::Budget(format!(
            "optimal argument input exceeds {MAX_INPUT_NODES} argument/evidence nodes"
        )));
    }
    // Count reference occurrences rather than deduplicating them: duplicate
    // fields also consume validation work and cannot evade this bound.
    let edges = case
        .warrants
        .iter()
        .map(|warrant| {
            1usize
                .saturating_add(warrant.premise_ids.len())
                .saturating_add(warrant.backing_evidence_ids.len())
        })
        .chain(case.grounds.iter().map(|ground| ground.evidence_ids.len()))
        .chain(case.assumptions.iter().map(|assumption| {
            assumption
                .applies_to
                .len()
                .saturating_add(assumption.evidence_ids.len())
        }))
        .chain(
            case.defeaters
                .iter()
                .map(|defeater| 1usize.saturating_add(defeater.evidence_ids.len())),
        )
        .chain(
            case.compositions
                .iter()
                .map(|composition| composition.selected_warrant_ids.len()),
        )
        .chain(
            case.alternatives
                .iter()
                .map(|alternative| alternative.evidence_ids.len()),
        )
        .chain(case.reading.iter().flat_map(|section| {
            section
                .paragraphs
                .iter()
                .map(|paragraph| paragraph.reference_ids.len())
        }))
        .chain(std::iter::once(case.evidence_plan.len().saturating_mul(2)))
        .fold(0usize, usize::saturating_add);
    if edges > MAX_INPUT_EDGES {
        return Err(Error::Budget(format!(
            "optimal argument input exceeds {MAX_INPUT_EDGES} reference occurrences"
        )));
    }
    Ok(())
}

fn dimensions(case: &ArgumentCase) -> Result<Vec<Dimension>, Error> {
    let index = ArgumentIndex::new(case);
    let reachable = index.candidate_dependency("current_claim");
    let mut dimensions = Vec::new();
    for composition in &case.compositions {
        if composition.relationship != SupportRelationship::AlternativeRoutes
            || !reachable.contains(composition.conclusion_id.as_str())
        {
            continue;
        }
        if dimensions.len() == MAX_CHOICE_POINTS {
            return Err(Error::Budget(format!(
                "optimal argument search exceeds {MAX_CHOICE_POINTS} alternative choice points"
            )));
        }
        let mut options: Vec<_> = index
            .warrants_for(&composition.conclusion_id)
            .iter()
            .map(|warrant| vec![warrant.id.clone()])
            .collect();
        // No-selection is an option only when present in the authored baseline.
        if composition.selected_warrant_ids.is_empty() {
            options.push(Vec::new());
        }
        options.sort();
        options.dedup();
        dimensions.push(Dimension {
            conclusion: composition.conclusion_id.clone(),
            options,
        });
    }
    dimensions.sort_by(|left, right| left.conclusion.cmp(&right.conclusion));
    Ok(dimensions)
}

fn candidate(case: &ArgumentCase, selections: BTreeMap<String, Vec<String>>) -> Candidate {
    let mut candidate = Candidate {
        selections,
        dependency_ids: Vec::new(),
        length: 0,
    };
    let selected = candidate.apply(case);
    candidate.dependency_ids = ArgumentIndex::new(&selected)
        .selected_dependency("current_claim")
        .into_iter()
        .map(str::to_owned)
        .collect();
    candidate.dependency_ids.sort();
    candidate.length = candidate.dependency_ids.len();
    candidate
}

fn compare(left: &CandidateResult, right: &CandidateResult) -> Ordering {
    right
        .assessment
        .supported
        .cmp(&left.assessment.supported)
        .then_with(|| left.candidate.length.cmp(&right.candidate.length))
        .then_with(|| left.candidate.selections.cmp(&right.candidate.selections))
}

/// Assess the baseline first, then expand alternative compositions in lexical
/// conclusion/warrant order. Every assessment receives a complete selection;
/// unexpanded choices retain their authored values. Identical selections reuse
/// their original result. Assessment errors propagate without becoming support.
///
/// Supported states precede unsupported states in the beam. Among equally
/// eligible states, unique dependency count precedes lexical choices. The best
/// result is chosen over *all* assessed supported states, including discarded
/// frontier states. Missing support returns `best: None`.
///
/// Width is 1..=64 and at most 1..=256 distinct candidates may be assessed.
/// Inputs are limited to 4096 argument/evidence nodes, 131072 reference
/// occurrences and 64 alternative compositions before assessment starts.
/// Any beam truncation or blocked assessment makes completeness false. This is
/// a bounded recommendation search, not a proof of a globally strongest argument.
pub async fn search<F, Fut>(
    request: &Request,
    case: &ArgumentCase,
    config: SearchConfig,
    mut assess: F,
) -> Result<SearchResult, Error>
where
    F: FnMut(Candidate) -> Fut,
    Fut: Future<Output = Result<Assessment, Error>>,
{
    if !(1..=64).contains(&config.beam_width) || !(1..=256).contains(&config.max_candidates) {
        return Err(invalid(
            "optimal argument search requires beam_width 1..=64 and max_candidates 1..=256",
        ));
    }
    bound_input(request, case)?;
    validate_argument(request, case).map_err(|errors| {
        invalid(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let dimensions = dimensions(case)?;
    let conclusions: BTreeSet<_> = dimensions
        .iter()
        .map(|dimension| dimension.conclusion.as_str())
        .collect();
    let baseline = candidate(
        case,
        case.compositions
            .iter()
            .filter(|composition| conclusions.contains(composition.conclusion_id.as_str()))
            .map(|composition| {
                (
                    composition.conclusion_id.clone(),
                    composition.selected_warrant_ids.clone(),
                )
            })
            .collect(),
    );
    let baseline_assessment = assess(baseline.clone()).await?;
    let mut cache = BTreeMap::from([(baseline.selections.clone(), 0)]);
    let mut candidates = vec![CandidateResult {
        candidate: baseline,
        assessment: baseline_assessment,
    }];
    let mut frontier = vec![0];
    let mut search_complete = true;
    let mut pruned_states = 0;
    'layers: for dimension in dimensions {
        let mut next = BTreeSet::new();
        for parent in frontier {
            for option in &dimension.options {
                let mut selections = candidates[parent].candidate.selections.clone();
                selections.insert(dimension.conclusion.clone(), option.clone());
                let position = if let Some(position) = cache.get(&selections) {
                    *position
                } else {
                    if candidates.len() == config.max_candidates {
                        search_complete = false;
                        break 'layers;
                    }
                    let candidate = candidate(case, selections);
                    let assessment = assess(candidate.clone()).await?;
                    let position = candidates.len();
                    cache.insert(candidate.selections.clone(), position);
                    candidates.push(CandidateResult {
                        candidate,
                        assessment,
                    });
                    position
                };
                next.insert(position);
            }
        }
        frontier = next.into_iter().collect();
        frontier.sort_by(|left, right| compare(&candidates[*left], &candidates[*right]));
        if frontier.len() > config.beam_width {
            search_complete = false;
            pruned_states += frontier.len() - config.beam_width;
            frontier.truncate(config.beam_width);
        }
    }
    let best = candidates
        .iter()
        .enumerate()
        .filter(|(_, result)| result.assessment.supported)
        .min_by(|(_, left), (_, right)| compare(left, right))
        .map(|(index, _)| index);
    Ok(SearchResult {
        best,
        candidates,
        search_complete,
        pruned_states,
        config,
    })
}

#[cfg(test)]
mod tests;
