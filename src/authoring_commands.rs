//! Inspect and resume initial authoring without repeating completed model calls.
use clap::{Args, Subcommand};
use engarg::{
    Error,
    authoring_checkpoint::{self, Checkpoint, Store},
    engine::{CallRecord, RouteResolver},
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

#[derive(Subcommand)]
pub enum AuthoringCommand {
    /// Inspect a private checkpoint without inference.
    Inspect(Source),
    /// Resume final validation with a separately bounded repair allowance.
    Resume(Box<ResumeArgs>),
}
#[derive(Args)]
pub struct Source {
    #[arg(long)]
    checkpoint: PathBuf,
}
impl Source {
    fn load(&self) -> Result<Checkpoint, Error> {
        Checkpoint::read(&self.checkpoint)
    }
}
#[derive(Args)]
#[command(mut_arg("max_output_tokens", |arg| arg.default_value("65536")))]
#[command(mut_arg("request_timeout_secs", |arg| arg.default_value("1800")))]
pub struct ResumeArgs {
    #[command(flatten)]
    source: Source,
    /// Explicit direct operation bindings retained by the checkpoint.
    #[arg(long, conflicts_with = "evidence_config")]
    tools: Option<PathBuf>,
    /// Optional capability-discovery configuration for an explicitly graph-authored checkpoint.
    #[arg(long)]
    evidence_config: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    audit: PathBuf,
    #[arg(long)]
    next_checkpoint: PathBuf,
    /// Acknowledge changed or unrecorded reasoning protocol. History is revalidated.
    #[arg(long)]
    accept_protocol_migration: bool,
    #[arg(long, default_value_t=1,value_parser=clap::value_parser!(u8).range(1..=3))]
    max_repairs: u8,
    #[arg(long,default_value_t=3600,value_parser=clap::value_parser!(u64).range(1..=86400))]
    timeout_secs: u64,
    #[arg(long)]
    progress: bool,
    #[command(flatten)]
    provider: crate::tooling_commands::provider::ProviderArgs,
}
enum RecoveryPlanner {
    Direct(engarg::direct_planning::DirectPlanner),
    Discovery(Box<crate::support_commands::CliPlanner>),
}
impl RouteResolver for RecoveryPlanner {
    type Plan = Value;
    fn authoring_context(&self) -> Option<Value> {
        match self {
            Self::Direct(planner) => planner.authoring_context(),
            Self::Discovery(planner) => planner.authoring_context(),
        }
    }
    fn preview(&self, plan: &Value) -> Result<Value, Error> {
        match self {
            Self::Direct(_) => Ok(plan.clone()),
            Self::Discovery(planner) => planner.preview(&serde_json::from_value(plan.clone())?),
        }
    }
    async fn resolve(
        &self,
        case: &engarg::domain::ArgumentCase,
        elaboration: &engarg::support_requirements::SupportElaboration,
    ) -> Result<Value, Error> {
        match self {
            Self::Direct(planner) => {
                serde_json::to_value(planner.resolve(case, elaboration).await?).map_err(Error::from)
            }
            Self::Discovery(planner) => {
                serde_json::to_value(planner.resolve(case, elaboration).await?).map_err(Error::from)
            }
        }
    }
}
fn append(audit: &Mutex<File>, event: &Value) -> Result<(), Error> {
    let mut file = audit
        .lock()
        .map_err(|_| Error::InvalidInput("authoring audit lock poisoned".into()))?;
    serde_json::to_writer(&mut *file, event)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
struct AuditedModel<'a, M> {
    inner: M,
    audit: &'a Mutex<File>,
}
impl<M: LanguageModel> LanguageModel for AuditedModel<'_, M> {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        append(
            self.audit,
            &json!({"event":"resume_request","request":request}),
        )?;
        self.inner.complete(request).await
    }
}

/// Publish only a complete accepted report and never overwrite an existing path.
fn publish(path: &Path, value: &Value) -> Result<(), Error> {
    let bytes = authoring_checkpoint::admitted_report_bytes(value)?;
    let path = std::path::absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidInput("output has no parent".into()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos();
    let temporary = parent.join(format!(
        ".authoring-report-{}-{stamp}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut file = authoring_checkpoint::private_new(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::hard_link(&temporary, &path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(temporary);
    result
}
fn require_new(path: &Path) -> Result<(), Error> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(Error::InvalidInput(format!(
            "authoring recovery requires a new output path: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub async fn execute(command: AuthoringCommand) -> Result<(), Error> {
    match command {
        AuthoringCommand::Inspect(source) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&source.load()?.inspection())?
            );
            Ok(())
        }
        AuthoringCommand::Resume(args) => {
            let progress = crate::progress::Progress::new(args.progress);
            progress
                .run("authoring recovery", resume(*args, &progress))
                .await
        }
    }
}
async fn resume(args: ResumeArgs, progress: &crate::progress::Progress) -> Result<(), Error> {
    let started = std::time::Instant::now();
    let saved = args.source.load()?;
    let parent_digest = authoring_checkpoint::digest(serde_json::to_vec(&saved)?);
    if saved.protocol_identity != authoring_checkpoint::protocol_identity()
        && !args.accept_protocol_migration
    {
        return Err(Error::InvalidInput("authoring protocol differs or is unrecorded; inspect first and supply --accept-protocol-migration to revalidate historical calls under the current protocol".into()));
    }
    if saved.inspection()["resumable"] != true {
        return Err(Error::InvalidInput(
            "only completed final authoring stages can resume; inspect the checkpoint".into(),
        ));
    }
    if saved
        .transcript
        .len()
        .saturating_add(usize::from(args.max_repairs))
        > authoring_checkpoint::MAX_CALLS
    {
        return Err(Error::Budget(
            "requested repair allowance exceeds bounded checkpoint history".into(),
        ));
    }
    let evidence = if let Some(path) = args.tools.as_ref().or(args.evidence_config.as_ref()) {
        engarg::bounded_file::read(path, 1_048_576, "resume evidence configuration")?
    } else {
        b"[tools]\n".to_vec()
    };
    if saved.evidence_config_sha256 != authoring_checkpoint::digest(&evidence) {
        return Err(Error::InvalidInput("resume evidence configuration differs from the recorded configuration; preserve the original configuration for authoring recovery and replan after acceptance".into()));
    }
    let paths = [
        authoring_checkpoint::output_identity(&args.output)?,
        authoring_checkpoint::output_identity(&args.audit)?,
        authoring_checkpoint::output_identity(&args.next_checkpoint)?,
    ];
    if paths[0] == paths[1] || paths[0] == paths[2] || paths[1] == paths[2] {
        return Err(Error::InvalidInput(
            "resume output, audit and next checkpoint require distinct new paths".into(),
        ));
    }
    for path in &paths {
        require_new(path)?;
    }
    let direct = args.evidence_config.is_none();
    let planner = if let Some(path) = &args.evidence_config {
        RecoveryPlanner::Discovery(Box::new(crate::support_commands::admit(path).await?))
    } else {
        RecoveryPlanner::Direct(
            args.tools
                .as_deref()
                .map(engarg::direct_planning::DirectPlanner::from_tools)
                .transpose()?
                .unwrap_or_default(),
        )
    };
    let model = args.provider.model()?;
    let audit = Mutex::new(authoring_checkpoint::private_new(&args.audit)?);
    let mut next = saved.clone();
    next.source = json!({"kind":"authoring_resume","parent_checkpoint_sha256":parent_digest,
        "previous_protocol":saved.protocol_identity,"protocol_migration":saved.protocol_identity!=authoring_checkpoint::protocol_identity(),
        "additional_repair_limit":args.max_repairs,"stage_byte_limits":{"max_input_bytes":args.provider.max_input_bytes,"max_response_bytes":args.provider.max_response_bytes},"previous_provider_config":saved.provider_config});
    next.protocol_identity = authoring_checkpoint::protocol_identity();
    next.producer_version = env!("CARGO_PKG_VERSION").into();
    next.provider_config = args.provider.metadata();
    next.failure = None;
    let checkpoint = Store::create(&args.next_checkpoint, next)?;
    append(
        &audit,
        &json!({"event":"authoring_resume_start","parent_checkpoint_sha256":parent_digest,
        "recorded_calls":saved.transcript.len(),"additional_repairs":args.max_repairs,"provider_config":args.provider.metadata()}),
    )?;
    let observer = |record: &CallRecord| {
        append(&audit, &json!({"event":"completion","record":record}))?;
        checkpoint.completion(record)
    };
    let model = AuditedModel {
        inner: progress.model(model),
        audit: &audit,
    };
    let recovery = async {
        let report = engarg::engine::support::authoring::resume(
            &model,
            &saved,
            engarg::engine::support::authoring::ResumeConfig {
                additional_repairs: args.max_repairs,
                max_input_bytes: args.provider.max_input_bytes,
                max_response_bytes: args.provider.max_response_bytes,
                accept_protocol_migration: args.accept_protocol_migration,
            },
            &observer,
            &planner,
        )
        .await?;
        let added = report.report.transcript.len() - saved.transcript.len();
        let mut value = serde_json::to_value(report)?;
        if direct {
            let planning = value
                .as_object_mut()
                .unwrap()
                .remove("evidence_planning")
                .unwrap();
            value["support_elaboration"] = planning["elaboration"].clone();
            value["direct_bindings"] = planning["routes"].clone();
            value["design_review"] = planning["design_review"].clone();
        }
        let (parent_metadata, parent_call_order) = saved.compact_parent()?;
        value["authoring_recovery"] = json!({"schema":"engarg.authoring-recovery/1","parent_checkpoint_sha256":parent_digest,"previous_checkpoint":parent_metadata,"previous_call_order":parent_call_order,
            "recorded_calls":saved.transcript.len(),"new_model_calls":added,"additional_repair_limit":args.max_repairs,
            "stage_byte_limits":{"max_input_bytes":args.provider.max_input_bytes,"max_response_bytes":args.provider.max_response_bytes},
            "previous_provider_config":saved.provider_config,"provider_config":args.provider.metadata(),
            "protocol_migration":saved.protocol_identity!=authoring_checkpoint::protocol_identity(),
            "previous_protocol":saved.protocol_identity,"current_protocol":authoring_checkpoint::protocol_identity()});
        engarg::authoring_checkpoint::validate_recovery_report(&value)?;
        publish(&args.output, &value)?;
        append(&audit, &json!({"event":"success","new_model_calls":added}))?;
        Ok(())
    };
    let remaining = Duration::from_secs(args.timeout_secs)
        .checked_sub(started.elapsed())
        .ok_or_else(|| Error::Budget("authoring recovery admission exhausted deadline".into()))?;
    let result = tokio::select! {
        result=tokio::time::timeout(remaining,recovery)=>match result {Ok(result)=>result,Err(_)=>Err(Error::Budget("authoring recovery deadline exhausted".into()))},
        signal=tokio::signal::ctrl_c()=>{signal?;Err(Error::Budget("authoring recovery interrupted".into()))},
    };
    if let Err(error) = &result {
        checkpoint.failure(error)?;
        append(
            &audit,
            &json!({"event":"failure","stage":"final_authoring_validation","error":error.to_string()}),
        )?;
        eprintln!(
            "Retained authoring checkpoint: {}",
            args.next_checkpoint.display()
        );
    }
    result
}
