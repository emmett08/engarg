//! Shared lookups must not retain an expanded source copy for every reference.

#[cfg(target_os = "linux")]
fn child_process(test: &str) -> bool {
    const CHILD: &str = "ENGARG_TEMPLATE_EXPANSION_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(test) {
        return true;
    }
    // Each fresh process has a persistent high-water mark independent of other
    // tests. This avoids both timing samples and namespace-dependent child PIDs.
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, test)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&result.stderr));
    false
}

#[cfg(target_os = "linux")]
fn bounded_failure(request: &engarg::domain::Request, case: &engarg::domain::ArgumentCase) {
    use engarg::{
        presentations::{RenderError, RenderLimits},
        render,
    };
    let encoded = serde_json::to_vec(&(request, case)).unwrap();
    assert!(encoded.len() < 1024 * 1024);
    drop(encoded);
    let limits = RenderLimits::default();
    assert_eq!(
        render::markdown_with_limits(request, case, limits),
        Err(RenderError::ResourceLimit("output bytes"))
    );
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
    eprintln!("shared-lookup rendering peak: {peak_kib} KiB");
    assert!(
        peak_kib * 1024 < limits.max_working_bytes,
        "shared references used {peak_kib} KiB, exceeding {} working bytes",
        limits.max_working_bytes
    );
}

#[cfg(target_os = "linux")]
fn fixtures() -> (engarg::domain::Request, engarg::domain::ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

#[cfg(target_os = "linux")]
#[test]
fn shared_evidence_expansion_stays_within_the_working_allowance() {
    if !child_process("shared_evidence_expansion_stays_within_the_working_allowance") {
        return;
    }
    let (mut request, mut case) = fixtures();
    request.evidence[0].source = "x".repeat(64 * 1024);
    for index in 0..1_500 {
        let mut ground = case.grounds[0].clone();
        ground.id = format!("RepeatedGround{index}");
        case.warrants[0].premise_ids.push(ground.id.clone());
        case.grounds.push(ground);
    }
    assert_eq!(engarg::domain::validate_case(&request, &case), Ok(()));
    bounded_failure(&request, &case);
}

#[cfg(target_os = "linux")]
fn duplicate_compositions(alternative: bool) {
    let (request, mut case) = fixtures();
    let mut repeated = case
        .compositions
        .iter()
        .find(|composition| composition.conclusion_id == "current_claim")
        .unwrap()
        .clone();
    repeated.rationale = "x".into();
    if alternative {
        repeated.relationship = engarg::domain::SupportRelationship::AlternativeRoutes;
    }
    case.compositions
        .extend(std::iter::repeat_n(repeated, 2_000));
    case.compositions.last_mut().unwrap().rationale = "r".repeat(64 * 1024);
    // Reference-safe inspection deliberately retains composition diagnostics.
    // The renderer must therefore budget its shared lookup expansion too.
    assert!(engarg::domain::validate_case(&request, &case).is_err());
    bounded_failure(&request, &case);
}

#[cfg(target_os = "linux")]
#[test]
fn duplicate_composition_support_expansion_stays_within_the_working_allowance() {
    if child_process("duplicate_composition_support_expansion_stays_within_the_working_allowance") {
        duplicate_compositions(false);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn duplicate_alternative_route_expansion_stays_within_the_working_allowance() {
    if child_process("duplicate_alternative_route_expansion_stays_within_the_working_allowance") {
        duplicate_compositions(true);
    }
}
