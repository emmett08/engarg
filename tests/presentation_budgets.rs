use engarg::{
    domain::{ArgumentCase, Request},
    presentations::{self, RenderError, RenderLimits},
    render,
};
fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}
fn chain(count: usize) -> (Request, ArgumentCase) {
    let (request, mut case) = fixtures();
    let original = case.assumptions[0].clone();
    for n in 0..count {
        let mut a = original.clone();
        a.id = format!("Long{n}");
        a.statement = format!("Independent attachment record {n}");
        a.critical = false;
        a.applies_to = vec![if n == 0 {
            "current_claim".into()
        } else {
            format!("Long{}", n - 1)
        }];
        case.assumptions.push(a);
    }
    (request, case)
}
#[test]
fn twelve_thousand_attachments_are_stack_safe_and_complete() {
    let (request, case) = chain(12_000);
    // The independent regression case fits the former CLI file allowance.
    assert!(serde_json::to_vec(&case).unwrap().len() < 16 * 1024 * 1024);
    let result = presentations::plain_lines(&request, &case, 3).unwrap();
    assert!(result.contains("Independent attachment record 11999"));
    assert_eq!(result.matches("The assumption Long11999 is").count(), 1);
    assert!(
        result.find("The assumption Long0 is").unwrap()
            < result.find("The assumption Long11999 is").unwrap()
    );
}
#[test]
fn every_backend_reports_output_exhaustion_without_successful_truncation() {
    let (request, case) = fixtures();
    let limits = RenderLimits {
        max_output_bytes: 32,
        ..Default::default()
    };
    for result in [
        presentations::plain_lines_with_limits(&request, &case, 3, None, limits),
        presentations::outline_with_limits(&request, &case, 3, None, limits),
        presentations::prose_with_limits(&request, &case, None, limits),
        presentations::html_with_limits(&request, &case, None, limits),
        render::markdown_with_limits(&request, &case, limits),
    ] {
        assert_eq!(result, Err(RenderError::ResourceLimit("output bytes")));
    }
}
#[test]
fn every_backend_obeys_shared_graph_work_and_storage_limits() {
    let (request, case) = fixtures();
    for limits in [
        RenderLimits {
            max_visits: 1,
            ..Default::default()
        },
        RenderLimits {
            max_nodes: 1,
            ..Default::default()
        },
        RenderLimits {
            max_edges: 1,
            ..Default::default()
        },
        RenderLimits {
            max_input_bytes: 1,
            ..Default::default()
        },
        RenderLimits {
            max_working_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(presentations::plain_lines_with_limits(&request, &case, 3, None, limits).is_err());
        assert!(presentations::outline_with_limits(&request, &case, 3, None, limits).is_err());
        assert!(presentations::prose_with_limits(&request, &case, None, limits).is_err());
        assert!(presentations::html_with_limits(&request, &case, None, limits).is_err());
        assert!(render::markdown_with_limits(&request, &case, limits).is_err());
    }
}
#[test]
fn cyclic_and_shared_attachments_keep_order_and_terminate() {
    let (request, mut case) = chain(3);
    case.assumptions
        .iter_mut()
        .find(|a| a.id == "Long0")
        .unwrap()
        .applies_to
        .push("Long2".into());
    case.assumptions
        .iter_mut()
        .find(|a| a.id == "Long2")
        .unwrap()
        .applies_to
        .push("stronger_target".into());
    let mut d = case.defeaters[0].clone();
    d.id = "AfterChain".into();
    d.target_id = "Long1".into();
    d.challenge = "Nested challenge sentinel".into();
    case.defeaters.push(d);
    let result = presentations::plain_lines(&request, &case, 3).unwrap();
    assert!(result.contains("Nested challenge sentinel"));
    assert!(
        result.find("The assumption Long1 is").unwrap()
            < result.find("The assumption Long2 is").unwrap()
    );
    assert!(result.len() < 100_000);
}
#[test]
fn deep_challenge_chain_is_stack_safe_in_all_views() {
    let (request, mut case) = fixtures();
    let original = case.defeaters[0].clone();
    for n in 0..2_000 {
        let mut d = original.clone();
        d.id = format!("Attack{n}");
        d.target_id = if n == 0 {
            "current_claim".into()
        } else {
            format!("Attack{}", n - 1)
        };
        d.material = false;
        d.challenge = format!("Deep attack sentinel {n}");
        case.defeaters.push(d);
    }
    for result in [
        presentations::plain_lines(&request, &case, 3),
        presentations::outline(&request, &case, 3),
        presentations::html(&request, &case),
        render::markdown(&request, &case),
    ] {
        let text = result.unwrap();
        assert!(text.contains("Deep attack sentinel 1999"));
    }
}
#[test]
fn audit_append_has_the_same_output_boundary() {
    let limits = RenderLimits {
        max_output_bytes: 64,
        ..Default::default()
    };
    assert!(render::with_audit_limits("a".repeat(65), &serde_json::json!({}), limits).is_err());
    assert!(
        render::with_audit_limits(
            "report".into(),
            &serde_json::json!({"value":"a".repeat(1000)}),
            limits
        )
        .is_err()
    );
}

#[test]
fn audit_depth_is_checked_before_recursive_pretty_printing() {
    let mut value = serde_json::json!("leaf");
    for _ in 0..200 {
        value = serde_json::Value::Array(vec![value]);
    }
    let error = render::with_audit("report".into(), &value).unwrap_err();
    assert!(error.to_string().contains("audit graph"));
}

#[test]
fn inspection_rejects_ambiguous_duplicate_identities() {
    let (request, mut case) = fixtures();
    case.assumptions.push(case.assumptions[0].clone());
    assert_eq!(
        presentations::plain_lines(&request, &case, 3),
        Err(RenderError::InvalidStructure)
    );
    assert_eq!(
        render::markdown(&request, &case),
        Err(RenderError::InvalidStructure)
    );
}

#[test]
fn saved_report_cli_renders_twelve_thousand_attachments_without_aborting() {
    let (request, case) = chain(12_000);
    assert!(engarg::domain::validate_case(&request, &case).is_ok());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep-report.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"request":request,"argument":case})).unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["render", "--format", "argument", "--input"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Independent attachment record 11999")
    );
}

#[test]
fn all_backends_retain_nested_consequential_conditions_from_selected_support() {
    let (request, mut case) = fixtures();
    let mut assumption = case.assumptions[0].clone();
    assumption.id = "NestedCondition".into();
    assumption.statement = "The retained calibration covers the operating range".into();
    assumption.applies_to = vec![case.assumptions[0].id.clone()];
    case.assumptions.push(assumption);
    let mut challenge = case.defeaters[0].clone();
    challenge.id = "NestedMaterialChallenge".into();
    challenge.target_id = "NestedCondition".into();
    challenge.material = true;
    challenge.challenge = "The calibration could have drifted during this measurement".into();
    case.defeaters.push(challenge);
    for result in [
        presentations::plain_lines_with_depth(&request, &case, 3, Some(1)),
        presentations::outline_with_depth(&request, &case, 3, Some(1)),
        presentations::prose_with_depth(&request, &case, Some(1)),
        presentations::html_with_depth(&request, &case, Some(1)),
        render::markdown(&request, &case),
    ] {
        let text = result.unwrap();
        assert!(text.contains("The retained calibration covers the operating range"));
        assert!(text.contains("The calibration could have drifted during this measurement"));
    }
}

#[test]
fn policy_invalid_material_investigation_is_labelled_in_every_backend() {
    let (request, mut case) = fixtures();
    let challenge = case.defeaters.iter().find(|d| d.material).unwrap();
    let need = case
        .evidence_plan
        .iter_mut()
        .find(|n| n.target_id == challenge.id)
        .unwrap();
    let need_id = need.id.clone();
    need.acceptance_criterion.clear();
    let diagnostics = engarg::domain::validate_case(&request, &case).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.contains("acceptance_criterion must contain text"))
    );
    for result in [
        presentations::plain_lines(&request, &case, 3),
        presentations::outline(&request, &case, 3),
        presentations::prose(&request, &case),
        presentations::html(&request, &case),
        render::markdown(&request, &case),
    ] {
        let text = result.unwrap();
        assert!(text.contains("Inspection view. Domain validation failed."));
        assert!(text.contains("The recorded assessments require repair."));
        assert!(text.contains(&need_id));
        assert!(text.contains("must contain text"));
    }
}
