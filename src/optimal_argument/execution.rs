//! Execute each proposed reliance selection with fresh source-bound observations.
mod verification;

use super::{Assessment, Candidate};
use crate::{
    Error,
    eal_host::{self, HostConfig},
    engine::{RunReport, support::revision},
    export_policy::ExportPolicy,
    package,
};
use serde::Serialize;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

/// Operator-owned execution authority, shared without reusing candidate stores.
pub struct ExecutionConfig {
    pub tools: PathBuf,
    pub output: PathBuf,
    pub eal_host: PathBuf,
    pub timeout: Duration,
    pub collector_env: Vec<String>,
    pub public_collector_env: Vec<String>,
    pub policy: ExportPolicy,
}

fn directory(path: &Path) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

fn candidate_report(report: &RunReport, candidate: &Candidate) -> Result<RunReport, Error> {
    let argument = candidate.apply(&report.argument);
    let digest = |argument: &crate::domain::ArgumentCase| {
        eal_collector_contracts::execution::canonical_digest(argument).map_err(Error::InvalidInput)
    };
    if digest(&argument)? == digest(&report.argument)? {
        crate::report::validate(report)?;
        return Ok(report.clone());
    }
    // Selecting a different route is an explicit semantic revision. The full
    // support contract and every retained objection remain subject to admission.
    revision::revise_with_support(report, argument, report.support_elaboration.clone(), None)
}

fn outcome(status: &str, artifact: &str, reasons: Vec<String>) -> Assessment {
    Assessment {
        supported: status == "supported",
        status: status.into(),
        artifact: artifact.into(),
        reasons,
    }
}

fn failure(status: &str, artifact: &str, error: impl std::fmt::Display) -> Assessment {
    outcome(status, artifact, vec![error.to_string()])
}

/// Retain every candidate attempt. Failure to establish support is a result,
/// whereas inability to retain that result is an IO error for the whole run.
pub async fn assess(
    report: &RunReport,
    candidate: &Candidate,
    config: &ExecutionConfig,
    index: usize,
) -> Result<Assessment, Error> {
    let parent = config.output.join("candidates");
    match parent.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(Error::InvalidInput(
                "Candidate parent must be a real directory".into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => directory(&parent)?,
        Err(error) => return Err(error.into()),
    }
    let artifact = format!("candidates/{index:04}");
    let root = config.output.join(&artifact);
    directory(&root)?;
    // Retain a rejected proposal too, without representing it as an admitted run.
    write_json(
        &root.join("argument.json"),
        &candidate.apply(&report.argument),
    )?;
    let assessed = match candidate_report(report, candidate) {
        Err(error) => failure("invalid_revision", &artifact, error),
        Ok(revised) => {
            write_json(&root.join("report.json"), &revised)?;
            execute(&revised, config, &root, &artifact).await?
        }
    };
    write_json(&root.join("assessment.json"), &assessed)?;
    Ok(assessed)
}

async fn execute(
    report: &RunReport,
    config: &ExecutionConfig,
    root: &Path,
    artifact: &str,
) -> Result<Assessment, Error> {
    let Some(support) = &report.support_elaboration else {
        return Ok(failure(
            "build_failed",
            artifact,
            "Candidate has no executable support elaboration",
        ));
    };
    let packaged = root.join("package");
    let receipt = match package::build(
        &report.request,
        &report.argument,
        support,
        &config.tools,
        &packaged,
        &config.policy,
    ) {
        Ok(receipt) => receipt,
        Err(error) => return Ok(failure("build_failed", artifact, error)),
    };
    let source = String::from_utf8(crate::bounded_file::read(
        &packaged.join("case.eal"),
        16 * 1_048_576,
        "optimisation candidate source",
    )?)
    .map_err(|_| Error::InvalidInput("Candidate source must be UTF-8".into()))?;
    let tools = packaged.join("tools.toml");
    let manifest = eal_collector_contracts::RegistryDocument::from_toml(
        &String::from_utf8(crate::bounded_file::read(
            &tools,
            1_048_576,
            "optimisation candidate tools",
        )?)
        .map_err(|_| Error::InvalidInput("Candidate tools must be UTF-8".into()))?,
    )
    .map_err(Error::InvalidInput)?;
    let workspace = root.join("run");
    directory(&workspace)?;
    let host = HostConfig {
        executable: config.eal_host.clone(),
        workspace: workspace.clone(),
        database: Some(workspace.join("evidence.sqlite")),
        registry: Some(tools),
        collector_env: config.collector_env.clone(),
        public_collector_env: config.public_collector_env.clone(),
        url: None,
        token_env: None,
        timeout: config.timeout,
    };
    let context = serde_json::to_value(&manifest.context)?;
    let request = json!({
        "operation":"execute", "source":source, "context":context,
        "claim":receipt["claims"]["current_claim"]
    });
    let envelope = match eal_host::invoke(&host, &request).await {
        Ok(envelope) => envelope,
        Err(error) => {
            write_json(
                &root.join("execution-error.json"),
                &json!({"schema":"engarg.optimisation-execution-error/1", "reason":error.to_string()}),
            )?;
            return Ok(failure("execution_error", artifact, error));
        }
    };
    write_json(&root.join("execution.json"), &envelope)?;
    match verification::verify_execution(&source, &receipt, &envelope, &context) {
        Ok((status, reasons)) => Ok(outcome(&status, artifact, reasons)),
        Err(error) => Ok(failure("execution_error", artifact, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ArgumentCase, SupportRelationship};
    use std::collections::BTreeMap;

    fn authored_report() -> RunReport {
        let request: crate::domain::Request =
            serde_json::from_str(include_str!("../../examples/claim-only-request.json")).unwrap();
        let mut argument: ArgumentCase =
            crate::live_output::case(include_str!("../../examples/claim-only-case.json"));
        argument.alternatives.clear();
        let mut second = argument
            .warrants
            .iter()
            .find(|warrant| warrant.id == "W2")
            .unwrap()
            .clone();
        second.id = "W4".into();
        argument.warrants.push(second);
        argument
            .compositions
            .iter_mut()
            .find(|composition| composition.conclusion_id == "current_claim")
            .unwrap()
            .relationship = SupportRelationship::AlternativeRoutes;
        // This unresolved root challenge must survive every route selection.
        argument.defeaters[0].target_id = "current_claim".into();
        let mut requirements = Vec::new();
        let mut evaluations = Vec::new();
        for target in crate::support_requirements::material_targets(&argument) {
            let id = format!("requirement_{target}");
            let mut needs: Vec<_> = argument
                .evidence_plan
                .iter()
                .filter(|need| need.target_id == target)
                .collect();
            needs.sort_by_key(|need| &need.id);
            let role = if argument
                .defeaters
                .iter()
                .any(|defeater| defeater.id == target)
            {
                "challenge_resolution"
            } else {
                "support"
            };
            let criterion = json!({"kind":"unavailable","reason":"A real scoped investigation is still required."});
            requirements.push(json!({
                "id":id,"role":role,"target_id":target,
                "evidence_need_ids":needs.iter().map(|need|&need.id).collect::<Vec<_>>(),
                "original_acceptance_criteria":needs.iter().map(|need|&need.acceptance_criterion).collect::<Vec<_>>(),
                "contract":null,"discriminating_evidence":[],"acceptance":criterion,
                "acceptance_rationale":"The source has not been observed.",
                "gaps":[{"kind":"clarification","description":"Bind and assess the scoped observation."}]
            }));
            evaluations.push(json!({
                "id":format!("evaluation_{id}"),"requirement_id":id,"inputs":[],
                "criterion":criterion,"max_age_seconds":null,"required_coverage":"unspecified",
                "applicability":[],"challenges":[]
            }));
        }
        let support: crate::support_requirements::SupportElaboration =
            serde_json::from_value(json!({
                "schema":crate::support_requirements::SUPPORT_ELABORATION_SCHEMA,
                "requirements":requirements,"acquisitions":[],"derivations":[],
                "evaluations":evaluations,"revisions":[],"alternative_support":[]
            }))
            .unwrap();
        let report: RunReport = serde_json::from_value(json!({
            "schema_version":"engarg.run.v3","execution_mode":"offline_synthetic_fixture",
            "input_sha256":eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
            "mode":"single","request":request,"argument":argument,"transcript":[],
            "support_elaboration":support,"direct_bindings":crate::direct_planning::DirectPlan::unbound(&support),
            "limitations":["Synthetic unobserved obligations; no empirical claim is established."]
        }))
        .unwrap();
        crate::report::validate(&report).unwrap();
        report
    }

    fn selection(warrant: &str) -> Candidate {
        Candidate {
            selections: BTreeMap::from([("current_claim".into(), vec![warrant.into()])]),
            dependency_ids: Vec::new(),
            length: 0,
        }
    }

    #[test]
    fn unchanged_selection_preserves_exact_report_and_history() {
        let original = authored_report();
        let baseline = candidate_report(&original, &selection("W2")).unwrap();
        assert_eq!(
            serde_json::to_value(&baseline).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert!(baseline.argument_revisions.is_empty());
    }

    #[test]
    fn selecting_another_route_retains_root_challenge_criteria_and_parent() {
        let original = authored_report();
        let before = serde_json::to_value(&original).unwrap();
        let changed = candidate_report(&original, &selection("W4")).unwrap();
        crate::report::validate(&changed).unwrap();
        assert_eq!(serde_json::to_value(&original).unwrap(), before);
        assert_eq!(changed.argument_revisions.len(), 1);
        let revision = &changed.argument_revisions[0];
        assert_eq!(
            serde_json::to_value(&revision.previous_report).unwrap(),
            before
        );
        assert_eq!(
            serde_json::to_value(&changed.support_elaboration).unwrap(),
            before["support_elaboration"]
        );
        assert_eq!(
            serde_json::to_value(&changed.argument.defeaters).unwrap(),
            before["argument"]["defeaters"]
        );
        assert_eq!(changed.argument.defeaters[0].target_id, "current_claim");
        assert!(
            revision
                .invalidated
                .iter()
                .any(|field| field == "assessment")
        );
        assert!(
            changed
                .argument
                .dissent_revisions
                .as_ref()
                .unwrap()
                .applications
                .is_empty()
        );
    }
}
