use engarg::domain::{ArgumentCase, Request};
use engarg::render::{markdown, with_audit};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

#[test]
fn every_inference_has_all_fourteen_template_components() {
    let (request, case) = fixtures();
    let report = markdown(&request, &case);
    let labels = [
        "1. Claim —",
        "2. Grounds —",
        "3. Warrant —",
        "4. Backing",
        "5. Qualifier",
        "6. Rebuttal conditions",
        "7. Scope, definitions and criteria",
        "8. Evidence provenance and adequacy",
        "9. Critical assumptions and dependencies",
        "10. Competing explanations or options",
        "11. Challenges and responses",
        "12. Stronger target and evidence obligations",
        "13. Current assessment and defensible claim",
        "14. Decision basis and revision triggers",
    ];
    for label in labels {
        assert_eq!(
            report.matches(&format!("| {label}")).count(),
            case.warrants.len(),
            "{label}"
        );
    }
    for warrant in &case.warrants {
        let heading = format!("## Argument record — ARG-{}", warrant.id);
        let start = report.find(&heading).unwrap();
        let end = report[start + heading.len()..]
            .find("\n## ")
            .map(|offset| start + heading.len() + offset)
            .unwrap_or(report.len());
        let record = &report[start..end];
        assert!(record.contains(&format!("1. Claim — {}", warrant.conclusion_id)));
        assert!(record.contains(&warrant.rule));
        assert!(record.contains(&warrant.applicability));
    }
}

#[test]
fn shared_registers_keep_canonical_evidence_and_obligation_details() {
    let (request, case) = fixtures();
    let report = markdown(&request, &case);
    for section in [
        "## Argument context",
        "## Evidence register",
        "## Assumptions and dependencies",
        "## Challenge register",
        "## Evidence obligations for the stronger target",
        "## Assessed argument",
        "## Composition overview for a complex case",
        "### Claim register",
        "### Composition register",
        "### Relationship meanings",
        "### Composition checks",
        "### Whole-case conclusion",
    ] {
        assert!(report.contains(section), "{section}");
    }
    for evidence in &request.evidence {
        assert!(report.contains(&evidence.id));
        assert!(report.contains(&evidence.title));
        assert!(report.contains(&evidence.source));
        assert!(report.contains(&evidence.excerpt));
    }
    for need in &case.evidence_plan {
        for text in [
            &need.id,
            &need.question,
            &need.method,
            &need.measurement,
            &need.comparator,
            &need.acceptance_criterion,
            &need.if_failed,
            &need.enables_claim,
            &need.cost,
        ] {
            assert!(report.contains(text), "{text}");
        }
    }
    for assumption in &case.assumptions {
        assert!(report.contains(&assumption.statement));
        assert!(report.contains(&assumption.validation));
    }
    for challenge in &case.defeaters {
        assert!(report.contains(&challenge.challenge));
        assert!(report.contains(&challenge.response));
    }
    assert!(report.contains(&case.adjudication));
    for composition in &case.compositions {
        assert!(report.contains(&composition.rationale));
    }
    for section in &case.reading {
        assert!(report.contains(&section.title));
        for paragraph in &section.paragraphs {
            assert!(report.contains(&paragraph.text));
        }
    }
    assert!(!report.contains("[System or artefact/version]"));
    assert!(!report.contains("[C-current]"));
    assert!(!report.contains("[ ]"));
}

#[test]
fn alternative_route_selection_is_preserved_as_an_assessment() {
    let (request, mut case) = fixtures();
    let composition = case
        .compositions
        .iter_mut()
        .find(|c| c.conclusion_id == "current_claim")
        .unwrap();
    composition.relationship = engarg::domain::SupportRelationship::AlternativeRoutes;
    composition.rationale =
        "A separately assessed alternative is selected for this bounded case.".into();
    let chosen = composition.selected_warrant_ids[0].clone();
    let mut candidate = case
        .warrants
        .iter()
        .find(|w| w.id == chosen)
        .unwrap()
        .clone();
    candidate.id = "W-unselected".into();
    case.warrants.push(candidate);
    let report = markdown(&request, &case);
    assert!(report.contains("Relationship: alternative_routes."));
    assert!(report.contains(&format!("Selected warrants: {chosen}.")));
    assert!(report.contains("a retained, unselected candidate"));
    assert!(
        report.contains("A separately assessed alternative is selected for this bounded case.")
    );
    assert!(!report.contains("All attached routes are relied upon"));
}

#[test]
fn current_verdict_is_not_reused_as_a_subclaim_or_target_verdict() {
    let (request, case) = fixtures();
    let report = markdown(&request, &case);
    assert!(report.contains("Separate subclaim verdict not recorded"));
    assert!(report.contains("Conditional target. Its fulfilment has not been established"));
    assert!(report.contains("Current assessment: unsupported."));
    assert!(
        report.contains("Separate adequacy") || report.contains("No backing evidence supplied")
    );
    assert!(report.contains("Proposed criterion:"));
    assert!(report.contains("Inconclusive: leave the stronger target unresolved."));
}

#[test]
fn evidence_pipes_newlines_and_markup_cannot_add_table_rows_or_columns() {
    let (mut request, case) = fixtures();
    let tick = char::from(96);
    request.evidence[0].excerpt = format!("A | B\n# injected heading\n{tick}quoted{tick} <script>");
    let report = markdown(&request, &case);
    assert!(report.contains("A \\| B<br># injected heading<br>"));
    assert!(!report.contains("\n# injected heading"));
    assert!(report.contains("&lt;script&gt;"));
    let evidence_row = report
        .lines()
        .find(|line| line.contains("Supplied finding/excerpt:"))
        .unwrap();
    let mut escaped = false;
    let columns = evidence_row
        .chars()
        .filter(|&c| {
            if escaped {
                escaped = false;
                false
            } else if c == '\\' {
                escaped = true;
                false
            } else {
                c == '|'
            }
        })
        .count();
    assert_eq!(columns, 7);
}

#[test]
fn audit_fence_remains_intact_when_model_content_contains_fences() {
    let ticks = char::from(96).to_string().repeat(6);
    let result = with_audit("Report".into(), &serde_json::json!({"content":ticks})).unwrap();
    let expected = char::from(96).to_string().repeat(7);
    assert!(result.contains(&format!("{expected}json")));
    assert!(result.ends_with(&format!("{expected}\n")));
}
