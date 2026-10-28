//! Standalone HTML retains complete records behind native disclosure controls.
use engarg::{
    domain::{ArgumentCase, Request},
    glossary, presentations,
};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

#[test]
fn typed_parts_offer_canonical_tooltips_and_keyboard_touch_glossary_links() {
    let (request, mut case) = fixtures();
    case.warrants[0]
        .backing_evidence_ids
        .push(request.evidence[0].id.clone());
    let output = presentations::html(&request, &case).unwrap();
    for code in [
        "C",
        "S",
        "G",
        "E",
        "W",
        "B",
        "Q",
        "R",
        "A",
        "D",
        "N",
        "ARG",
        "current_claim",
        "stronger_target",
        "scope",
        "criterion",
        "composition",
        "alternative",
        "assessment",
        "revision_trigger",
        "residual_risk",
        "decision_limit",
        "decision_basis",
    ] {
        let entry = glossary::find(code).unwrap();
        assert!(
            output.contains(&format!("data-glossary-code=\"{}\"", entry.code)),
            "missing {code}"
        );
        let title = format!(
            "{} — {}\n{}\n{}",
            entry.code, entry.name, entry.definition, entry.question
        );
        assert!(
            output.contains(&format!("title=\"{}\"", escaped(&title))),
            "incorrect {code} tooltip"
        );
        let target = format!("glossary-{}", anchor(code));
        let definition = format!("glossary-definition-{}", anchor(code));
        assert!(
            output.contains(&format!("href=\"#{target}\"")),
            "missing {code} help link"
        );
        assert!(
            output.contains(&format!("aria-describedby=\"{definition}\"")),
            "missing {code} accessible description"
        );
        assert!(
            output.contains(&format!(
                "aria-description=\"{}\"",
                escaped(&format!("{} {}", entry.definition, entry.question))
            )),
            "missing {code} closed-glossary description fallback"
        );
        assert_eq!(output.matches(&format!("id=\"{definition}\"")).count(), 1);
    }
    assert!(output.contains("pointer, touch, or keyboard"));
    assert!(!output.contains("<script"));
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
fn record_roles_do_not_depend_on_identifier_prefixes() {
    let (request, case) = fixtures();
    let mut encoded = serde_json::to_value(case).unwrap();
    fn rename(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) if text == "G1" => *text = "D-lookalike-ground".into(),
            serde_json::Value::Array(items) => items.iter_mut().for_each(rename),
            serde_json::Value::Object(fields) => fields.values_mut().for_each(rename),
            _ => {}
        }
    }
    rename(&mut encoded);
    let case = serde_json::from_value(encoded).unwrap();
    let output = presentations::html(&request, &case).unwrap();
    let reference = format!(
        "<a href=\"#{}\">D-lookalike-ground</a>",
        anchor("D-lookalike-ground")
    );
    let before_reference = output.split_once(&reference).unwrap().0;
    let part = before_reference
        .rsplit_once("data-glossary-code=\"")
        .unwrap()
        .1;
    assert!(
        part.starts_with("G\""),
        "identifier spelling replaced the typed role: {part}"
    );
}

#[test]
fn tooltip_markup_keeps_authored_html_inert_and_retains_alternative_checks() {
    let (request, mut case) = fixtures();
    let hostile = "<img src=x onerror=\"alert('authored')\"> & <script>unsafe()</script>";
    case.current_claim.text = hostile.into();
    case.current_claim.qualifier = "\" title=\"invented\" onmouseover=\"unsafe()\"".into();
    case.alternatives[0].explanation =
        "A distinctive competing account remains inspectable.".into();
    case.alternatives[0].distinguishing_test =
        "A distinctive discriminating check remains inspectable.".into();
    let output = presentations::html(&request, &case).unwrap();
    assert!(output.contains(&escaped(hostile)));
    assert!(output.contains(&escaped(&case.current_claim.qualifier)));
    assert!(!output.contains("<img"));
    assert!(!output.contains("<script"));
    assert!(!output.contains("onmouseover=\"unsafe()\""));
    assert_eq!(output.matches(&case.alternatives[0].explanation).count(), 1);
    assert_eq!(
        output
            .matches(&case.alternatives[0].distinguishing_test)
            .count(),
        1
    );
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
