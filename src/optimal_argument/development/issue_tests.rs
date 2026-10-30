//! Review lifecycle tests separate model observations from checked resolution authority.
use super::*;

fn issue(kind: IssueKind, id: &str) -> CriticalIssue {
    CriticalIssue {
        id: id.into(),
        kind,
        target_id: "G1".into(),
        description:
            "Check the named obligation using the separately retained verification evidence.".into(),
        obligation_ids: vec!["support_G1".into()],
        check_ids: vec!["check_G1".into()],
        artifacts: vec![],
    }
}

fn criticism(issues: &[CriticalIssue]) -> String {
    json!({"critical_issues":issues,"observations":[]}).to_string()
}

#[test]
fn generated_criticism_is_typed_while_legacy_findings_remain_unresolved_other() {
    let legacy = json!({"critical_issues":["An older substantive interpretation problem."],"observations":[]});
    let retained: Criticism = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(retained.critical_issues[0].kind, IssueKind::Other);
    let reencoded = serde_json::to_value(&retained).unwrap();
    assert!(reencoded["critical_issues"][0].is_object());
    let schema = crate::optimal_argument::development::critic::schema();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(!validator.is_valid(&legacy));
    assert!(validator.is_valid(&reencoded));
    assert!(
        crate::optimal_argument::development::critic::parse(&legacy.to_string(), 1_048_576)
            .is_err()
    );
    let typed = criticism(&[issue(IssueKind::MissingEvidence, "needs_source")]);
    assert!(crate::optimal_argument::development::critic::parse(&typed, 1_048_576).is_ok());
    let mut forged: Value = serde_json::from_str(&typed).unwrap();
    forged["resolved_issues"] = json!(["needs_source"]);
    assert!(!validator.is_valid(&forged));
    assert!(
        crate::optimal_argument::development::critic::parse(&forged.to_string(), 1_048_576)
            .is_err()
    );
}

#[tokio::test]
async fn substantive_issue_survives_presentation_changes_and_critic_omission() {
    let original = report();
    let short = shorter(&original);
    let mut changed_title = short.clone();
    changed_title.argument.reading[0]
        .title
        .push_str(" (new presentation)");
    let concern = issue(IssueKind::Semantic, "meaning_gap");
    let model = MockModel::new(vec![
        Ok(envelope(&short)),
        Ok(criticism(std::slice::from_ref(&concern))),
        Ok(envelope(&changed_title)),
        Ok(clear_critic()),
    ]);
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &Investigator::supported(),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    assert_ne!(
        result.candidates[1].proposal_sha256,
        result.candidates[2].proposal_sha256
    );
    assert_eq!(result.best, None);
    assert_eq!(result.issues.len(), 1);
    assert_eq!(
        result.issues[0].issue_sha256,
        issue_digest(&concern).unwrap()
    );
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.pending_critical_issues == [concern.clone()])
    );
    let next_prompt: Value = serde_json::from_str(&model.requests.lock().unwrap()[2].user).unwrap();
    assert_eq!(
        next_prompt["registered_issues"][0]["issue"]["kind"],
        "semantic"
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProofFault {
    None,
    IssueHash,
    CandidateHash,
    VerificationHash,
    Kind,
    CheckCoverage,
    MissingArtifact,
    ArtifactHash,
    WrongArtifactDigest,
    ArtifactPath,
    HookError,
    SemanticApproval,
}

/// A synthetic independent investigator makes the additional named source
/// obligation pass only on the third assessed candidate. No model supplies it.
struct CheckedFixture {
    inner: Investigator,
    fault: ProofFault,
}

impl CandidateInvestigator for CheckedFixture {
    async fn assess(
        &self,
        parent: &RunReport,
        proposal: SupportedArgument,
        index: usize,
    ) -> Result<CandidateOutcome, Error> {
        let raw_digest = digest(&proposal)?;
        let mut outcome = self.inner.assess(parent, proposal, index).await?;
        let report = outcome.report.as_ref().unwrap();
        let ready = index >= 2;
        outcome.assessment.supported = ready;
        outcome.assessment.status = if ready {
            "supported"
        } else {
            "verification_incomplete"
        }
        .into();
        outcome.verification = json!({
            "fixture_only":true,
            "checks":[{"id":"check_G1","target_id":"G1","status":if ready {"pass"} else {"unknown"}}],
            "lifecycle":{
                "phase":if ready {"post_execution"} else {"pre_execution"},
                "authored_status":report.argument.status,
                "runtime_status":if ready {"supported"} else {"not_run"},
                "bindings":{
                    "raw_candidate_sha256":raw_digest,
                    "admitted_candidate_sha256":candidate_digest(report)?,
                    "source_digest":"1".repeat(64),"executable_digest":"2".repeat(64),
                    "collection_id":format!("fixture_collection_{index}"),
                    "assessment_id":format!("fixture_assessment_{index}"),
                    "normalization_checked":ready,"replay_checked":ready
                }
            },
            "verification_reports":[{"target_id":"G1","obligations":[{"id":"source_G1","status":if ready {"pass"} else {"unknown"}}]}]
        });
        Ok(outcome)
    }

    async fn resolve_issues(
        &self,
        outcome: &CandidateOutcome,
        issues: &[CriticalIssue],
    ) -> Result<Vec<CheckedIssueResolution>, Error> {
        if outcome.verification["checks"][0]["status"] != "pass" {
            return Ok(vec![]);
        }
        if self.fault == ProofFault::HookError {
            return Err(Error::InvalidInput(
                "Synthetic resolution service failed".into(),
            ));
        }
        let mut resolutions = Vec::new();
        for issue in issues {
            if issue.check_ids != ["check_G1"] {
                continue;
            }
            let kind = match issue.kind {
                IssueKind::MissingEvidence => IssueResolutionKind::ObservedEvidence,
                IssueKind::MissingVerification => IssueResolutionKind::PassedVerification,
                IssueKind::Semantic if self.fault == ProofFault::SemanticApproval => {
                    IssueResolutionKind::ObservedEvidence
                }
                _ => continue,
            };
            let receipt_digest = digest(&outcome.verification)?;
            let mut resolution = CheckedIssueResolution {
                issue_sha256: issue_digest(issue)?,
                candidate_sha256: candidate_digest(outcome.report.as_ref().unwrap())?,
                verification_sha256: receipt_digest.clone(),
                kind,
                check_ids: vec!["check_G1".into()],
                artifacts: vec![ArtifactReference {
                    path: format!("{}/verification.json", outcome.assessment.artifact),
                    pointer: Some("/checks/0".into()),
                    sha256: Some(receipt_digest),
                }],
            };
            match self.fault {
                ProofFault::IssueHash => resolution.issue_sha256 = "a".repeat(64),
                ProofFault::CandidateHash => resolution.candidate_sha256 = "b".repeat(64),
                ProofFault::VerificationHash => resolution.verification_sha256 = "c".repeat(64),
                ProofFault::Kind => resolution.kind = IssueResolutionKind::PassedVerification,
                ProofFault::CheckCoverage => resolution.check_ids.clear(),
                ProofFault::MissingArtifact => resolution.artifacts.clear(),
                ProofFault::ArtifactHash => resolution.artifacts[0].sha256 = None,
                ProofFault::WrongArtifactDigest => {
                    resolution.artifacts[0].sha256 = Some("d".repeat(64))
                }
                ProofFault::ArtifactPath => {
                    resolution.artifacts[0].path = "../another-candidate/verification.json".into()
                }
                _ => {}
            }
            resolutions.push(resolution);
        }
        Ok(resolutions)
    }
}

async fn checked_run(
    fault: ProofFault,
    issues: &[CriticalIssue],
) -> (DevelopmentResult, Vec<CompletionRequest>) {
    let original = report();
    let first = shorter(&original);
    let mut repaired = first.clone();
    repaired.argument.reading[0]
        .title
        .push_str(" (source obligation checked)");
    let model = MockModel::new(vec![
        Ok(envelope(&first)),
        Ok(criticism(issues)),
        Ok(envelope(&repaired)),
        Ok(clear_critic()),
    ]);
    let investigator = CheckedFixture {
        inner: Investigator::supported(),
        fault,
    };
    let result = develop(
        &model,
        &original,
        config(2, 4),
        &json!({}),
        &investigator,
        &|_| Ok(()),
    )
    .await
    .unwrap();
    let requests = model.requests.into_inner().unwrap();
    (result, requests)
}

#[tokio::test]
async fn exact_investigator_evidence_resolves_only_the_checked_candidate() {
    let concern = issue(IssueKind::MissingEvidence, "source_pending");
    let (result, requests) = checked_run(ProofFault::None, std::slice::from_ref(&concern)).await;
    assert_eq!(result.best, Some(2));
    assert!(result.candidates[0].issue_resolutions.is_empty());
    assert!(result.candidates[1].issue_resolutions.is_empty());
    assert_eq!(result.candidates[2].issue_resolutions.len(), 1);
    assert_eq!(
        result.candidates[2].issue_resolutions[0].issue_sha256,
        issue_digest(&concern).unwrap()
    );
    assert!(result.candidates[2].pending_critical_issues.is_empty());
    assert_eq!(
        result.candidates[2]
            .outcome
            .report
            .as_ref()
            .unwrap()
            .argument
            .status,
        crate::domain::CaseStatus::Conditional
    );
    assert_eq!(
        result.candidates[2]
            .outcome
            .report
            .as_ref()
            .unwrap()
            .argument
            .grounds[0]
            .status,
        crate::domain::GroundStatus::Missing
    );
    let pre: Value = serde_json::from_str(&requests[1].user).unwrap();
    let post: Value = serde_json::from_str(&requests[3].user).unwrap();
    assert_eq!(pre["evidence_context"]["review_phase"], "pre_execution");
    assert_eq!(post["evidence_context"]["review_phase"], "post_execution");
    assert_eq!(post["evidence_context"]["authored_status"], "conditional");
    assert_eq!(
        post["evidence_context"]["runtime_assessment"]["supported"],
        true
    );
    assert_eq!(
        post["evidence_context"]["evaluated_coverage"][0]["obligations"][0]["status"],
        "pass"
    );
    assert_eq!(
        post["evidence_context"]["checked_identity_bindings"]["normalization_checked"],
        true
    );
    assert_eq!(
        post["evidence_context"]["checked_identity_bindings"]["replay_checked"],
        true
    );
    assert_eq!(result.calls[1].phase, "pre_execution_sceptic");
    assert_eq!(result.calls[3].phase, "post_execution_sceptic");
    assert!(
        requests[3]
            .system
            .contains("not the deliberate separation of authored and runtime states")
    );
}

#[tokio::test]
async fn wrong_or_incomplete_resolution_bindings_cannot_clear_a_finding() {
    for fault in [
        ProofFault::IssueHash,
        ProofFault::CandidateHash,
        ProofFault::VerificationHash,
        ProofFault::Kind,
        ProofFault::CheckCoverage,
        ProofFault::MissingArtifact,
        ProofFault::ArtifactHash,
        ProofFault::WrongArtifactDigest,
        ProofFault::ArtifactPath,
        ProofFault::HookError,
    ] {
        let (result, _) = checked_run(
            fault,
            &[issue(IssueKind::MissingEvidence, "source_pending")],
        )
        .await;
        assert_eq!(result.best, None);
        assert!(result.candidates[2].outcome.assessment.supported);
        assert!(!result.candidates[2].eligible);
        assert_eq!(result.candidates[2].pending_critical_issues.len(), 1);
        assert!(result.candidates[2].issue_resolutions.is_empty());
    }
}

#[tokio::test]
async fn source_check_cannot_resolve_semantic_adequacy_or_overwrite_a_colliding_id() {
    let source = issue(IssueKind::MissingEvidence, "same_model_id");
    let mut meaning = issue(IssueKind::Semantic, "same_model_id");
    meaning.description =
        "The intended meaning still lacks an independent adequate reference.".into();
    let (result, _) = checked_run(ProofFault::SemanticApproval, &[source, meaning.clone()]).await;
    assert_eq!(result.issues.len(), 2);
    assert_eq!(result.best, None);
    assert_eq!(result.candidates[2].issue_resolutions.len(), 1);
    assert_eq!(result.candidates[2].pending_critical_issues, [meaning]);
}
