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

fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[test]
fn outline_explains_shared_claim_once_and_states_parent_child_relationships() {
    let (request, case) = fixtures();
    let output = presentations::outline(&request, &case, 3).unwrap();
    assert_eq!(output.matches(&case.subclaims[0].text).count(), 1);
    assert!(output.contains("See [S1]"));
    assert!(output.contains("Jointly required supporting arguments"));
    assert!(output.contains("Premises jointly required"));
    assert!(output.contains(&case.current_claim.qualifier));
    assert!(output.contains("Current assessment: **unsupported**"));
    for warrant in &case.warrants {
        assert!(output.contains(&format!("ARG-{}", warrant.id)));
    }
}

#[test]
fn alternative_and_cumulative_support_are_explicitly_distinguished() {
    let (request, mut case) = fixtures();
    let original = case
        .warrants
        .iter()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap()
        .clone();
    let mut alternative = original.clone();
    alternative.id = "W-alt".into();
    case.warrants.push(alternative);
    let composition = case
        .compositions
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap();
    composition.relationship = SupportRelationship::AlternativeRoutes;
    composition.selected_warrant_ids = vec![original.id.clone()];
    let output = presentations::outline(&request, &case, 3).unwrap();
    assert!(output.contains("Alternative supporting arguments"));
    assert!(output.contains("candidate route, not selected for reliance"));
    assert!(output.contains("relied-upon route"));
    case.compositions
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap()
        .relationship = SupportRelationship::Cumulative;
    assert!(
        presentations::outline(&request, &case, 3)
            .unwrap()
            .contains("Cumulative evidence and arguments")
    );
}

#[test]
fn outline_depth_limits_indentation_and_retains_deeper_argument_records() {
    let (request, mut case) = fixtures();
    let original = case.warrants[0].clone();
    for index in 0..150 {
        let id = format!("Deep{index}");
        case.subclaims.push(ClaimNode {
            id: id.clone(),
            text: format!("Deep claim {index}."),
            qualifier: "Within the synthetic example only.".into(),
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
            rationale: "The stated premise is required for this synthetic inference.".into(),
        });
        case.warrants.push(warrant);
    }
    case.warrants
        .iter_mut()
        .find(|item| item.conclusion_id == "current_claim")
        .unwrap()
        .premise_ids = vec!["Deep149".into()];
    let output = presentations::outline(&request, &case, 2).unwrap();
    assert!(output.contains("Separate argument records"));
    assert!(output.contains("Deep claim 149."));
    assert!(output.contains("Deep claim 0."));
    assert!(
        output
            .lines()
            .filter(|line| line.trim_start().starts_with("- "))
            .all(|line| line.chars().take_while(|ch| *ch == ' ').count() <= 2)
    );
    assert_eq!(output.matches("Deep claim 149.").count(), 1);
}

#[test]
fn collapsed_html_summaries_retain_qualifiers_and_consequential_conditions() {
    let (request, case) = fixtures();
    let output = presentations::html(&request, &case).unwrap();
    let first_summary = output
        .split_once("<summary>")
        .unwrap()
        .1
        .split_once("</summary>")
        .unwrap()
        .0;
    assert!(first_summary.contains(&escaped(&case.current_claim.text)));
    assert!(first_summary.contains(&escaped(&case.current_claim.qualifier)));
    assert!(first_summary.contains("Critical condition A1"));
    assert!(first_summary.contains(&escaped(&case.assumptions[0].statement)));
    let target_summary = output
        .split("<summary>")
        .nth(2)
        .unwrap()
        .split_once("</summary>")
        .unwrap()
        .0;
    assert!(target_summary.contains("Outstanding challenge D1"));
    assert_eq!(output.matches("<details id=\"node-5331\">").count(), 1);
}

#[test]
fn html_escapes_content_and_never_uses_supplied_source_as_a_link() {
    let (mut request, mut case) = fixtures();
    let attack = "<script>alert(1)</script><img src=x onerror=alert(2)>\"&'";
    case.current_claim.text = attack.into();
    case.current_claim.qualifier = attack.into();
    case.reading[0].paragraphs[0].text = attack.into();
    request.evidence[0].source = "javascript:alert(3)".into();
    request.evidence[0].excerpt = attack.into();
    let output = presentations::html(&request, &case).unwrap();
    assert!(!output.contains("<script>"));
    assert!(!output.contains("<img"));
    assert!(!output.contains("href=\"javascript:"));
    assert!(output.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(output.contains("&quot;&amp;&#39;"));
    assert!(output.contains("Content-Security-Policy"));
    assert!(!output.contains("<script "));
}

#[test]
fn prose_preserves_authored_paragraphs_with_references_and_no_fabricated_measurements() {
    let (request, mut case) = fixtures();
    // Repeated sections should remain one shared explanation even if an invalid
    // unvalidated record is passed directly to this presentation function.
    let section = case.reading[1].clone();
    case.reading.push(section.clone());
    let output = presentations::prose(&request, &case).unwrap();
    for paragraph in &section.paragraphs {
        assert_eq!(output.matches(&paragraph.text).count(), 1);
        assert!(output.contains(&format!(
            "(References: {}.)",
            paragraph.reference_ids.join(", ")
        )));
    }
    assert!(output.contains(&case.current_claim.qualifier));
    assert!(!output.contains("measured a 40% improvement"));
    assert!(!output.contains("Purpose: answer"));
    assert!(!output.contains("- The supplied"));
}

#[test]
fn authored_target_is_explained_once_with_its_canonical_conditions_beside_it() {
    let (request, mut case) = fixtures();
    let target = case
        .reading
        .iter_mut()
        .find(|section| section.claim_id == "stronger_target")
        .unwrap();
    target.title = "Whether the performance target is warranted".into();
    let paragraphs = target.paragraphs.clone();
    let output = presentations::prose(&request, &case).unwrap();
    assert!(!output.contains("## Stronger conditional target\n"));
    assert_eq!(
        output
            .matches("Whether the performance target is warranted")
            .count(),
        1
    );
    assert_eq!(output.matches(&case.stronger_target.text).count(), 1);
    assert_eq!(output.matches(&case.stronger_target.qualifier).count(), 1);
    for paragraph in &paragraphs {
        assert_eq!(output.matches(&paragraph.text).count(), 1);
    }
    let html = presentations::html(&request, &case).unwrap();
    let reading = html
        .split_once("<section id=\"reading-view\">")
        .unwrap()
        .1
        .split_once("<h3>Limits on reliance</h3>")
        .unwrap()
        .0;
    assert_eq!(
        reading
            .matches("Whether the performance target is warranted")
            .count(),
        1
    );
    assert_eq!(
        reading
            .matches(&escaped(&case.stronger_target.text))
            .count(),
        1
    );
    assert_eq!(
        reading
            .matches(&escaped(&case.stronger_target.qualifier))
            .count(),
        1
    );
    assert!(!reading.contains("<h3>Stronger conditional target</h3>"));
}

#[test]
fn target_without_authored_section_still_has_one_explicit_scope_and_qualification() {
    let (request, mut case) = fixtures();
    case.reading
        .retain(|section| section.claim_id != "stronger_target");
    let output = presentations::prose(&request, &case).unwrap();
    assert_eq!(
        output.matches("## Stronger conditional target\n").count(),
        1
    );
    assert_eq!(output.matches(&case.stronger_target.text).count(), 1);
    let html = presentations::html(&request, &case).unwrap();
    let reading = html
        .split_once("<section id=\"reading-view\">")
        .unwrap()
        .1
        .split_once("<h3>Limits on reliance</h3>")
        .unwrap()
        .0;
    assert_eq!(
        reading
            .matches("<h3>Stronger conditional target</h3>")
            .count(),
        1
    );
    assert_eq!(
        reading
            .matches(&escaped(&case.stronger_target.text))
            .count(),
        1
    );
    assert_eq!(
        reading
            .matches(&escaped(&case.stronger_target.qualifier))
            .count(),
        1
    );
}

#[test]
fn display_depth_omits_deeper_details_without_changing_the_graph_or_hiding_conditions() {
    let (request, case) = fixtures();
    let before = serde_json::to_value(&case).unwrap();
    let deeper_warrant = case
        .warrants
        .iter()
        .find(|w| w.conclusion_id == "S1")
        .unwrap();
    let deeper_reading = case.reading.iter().find(|s| s.claim_id == "S1").unwrap();
    let outline = presentations::outline_with_depth(&request, &case, 3, Some(1)).unwrap();
    assert!(!outline.contains(&format!("ARG-{} — Warrant", deeper_warrant.id)));
    assert!(outline.contains("Supporting detail collapsed by display-depth"));
    assert!(outline.contains("Qualification:"));
    assert!(outline.contains(&case.current_claim.qualifier));
    assert!(outline.contains(&case.assumptions[0].statement));
    assert!(outline.contains(&case.defeaters[0].challenge));
    let prose = presentations::prose_with_depth(&request, &case, Some(1)).unwrap();
    let html = presentations::html_with_depth(&request, &case, Some(1)).unwrap();
    assert!(!prose.contains(&deeper_reading.paragraphs[0].text));
    assert!(!html.contains(&escaped(&deeper_reading.paragraphs[0].text)));
    for output in [&prose, &html] {
        assert!(output.contains("canonical JSON"));
        // These fixture conditions have no Markdown/HTML metacharacters.
        assert!(output.contains(&case.assumptions[0].statement));
        assert!(output.contains(&case.defeaters[0].challenge));
    }
    assert!(!html.contains(&format!("<h4>ARG-{}", deeper_warrant.id)));
    let full = presentations::outline_with_depth(&request, &case, 3, Some(2)).unwrap();
    assert!(full.contains(&format!("ARG-{} — Warrant", deeper_warrant.id)));
    assert_eq!(serde_json::to_value(&case).unwrap(), before);
}
