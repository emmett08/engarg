//! Search contracts checked against independently specified small route families.

use super::*;
use crate::domain::{ClaimNode, NarrativeSection, SupportComposition};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

fn example() -> (Request, ArgumentCase) {
    let request =
        serde_json::from_str(include_str!("../../../examples/claim-only-request.json")).unwrap();
    let case =
        serde_json::from_str(include_str!("../../../examples/claim-only-case.json")).unwrap();
    (request, case)
}

fn alternative_case() -> (Request, ArgumentCase) {
    let (request, mut case) = example();
    let mut short = case
        .warrants
        .iter()
        .find(|warrant| warrant.id == "W2")
        .unwrap()
        .clone();
    short.id = "A_short".into();
    short.premise_ids = vec!["G2".into()];
    case.warrants.push(short);
    let root = case
        .compositions
        .iter_mut()
        .find(|composition| composition.conclusion_id == "current_claim")
        .unwrap();
    root.relationship = SupportRelationship::AlternativeRoutes;
    (request, case)
}

fn assessment(supported: bool) -> Assessment {
    Assessment {
        supported,
        status: if supported {
            "supported"
        } else {
            "unsupported"
        }
        .into(),
        artifact: "independently-assessed-test-fixture".into(),
        reasons: Vec::new(),
    }
}

fn exhaustive() -> SearchConfig {
    SearchConfig {
        beam_width: 64,
        max_candidates: 256,
    }
}

#[tokio::test]
async fn shorter_unsupported_route_cannot_replace_supported_baseline() {
    let (request, case) = alternative_case();
    let result = search(&request, &case, exhaustive(), |candidate| async move {
        Ok(assessment(candidate.selections["current_claim"] == ["W2"]))
    })
    .await
    .unwrap();
    assert!(result.search_complete);
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(result.best, Some(0));
    assert!(result.candidates[1].candidate.length < result.candidates[0].candidate.length);
    assert_eq!(
        result.candidates[0].candidate.selections["current_claim"],
        ["W2"]
    );
}

#[tokio::test]
async fn smallest_supported_route_preserves_complete_authored_case() {
    let (request, case) = alternative_case();
    let before = serde_json::to_value(&case).unwrap();
    let result = search(&request, &case, exhaustive(), |_| async {
        Ok(assessment(true))
    })
    .await
    .unwrap();
    let best = &result.candidates[result.best.unwrap()].candidate;
    assert_eq!(best.selections["current_claim"], ["A_short"]);
    assert_eq!(best.length, best.dependency_ids.len());
    assert!(best.dependency_ids.windows(2).all(|pair| pair[0] < pair[1]));
    let mut applied = serde_json::to_value(best.apply(&case)).unwrap();
    let position = case
        .compositions
        .iter()
        .position(|composition| composition.conclusion_id == "current_claim")
        .unwrap();
    applied["compositions"][position]["selected_warrant_ids"] = json!(["W2"]);
    assert_eq!(applied, before);
    assert_eq!(serde_json::to_value(&case).unwrap(), before);
}

#[tokio::test]
async fn no_supported_candidate_has_no_winner() {
    let (request, case) = alternative_case();
    let result = search(&request, &case, exhaustive(), |_| async {
        Ok(assessment(false))
    })
    .await
    .unwrap();
    assert_eq!(result.best, None);
    assert_eq!(result.candidates.len(), 2);
}

#[tokio::test]
async fn original_empty_selection_is_an_option_and_is_assessed_first() {
    let (request, mut case) = alternative_case();
    case.compositions
        .iter_mut()
        .find(|composition| composition.conclusion_id == "current_claim")
        .unwrap()
        .selected_warrant_ids
        .clear();
    let result = search(&request, &case, exhaustive(), |candidate| async move {
        Ok(assessment(
            !candidate.selections["current_claim"].is_empty(),
        ))
    })
    .await
    .unwrap();
    assert!(result.candidates[0].candidate.selections["current_claim"].is_empty());
    assert_eq!(result.candidates.len(), 3);
    assert!(result.candidates[result.best.unwrap()].assessment.supported);
    assert!(result.search_complete);
}

fn two_dimensions() -> (Request, ArgumentCase) {
    let (request, mut case) = alternative_case();
    let mut nested = case
        .warrants
        .iter()
        .find(|warrant| warrant.id == "W1")
        .unwrap()
        .clone();
    nested.id = "W1b".into();
    case.warrants.push(nested);
    case.compositions
        .iter_mut()
        .find(|composition| composition.conclusion_id == "S1")
        .unwrap()
        .relationship = SupportRelationship::AlternativeRoutes;
    (request, case)
}

#[tokio::test]
async fn complete_finite_family_is_cached_and_matches_four_combinations() {
    let (request, case) = two_dimensions();
    let calls = Rc::new(Cell::new(0));
    let result = search(&request, &case, exhaustive(), {
        let calls = calls.clone();
        move |_| {
            calls.set(calls.get() + 1);
            async { Ok(assessment(true)) }
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.get(), 4);
    assert_eq!(result.candidates.len(), 4);
    assert!(result.search_complete);
    let selections: BTreeSet<_> = result
        .candidates
        .iter()
        .map(|result| result.candidate.selections.clone())
        .collect();
    assert_eq!(selections.len(), 4);
    assert!(
        selections
            .iter()
            .any(|selection| selection["S1"] == ["W1b"] && selection["current_claim"] == ["W2"])
    );
}

#[tokio::test]
async fn beam_pruning_and_candidate_budget_never_claim_completeness() {
    let (request, case) = two_dimensions();
    let pruned = search(
        &request,
        &case,
        SearchConfig {
            beam_width: 1,
            max_candidates: 256,
        },
        |_| async { Ok(assessment(true)) },
    )
    .await
    .unwrap();
    assert!(!pruned.search_complete);
    assert!(pruned.pruned_states > 0);
    let bounded = search(
        &request,
        &case,
        SearchConfig {
            beam_width: 64,
            max_candidates: 1,
        },
        |_| async { Ok(assessment(true)) },
    )
    .await
    .unwrap();
    assert!(!bounded.search_complete);
    assert_eq!(bounded.candidates.len(), 1);
    assert_eq!(bounded.best, Some(0));
}

#[tokio::test]
async fn singleton_family_is_complete_with_one_assessment() {
    let (request, case) = example();
    let result = search(
        &request,
        &case,
        SearchConfig {
            beam_width: 1,
            max_candidates: 1,
        },
        |_| async { Ok(assessment(false)) },
    )
    .await
    .unwrap();
    assert!(result.search_complete);
    assert_eq!(result.candidates.len(), 1);
    assert!(result.candidates[0].candidate.selections.is_empty());
}

#[test]
fn imported_candidate_cannot_change_joint_or_cumulative_composition() {
    let (_, mut case) = example();
    case.compositions[0].relationship = SupportRelationship::Cumulative;
    let candidate = Candidate {
        selections: case
            .compositions
            .iter()
            .map(|composition| (composition.conclusion_id.clone(), Vec::new()))
            .collect(),
        dependency_ids: Vec::new(),
        length: 0,
    };
    assert_eq!(
        serde_json::to_value(candidate.apply(&case)).unwrap(),
        serde_json::to_value(case).unwrap()
    );
}

#[tokio::test]
async fn invalid_configuration_or_source_stops_before_assessment() {
    let (request, case) = alternative_case();
    for config in [
        SearchConfig {
            beam_width: 0,
            max_candidates: 1,
        },
        SearchConfig {
            beam_width: 65,
            max_candidates: 1,
        },
        SearchConfig {
            beam_width: 1,
            max_candidates: 0,
        },
        SearchConfig {
            beam_width: 1,
            max_candidates: 257,
        },
    ] {
        assert!(
            search(&request, &case, config, |_| async {
                panic!("invalid input must not invoke assessment")
            })
            .await
            .is_err()
        );
    }
    let mut invalid_case = case;
    invalid_case.warrants[0].premise_ids.push("absent".into());
    assert!(
        search(&request, &invalid_case, exhaustive(), |_| async {
            panic!("invalid source must not invoke assessment")
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn excessive_choice_points_stop_before_assessment() {
    let (request, mut case) = alternative_case();
    for index in 0..65 {
        let id = format!("S_many_{index}");
        let warrant_id = format!("W_many_{index}");
        let mut warrant = case.warrants[0].clone();
        warrant.id = warrant_id.clone();
        warrant.conclusion_id = id.clone();
        warrant.premise_ids = vec!["G1".into()];
        case.warrants.push(warrant);
        case.subclaims.push(ClaimNode {
            id: id.clone(),
            text: "A separately identified test subclaim.".into(),
            qualifier: "Conditional test fixture.".into(),
        });
        case.compositions.push(SupportComposition {
            conclusion_id: id.clone(),
            relationship: SupportRelationship::AlternativeRoutes,
            selected_warrant_ids: vec![warrant_id],
            rationale: "Test fixture selects its sole declared route.".into(),
        });
        let mut paragraph = case.reading[0].paragraphs[0].clone();
        paragraph.reference_ids = vec![id.clone()];
        case.reading.push(NarrativeSection {
            claim_id: id.clone(),
            title: "A test subclaim.".into(),
            paragraphs: vec![paragraph],
        });
        case.warrants
            .iter_mut()
            .find(|warrant| warrant.id == "W2")
            .unwrap()
            .premise_ids
            .push(id);
    }
    validate_argument(&request, &case).unwrap();
    assert!(matches!(
        search(&request, &case, exhaustive(), |_| async {
            panic!("overlarge family must not invoke assessment")
        })
        .await,
        Err(Error::Budget(_))
    ));
}

#[tokio::test]
async fn assessment_errors_propagate_instead_of_becoming_support() {
    let (request, case) = alternative_case();
    assert!(matches!(
        search(&request, &case, exhaustive(), |_| async {
            Err(Error::ToolExecution(
                "independent checker unavailable".into(),
            ))
        })
        .await,
        Err(Error::ToolExecution(_))
    ));
}

/// Deliberately slow specification walk, independent of ArgumentIndex and
/// Candidate::apply. Reference occurrences become a set only after traversal.
fn oracle_length(case: &ArgumentCase, selections: &BTreeMap<String, Vec<String>>) -> usize {
    let mut pending = vec!["current_claim"];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(composition) = case
            .compositions
            .iter()
            .find(|composition| composition.conclusion_id == id)
        {
            let selected = selections
                .get(id)
                .unwrap_or(&composition.selected_warrant_ids);
            for warrant in case
                .warrants
                .iter()
                .filter(|warrant| warrant.conclusion_id == id && selected.contains(&warrant.id))
            {
                pending.push(&warrant.id);
                pending.extend(warrant.premise_ids.iter().map(String::as_str));
            }
        }
        for assumption in &case.assumptions {
            if assumption.applies_to.iter().any(|target| target == id) {
                pending.push(&assumption.id);
            }
        }
        for defeater in &case.defeaters {
            if defeater.target_id == id {
                pending.push(&defeater.id);
            }
        }
    }
    visited.len()
}

fn adversarial_support(selections: &BTreeMap<String, Vec<String>>) -> bool {
    (selections["S1"] == ["W1"] && selections["current_claim"] == ["W2"])
        || (selections["S1"] == ["W1b"] && selections["current_claim"] == ["A_short"])
}

#[tokio::test]
async fn exhaustive_search_matches_independent_oracle_while_narrow_beam_can_miss_optimum() {
    let (request, mut case) = two_dimensions();
    case.warrants
        .iter_mut()
        .find(|warrant| warrant.id == "W1b")
        .unwrap()
        .premise_ids = vec!["G2".into()];
    case.warrants
        .iter_mut()
        .find(|warrant| warrant.id == "A_short")
        .unwrap()
        .premise_ids = vec!["S1".into()];
    // Enumerate the declared family directly, without using the search's
    // dimension builder, cache, candidate application or dependency index.
    let mut oracle = Vec::new();
    for nested in ["W1", "W1b"] {
        for root in ["A_short", "W2"] {
            let selections = BTreeMap::from([
                ("S1".into(), vec![nested.into()]),
                ("current_claim".into(), vec![root.into()]),
            ]);
            if adversarial_support(&selections) {
                oracle.push((oracle_length(&case, &selections), selections));
            }
        }
    }
    oracle.sort();
    let complete = search(&request, &case, exhaustive(), |candidate| async move {
        Ok(assessment(adversarial_support(&candidate.selections)))
    })
    .await
    .unwrap();
    assert!(complete.search_complete);
    let best = &complete.candidates[complete.best.unwrap()].candidate;
    assert_eq!((best.length, best.selections.clone()), oracle[0]);
    assert_eq!(best.selections["S1"], ["W1b"]);
    assert_eq!(best.selections["current_claim"], ["A_short"]);
    for result in &complete.candidates {
        assert_eq!(
            result.candidate.length,
            oracle_length(&case, &result.candidate.selections)
        );
    }
    let narrow = search(
        &request,
        &case,
        SearchConfig {
            beam_width: 1,
            max_candidates: 32,
        },
        |candidate| async move { Ok(assessment(adversarial_support(&candidate.selections))) },
    )
    .await
    .unwrap();
    assert!(!narrow.search_complete);
    assert!(narrow.pruned_states > 0);
    let narrow_best = &narrow.candidates[narrow.best.unwrap()].candidate;
    assert!(narrow_best.length > best.length);
}

#[tokio::test]
async fn oversized_nodes_or_references_are_rejected_before_validation_and_assessment() {
    let (request, case) = alternative_case();
    let mut excessive_nodes = case.clone();
    excessive_nodes
        .grounds
        .resize(MAX_INPUT_NODES + 1, case.grounds[0].clone());
    let mut excessive_edges = case;
    excessive_edges.warrants[0]
        .premise_ids
        .resize(MAX_INPUT_EDGES + 1, "G1".into());
    for source in [excessive_nodes, excessive_edges] {
        assert!(matches!(
            search(&request, &source, exhaustive(), |_| async {
                panic!("oversized source must not invoke assessment")
            })
            .await,
            Err(Error::Budget(_))
        ));
    }
}
