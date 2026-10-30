//! Apply fixed assurance obligations before adopting and executing generated arguments.
use super::{
    Assessment,
    assurance::{self, AssurancePlan},
    development::{CandidateInvestigator, CandidateOutcome},
    execution::{self, ExecutionConfig},
};
use crate::{
    Error,
    direct_planning::DirectPlanner,
    engine::{RunReport, support::revision},
    support_requirements::SupportedArgument,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// All validation authority comes from the admitted input and operator plan.
pub struct VerifiedInvestigator {
    pub original: RunReport,
    pub plan: AssurancePlan,
    pub execution: ExecutionConfig,
}

fn write(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn directory(path: &Path) -> Result<(), Error> {
    if path.exists() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

fn same(left: &impl Serialize, right: &impl Serialize) -> Result<bool, Error> {
    let digest = |value| {
        eal_collector_contracts::execution::canonical_digest(value).map_err(Error::InvalidInput)
    };
    Ok(digest(&serde_json::to_value(left)?)? == digest(&serde_json::to_value(right)?)?)
}

impl VerifiedInvestigator {
    fn retained_failure(
        &self,
        index: usize,
        proposal: &SupportedArgument,
        report: Option<RunReport>,
        status: &str,
        verification: Value,
        reasons: Vec<String>,
    ) -> Result<CandidateOutcome, Error> {
        directory(&self.execution.output.join("candidates"))?;
        let artifact = format!("candidates/{index:04}");
        let path = self.execution.output.join(&artifact);
        directory(&path)?;
        write(&path.join("proposal.json"), proposal)?;
        if let Some(report) = &report {
            write(&path.join("report.json"), report)?;
        }
        write(&path.join("verification.json"), &verification)?;
        let assessment = Assessment {
            supported: false,
            status: status.into(),
            artifact,
            reasons,
        };
        write(&path.join("assessment.json"), &assessment)?;
        Ok(CandidateOutcome {
            report,
            assessment,
            verification,
        })
    }
}

impl CandidateInvestigator for VerifiedInvestigator {
    async fn assess(
        &self,
        parent: &RunReport,
        proposal: SupportedArgument,
        index: usize,
    ) -> Result<CandidateOutcome, Error> {
        let mut receipt = assurance::prepare(&self.original, &proposal, &self.plan)?;
        let unchanged = same(&proposal.argument, &parent.argument)?
            && same(
                &Some(proposal.support_elaboration.clone()),
                &parent.support_elaboration,
            )?;
        // Keep an admitted baseline available for diagnosis-driven repair even
        // when a newly specified formalisation check fails on that baseline.
        let baseline = unchanged.then(|| parent.clone());
        if !receipt.ready_for_execution() {
            return self.retained_failure(
                index,
                &proposal,
                baseline,
                "verification_failed",
                serde_json::to_value(&receipt)?,
                vec!["Fixed assurance checks require repair before execution".into()],
            );
        }
        let admitted = if unchanged {
            Ok(parent.clone())
        } else {
            (|| {
                let bindings = DirectPlanner::from_tools(&self.execution.tools)?
                    .resolve_operations(&proposal.argument, &proposal.support_elaboration)?;
                revision::revise_with_support(
                    parent,
                    proposal.argument.clone(),
                    Some(proposal.support_elaboration.clone()),
                    Some(bindings),
                )
            })()
        };
        let report = match admitted {
            Ok(report) => report,
            Err(error) => {
                return self.retained_failure(
                    index,
                    &proposal,
                    None,
                    "invalid_revision",
                    serde_json::to_value(&receipt)?,
                    vec![error.to_string()],
                );
            }
        };
        let mut assessed = execution::assess_report(&report, &self.execution, index).await?;
        let path = self.execution.output.join(&assessed.artifact);
        write(&path.join("proposal.json"), &proposal)?;
        let execution_path = path.join("execution.json");
        let envelope = if execution_path.exists() {
            let bytes =
                crate::bounded_file::read(&execution_path, 16 * 1_048_576, "candidate execution")?;
            eal_collector_contracts::strict_json_exact(
                &bytes,
                eal_collector_contracts::JsonLimits {
                    max_bytes: 16 * 1_048_576,
                    ..Default::default()
                },
            )
            .map_err(Error::InvalidInput)?
        } else {
            json!({})
        };
        assurance::finish(&mut receipt, &proposal, &envelope, &self.plan)?;
        receipt.record_execution(&proposal, &report, &envelope, &assessed.status)?;
        if !receipt.passed() {
            assessed.supported = false;
            assessed.status = "verification_incomplete".into();
            assessed.reasons.push(
                "A mandatory premise, formalisation or applicability check has not passed".into(),
            );
        }
        write(&path.join("verification.json"), &receipt)?;
        // Retain the native assessment unchanged, alongside the overall verdict.
        write(&path.join("investigation-assessment.json"), &assessed)?;
        Ok(CandidateOutcome {
            report: Some(report),
            assessment: assessed,
            verification: serde_json::to_value(receipt)?,
        })
    }

    async fn resolve_issues(
        &self,
        outcome: &CandidateOutcome,
        issues: &[super::development::CriticalIssue],
    ) -> Result<Vec<super::development::CheckedIssueResolution>, Error> {
        assurance::resolution::resolve(outcome, issues)
    }
}
