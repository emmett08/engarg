//! Admit bounded reference labels before constructing dependent diagnostics.

use engarg::{
    domain::{
        ArgumentCase, DiagnosticCode, NarrativeSection, ParagraphPurpose, ReasoningParagraph,
        Request, validate_argument,
    },
    presentations::{self, RenderError},
    render,
};

#[cfg(target_os = "linux")]
use engarg::presentations::RenderLimits;

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn unknown_reading_target() -> (Request, ArgumentCase) {
    let (request, mut case) = fixtures();
    case.reading.push(NarrativeSection {
        claim_id: "q".repeat(65_536),
        title: "Unknown target".into(),
        paragraphs: vec![
            ReasoningParagraph {
                purpose: ParagraphPurpose::Answer,
                text: String::new(),
                reference_ids: Vec::new(),
            };
            1_000
        ],
    });
    (request, case)
}

fn invalid_declaration_with_repeated_edges() -> (Request, ArgumentCase) {
    let (request, mut case) = fixtures();
    case.grounds[0].id = "q".repeat(65_536);
    case.grounds[0].evidence_ids = vec!["E1".into(); 4_000];
    (request, case)
}

fn assert_reference_rejection(request: &Request, case: &ArgumentCase, expected: &str) {
    let diagnostics = validate_argument(request, case).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::ReferenceIntegrity);
    assert_eq!(diagnostics[0].location, "argument");
    assert!(diagnostics[0].message.contains(expected));
    // Keep the full offending ID once. The correction preserves specificity
    // rather than truncating it or retaining one copy per dependent paragraph.
    assert_eq!(
        diagnostics[0].message.matches(&"q".repeat(65_536)).count(),
        1
    );
    assert!(diagnostics[0].message.len() < 2 * 65_536);
    for result in [
        presentations::plain_lines(request, case, 3),
        presentations::outline(request, case, 3),
        presentations::prose(request, case),
        presentations::html(request, case),
        render::markdown(request, case),
    ] {
        assert_eq!(result, Err(RenderError::InvalidStructure));
    }
}

#[test]
fn unknown_reading_targets_fail_reference_admission_before_paragraph_policy() {
    let (request, case) = unknown_reading_target();
    assert_reference_rejection(&request, &case, "references unknown claim");
}

#[test]
fn invalid_declaration_ids_stop_dependent_reference_diagnostics() {
    let (request, case) = invalid_declaration_with_repeated_edges();
    assert_reference_rejection(&request, &case, "invalid identifier");
}

#[test]
fn reading_sections_require_claims_and_paragraphs_require_distinct_canonical_references() {
    for target in ["G1", "W1", "A1", "D1", "E1", "missing"] {
        let (request, mut case) = fixtures();
        case.reading[0].claim_id = target.into();
        let diagnostics = validate_argument(&request, &case).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code == DiagnosticCode::ReferenceIntegrity)
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("references unknown claim"))
        );
    }
    let (request, mut case) = fixtures();
    case.reading[0].paragraphs[0].text.clear();
    case.reading[0].paragraphs[0].reference_ids = vec![
        "missing".into(),
        "current_claim".into(),
        "current_claim".into(),
    ];
    let diagnostics = validate_argument(&request, &case).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code == DiagnosticCode::ReferenceIntegrity)
    );
    for expected in ["unknown canonical ID", "repeats canonical ID"] {
        assert!(diagnostics.iter().any(|d| d.message.contains(expected)));
    }
}

#[test]
fn invalid_request_ids_stop_before_case_reference_admission() {
    let (mut request, case) = unknown_reading_target();
    request.evidence[0].id = "q".repeat(65_536);
    let diagnostics = validate_argument(&request, &case).unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::InvalidRequest);
    assert!(diagnostics[0].message.contains("invalid identifier"));
}

#[test]
fn known_reference_policy_defects_keep_the_labelled_inspection_path() {
    let (request, mut case) = fixtures();
    case.reading[0].title.clear();
    let diagnostics = validate_argument(&request, &case).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::NarrativePolicy)
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != DiagnosticCode::ReferenceIntegrity)
    );
    for result in [
        presentations::plain_lines(&request, &case, 3),
        presentations::outline(&request, &case, 3),
        presentations::prose(&request, &case),
        presentations::html(&request, &case),
        render::markdown(&request, &case),
    ] {
        assert!(
            result
                .unwrap()
                .contains("Inspection view. Domain validation failed.")
        );
    }
}

#[test]
fn saved_report_cli_rejects_unknown_labels_with_bounded_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    for (index, (request, case)) in [
        unknown_reading_target(),
        invalid_declaration_with_repeated_edges(),
    ]
    .into_iter()
    .enumerate()
    {
        let path = directory.path().join(format!("invalid-{index}.json"));
        let report =
            serde_json::to_vec(&serde_json::json!({"request":request,"argument":case})).unwrap();
        assert!(report.len() < 256 * 1024);
        std::fs::write(&path, report).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
            .args(["render", "--format", "markdown", "--input"])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.len() < 2 * 65_536);
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("q".repeat(65_536).as_str())
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn invalid_label_diagnostics_stay_within_the_working_allowance() {
    const CHILD: &str = "ENGARG_REFERENCE_ADMISSION_CHILD";
    const TEST: &str = "invalid_label_diagnostics_stay_within_the_working_allowance";
    if std::env::var(CHILD).as_deref() != Ok(TEST) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, TEST)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    for (request, case) in [
        unknown_reading_target(),
        invalid_declaration_with_repeated_edges(),
    ] {
        assert_eq!(
            render::markdown(&request, &case),
            Err(RenderError::InvalidStructure)
        );
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let peak_kib: usize = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    eprintln!("reference admission peak: {peak_kib} KiB");
    assert!(peak_kib * 1024 < RenderLimits::default().max_working_bytes);
}
