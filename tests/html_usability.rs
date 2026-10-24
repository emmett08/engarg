//! Standalone HTML retains complete records behind native disclosure controls.
use engarg::{
    domain::{ArgumentCase, Request},
    presentations,
};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn anchor(id: &str) -> String {
    format!(
        "node-{}",
        id.as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[test]
fn investigation_details_are_retained_once_and_linked_from_the_argument() {
    let (request, mut case) = fixtures();
    let need = &mut case.evidence_plan[0];
    need.acceptance_criterion =
        "A distinctive complete acceptance criterion remains inspectable.".into();
    need.method = "A distinctive collection method stays with its canonical investigation.".into();
    let id = anchor(&need.id);
    let criterion = need.acceptance_criterion.clone();
    let method = need.method.clone();
    let output = presentations::html(&request, &case).unwrap();
    assert_eq!(output.matches(&criterion).count(), 1);
    assert_eq!(output.matches(&method).count(), 1);
    assert!(output.contains(&format!("href=\"#{id}\"")));
    let record = output
        .split_once(&format!("<details class=\"record\" id=\"{id}\">"))
        .unwrap()
        .1
        .split_once("</details>")
        .unwrap()
        .0;
    let (summary, body) = record.split_once("</summary>").unwrap();
    assert!(summary.starts_with("<summary>"));
    assert!(!summary.contains(&criterion));
    assert!(body.contains(&criterion));
    assert!(body.contains(&method));
}

#[test]
fn standalone_html_uses_native_controls_without_scripts_or_external_styles() {
    let (request, case) = fixtures();
    let output = presentations::html(&request, &case).unwrap();
    assert!(output.contains("<style>"));
    assert!(output.contains("style-src 'unsafe-inline'"));
    assert!(!output.contains("<script"));
    assert!(!output.contains("<link"));
    assert!(!output.contains("onclick="));
    assert!(!output.contains("aria-expanded="));
    assert!(output.contains("press Enter or Space"));
    assert!(output.contains("Open this file in a web browser"));
    for (opening, closing) in [("<details", "</details>"), ("<summary>", "</summary>")] {
        assert_eq!(
            output.matches(opening).count(),
            output.matches(closing).count()
        );
    }
}

#[test]
fn attached_challenge_response_and_basis_remain_in_the_disclosed_record() {
    let (request, mut case) = fixtures();
    case.defeaters[0].response =
        "A distinctive retained response remains available for inspection.".into();
    let response = case.defeaters[0].response.clone();
    let basis = serde_json::to_value(case.defeaters[0].basis)
        .unwrap()
        .as_str()
        .unwrap()
        .replace('_', " ");
    let output = presentations::html(&request, &case).unwrap();
    assert_eq!(output.matches(&response).count(), 1);
    assert!(output.contains(&format!("Basis: {basis}.")));
    assert!(output.contains("Response and investigation record"));
}
