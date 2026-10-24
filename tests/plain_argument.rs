use engarg::{
    domain::{ArgumentCase, ClaimNode, Request, SupportComposition, SupportRelationship},
    presentations,
};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

#[test]
fn plain_argument_retains_graph_meaning_and_explains_shared_support_once() {
    let (request, case) = fixtures();
    let before = serde_json::to_value(&case).unwrap();
    let output = presentations::plain_lines(&request, &case, 3);
    assert_eq!(output.matches(&case.subclaims[0].text).count(), 1);
    assert!(output.contains("See S1 for the shared argument."));
    assert!(output.contains("These supporting arguments are jointly required."));
    assert!(output.contains("Every following premise is jointly required for this inference."));
    assert!(output.contains(&case.current_claim.qualifier));
    for warrant in &case.warrants {
        assert!(output.contains(&format!("{} — {}", warrant.id, warrant.rule)));
    }
    for need in &case.evidence_plan {
        assert!(output.contains(&format!(
            "The investigation {} addresses {}.",
            need.id, need.target_id
        )));
        assert!(output.contains(&need.acceptance_criterion));
        assert!(output.contains(&need.if_failed));
    }
    assert_eq!(serde_json::to_value(&case).unwrap(), before);
}

#[test]
fn plain_argument_distinguishes_composition_and_selection() {
    let (request, mut case) = fixtures();
    let item = case
        .compositions
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap();
    item.relationship = SupportRelationship::AlternativeRoutes;
    item.selected_warrant_ids.clear();
    let output = presentations::plain_lines(&request, &case, 3);
    assert!(output.contains("These supporting arguments provide alternative routes."));
    assert!(output.contains("This route remains a candidate."));
    case.compositions
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap()
        .relationship = SupportRelationship::Cumulative;
    assert!(
        presentations::plain_lines(&request, &case, 3)
            .contains("These supporting arguments contribute cumulative support.")
    );
}

#[test]
fn plain_argument_keeps_technical_syntax_and_authored_sentence_boundaries() {
    let (request, mut case) = fixtures();
    let literal = "TraceQL uses { span.duration > 1.25s && resource.service.name = \"pilot:sync\" }. See https://example.test/v1.2.";
    case.grounds[0].statement = literal.into();
    let output = presentations::plain_lines(&request, &case, 3);
    assert!(output.contains(literal));
    assert!(!output.contains("Qualification:"));
    assert!(!output.contains("Premises jointly required:"));
    assert!(!output.contains("candidate route, not selected"));
}

#[test]
fn plain_argument_display_depth_preserves_boundary_conditions() {
    let (request, case) = fixtures();
    let warrant = case
        .warrants
        .iter()
        .find(|item| item.conclusion_id == "S1")
        .unwrap();
    let output = presentations::plain_lines_with_depth(&request, &case, 3, Some(1));
    assert!(!output.contains(&format!("{} — {}", warrant.id, warrant.rule)));
    assert!(output.contains("The canonical JSON retains the deeper supporting detail."));
    assert!(output.contains(&case.current_claim.qualifier));
    assert!(output.contains(&case.assumptions[0].statement));
    assert!(output.contains(&case.defeaters[0].challenge));
}

#[test]
fn plain_argument_places_a_challenge_beside_its_assumption() {
    let (request, mut case) = fixtures();
    let mut challenge = case.defeaters[0].clone();
    challenge.id = "D-assumption".into();
    challenge.target_id = case.assumptions[0].id.clone();
    challenge.challenge = "The workload sample may omit the busiest operating period.".into();
    challenge.response =
        "A comparison with production workload records would assess this coverage.".into();
    case.defeaters.push(challenge.clone());
    let output = presentations::plain_lines(&request, &case, 3);
    let assumption = output
        .find(&format!("The assumption {} is", case.assumptions[0].id))
        .unwrap();
    let challenge_position = output[assumption..]
        .find(&format!("The challenge {} is", challenge.id))
        .unwrap();
    assert!(output[assumption + challenge_position..].contains(&challenge.response));

    let mut condition = case.assumptions[0].clone();
    condition.id = "A-challenge".into();
    condition.statement = "The workload audit preserves its sampling frame.".into();
    condition.applies_to = vec![challenge.id];
    case.assumptions.push(condition.clone());
    let output = presentations::plain_lines(&request, &case, 3);
    assert!(output.contains(&format!(
        "The assumption {} is {}",
        condition.id, condition.statement
    )));
}

#[test]
fn plain_argument_bounds_indentation_and_retains_deep_argument_records() {
    let (request, mut case) = fixtures();
    let original = case.warrants[0].clone();
    for index in 0..50 {
        let id = format!("Deep{index}");
        case.subclaims.push(ClaimNode {
            id: id.clone(),
            text: format!("Deep claim {index}."),
            qualifier: "Within this synthetic example.".into(),
        });
        let mut warrant = original.clone();
        warrant.id = format!("DW{index}");
        warrant.conclusion_id = id.clone();
        warrant.premise_ids = vec![if index == 0 {
            "G1".into()
        } else {
            format!("Deep{}", index - 1)
        }];
        case.compositions.push(SupportComposition {
            conclusion_id: id,
            relationship: SupportRelationship::JointlyRequired,
            selected_warrant_ids: vec![warrant.id.clone()],
            rationale: "This synthetic inference requires the stated premise.".into(),
        });
        case.warrants.push(warrant);
    }
    case.warrants
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap()
        .premise_ids = vec!["Deep49".into()];
    let output = presentations::plain_lines(&request, &case, 2);
    assert!(output.contains("The following record develops the supporting argument for Deep49."));
    assert_eq!(output.matches("Deep claim 49.").count(), 1);
    assert_eq!(output.matches("Deep claim 0.").count(), 1);
    assert!(
        output
            .lines()
            .all(|line| line.chars().take_while(|ch| *ch == ' ').count() <= 2)
    );
}
