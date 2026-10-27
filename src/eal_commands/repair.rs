//! Bounded recovery checkpoints every paid exchange before interpreting it.
use super::*;
use engarg::{
    engine::{RunConfig, support::recovery::repair_support},
    provider::{Completion, CompletionRequest, LanguageModel},
    support_requirements::{SupportedArgument, review_acquisition_design},
};
use std::sync::Mutex;

#[derive(Args)]
#[command(mut_arg("max_output_tokens", |arg| arg.default_value("65536")))]
#[command(mut_arg("request_timeout_secs", |arg| arg.default_value("1800")))]
pub struct RepairArgs {
    /// Completed graph analysis with retained authoring responses.
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    evidence_config: PathBuf,
    /// New report. The original is retained unchanged in its recovery provenance.
    #[arg(short, long)]
    output: PathBuf,
    /// New private audit, retained on errors, cancellation and exhausted repair.
    #[arg(long)]
    audit: PathBuf,
    /// Separate allowance. Previous authoring calls are never repeated.
    #[arg(long, default_value_t = 1, value_parser=clap::value_parser!(u8).range(1..=3))]
    max_repairs: u8,
    #[arg(long, default_value_t = 3600, value_parser=clap::value_parser!(u64).range(1..=86400))]
    timeout_secs: u64,
    #[arg(long)]
    progress: bool,
    #[command(flatten)]
    provider: crate::tooling_commands::provider::ProviderArgs,
}

fn append(audit: &Mutex<File>, event: &Value) -> Result<(), Error> {
    let mut file = audit
        .lock()
        .map_err(|_| Error::InvalidInput("repair audit lock poisoned".into()))?;
    serde_json::to_writer(&mut *file, event)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

struct AuditModel<'a, M> {
    inner: M,
    audit: &'a Mutex<File>,
}
impl<M: LanguageModel> LanguageModel for AuditModel<'_, M> {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        // Persist diagnostics and the complete request before spending tokens.
        append(
            self.audit,
            &json!({"event":"repair_request","request":request}),
        )?;
        self.inner.complete(request).await
    }
}

pub async fn execute(args: RepairArgs) -> Result<(), Error> {
    let progress = crate::progress::Progress::new(args.progress);
    progress
        .run("support recovery", execute_inner(args, &progress))
        .await
}

async fn execute_inner(
    args: RepairArgs,
    progress: &crate::progress::Progress,
) -> Result<(), Error> {
    let started = std::time::Instant::now();
    progress.phase("validating saved analysis and recovery provenance");
    let raw: Value = crate::read_json(&args.input, MAX_INPUT_BYTES)?;
    let executable = std::env::current_exe()?;
    report::validate_replanning(&raw, &executable)?;
    report::validate_support_recovery(&raw)?;
    if report::support_recovery_depth(&raw)? >= 3 {
        return Err(Error::Budget(
            "support recovery retains at most three prior reports".into(),
        ));
    }
    engarg::tooling::direct::revalidate_report(&raw, &executable)?;
    let EvidencePlanningReport::Graph(previous) = report::classify_evidence_planning(&raw)? else {
        return Err(Error::InvalidInput(
            "support repair requires a saved graph analysis".into(),
        ));
    };
    let canonical = report::read_direct(&raw)?;
    if canonical.transcript.is_empty() {
        return Err(Error::InvalidInput(
            "support recovery requires retained authoring calls".into(),
        ));
    }
    let source_digest =
        eal_collector_contracts::execution::canonical_digest(&raw).map_err(Error::InvalidInput)?;
    let output_path = new_path(&args.output)?;
    let audit_path = new_path(&args.audit)?;
    if audit_path == output_path {
        return Err(Error::InvalidInput(
            "repair output and audit must use distinct new paths".into(),
        ));
    }
    let mut output = PendingFile::create(output_path)?;
    let audit = Mutex::new(crate::private_file(&audit_path)?);
    let config = RunConfig {
        max_repairs: args.max_repairs,
        max_calls: u32::from(args.max_repairs),
        max_argument_depth: canonical
            .run_config
            .as_ref()
            .and_then(|config| config.max_argument_depth),
        max_input_bytes: args.provider.max_input_bytes,
        max_response_bytes: args.provider.max_response_bytes,
        ..RunConfig::default()
    };
    let original = SupportedArgument {
        argument: canonical.argument,
        support_elaboration: previous.elaboration,
    };
    // Reject deterministic configuration failures before spending a model call.
    let planner = crate::support_commands::admit(&args.evidence_config).await?;
    let model = AuditModel {
        inner: progress.model(args.provider.model()?),
        audit: &audit,
    };
    append(
        &audit,
        &json!({"event":"support_recovery_start","input_report_sha256":source_digest,"recorded_calls":canonical.transcript.len(),"config":config,"provider_config":args.provider.metadata()}),
    )?;
    let observer =
        |record: &CallRecord| append(&audit, &json!({"event":"completion","record":record}));
    progress.phase("repairing retained acquisition design");
    let recovery = async {
        let (candidate, calls) = repair_support(
            &model,
            &canonical.request,
            &original,
            &canonical.transcript,
            &config,
            &observer,
        )
        .await?;
        progress.phase("matching repaired support requirements to capabilities");
        let routes = planner
            .resolve(&candidate.argument, &candidate.support_elaboration)
            .await?;
        let mut transcript = canonical.transcript.clone();
        transcript.extend(calls.clone());
        let design = review_acquisition_design(
            &candidate.support_elaboration,
            &transcript,
            MAX_INPUT_BYTES,
        )?;
        if let Some(warning) = design.warning() {
            eprintln!("engarg: warning: {warning}");
        }
        let mut updated = raw.clone();
        updated["evidence_planning"] = json!({"schema":"engarg.support-planning/1","elaboration":candidate.support_elaboration,"routes":routes,"design_review":design});
        if raw.get("support_elaboration").is_some() {
            updated["support_elaboration"] = serde_json::to_value(&candidate.support_elaboration)?;
            updated["direct_bindings"] = serde_json::to_value(
                engarg::direct_planning::DirectPlan::unbound(&candidate.support_elaboration),
            )?;
        }
        if raw.get("design_review").is_some() {
            updated["design_review"] = serde_json::to_value(&design)?;
        }
        updated["transcript"] = serde_json::to_value(&transcript)?;
        let object = updated.as_object_mut().expect("validated object");
        object.remove("tooling");
        object.remove("replanning");
        object.insert("support_recovery".into(), json!({
            "schema":"engarg.support-recovery/1","input_report_sha256":source_digest,
            "previous_report":raw,"recorded_calls":canonical.transcript.len(),
            "model_calls":calls.len(),"repair_limit":args.max_repairs,"provider_config":args.provider.metadata(),
            "audit_path":audit_path,"provider_requests":calls.iter().map(|call|&call.prompt).collect::<Vec<_>>()
        }));
        let warning = "Support recovery preserves the original argument and all recorded authoring calls. Retained pending designs are non-executable; remaining qualification, configuration and judgement gaps still require resolution.";
        if let Some(limitations) = updated["limitations"].as_array_mut() {
            limitations.push(json!(warning));
        }
        report::validate_support_recovery(&updated)?;
        let bytes = json_bytes(&updated)?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(Error::Budget(
                "recovered report exceeds 16 MiB; full exchanges remain in repair audit".into(),
            ));
        }
        output.write(&bytes)?;
        append(
            &audit,
            &json!({"event":"success","model_calls":calls.len()}),
        )?;
        output.keep = true;
        Ok(())
    };
    let remaining = Duration::from_secs(args.timeout_secs)
        .checked_sub(started.elapsed())
        .ok_or_else(|| {
            Error::Budget("support recovery deadline exhausted during admission".into())
        })?;
    let result = tokio::select! {
        result = tokio::time::timeout(remaining, recovery) => match result {
            Ok(result) => result,
            Err(_) => Err(Error::Budget("support recovery deadline exhausted; completed exchanges remain in repair audit".into())),
        },
        signal = tokio::signal::ctrl_c() => {signal?; Err(Error::Budget("support recovery interrupted; completed exchanges remain in repair audit".into()))},
    };
    if let Err(error) = &result {
        append(
            &audit,
            &json!({"event":"failure","error":error.to_string()}),
        )?;
    }
    result
}
