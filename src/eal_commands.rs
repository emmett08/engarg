//! CLI boundary for deterministic EAL export and the official one-request host.
//!
//! Export and inspection never collect evidence. Collection is an explicit command.

use clap::{Args, Subcommand, ValueEnum};
use engarg::{
    Error,
    eal_host::{HostConfig, invoke},
    engine::{CallRecord, RouteResolver},
    report::{self, EvidencePlanningReport},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_INPUT_BYTES: usize = 16 * 1_048_576;
mod aspic;
mod repair;

#[derive(Subcommand)]
pub enum EalCommand {
    /// Rebuild evidence routes from a saved graph report without model calls.
    Replan(ReplanArgs),
    /// Check graph evidence freshness before implementing collectors.
    FreshnessCheck(FreshnessCheckArgs),
    /// Repair support design from retained calls while preserving the argument.
    Repair(repair::RepairArgs),
    /// Explicitly revise an argument, preserving its parent and invalidating derived work.
    Revise(super::package_commands::ReviseArgs),
    /// Implement missing collectors from a saved analysis using frozen fixtures.
    Implement(super::tooling_commands::ImplementArgs),
    /// Check the isolated compiler and runtime before any model calls.
    ToolchainCheck(super::tooling_commands::preflight::ToolchainCheckArgs),
    /// Reverify a retained candidate against its frozen fixtures without model calls.
    ToolReplay(super::tooling_commands::replay::ToolReplayArgs),
    /// Export a verified collector as a portable EAL tool package and TOML binding.
    ToolExport(super::tooling_commands::ToolExportArgs),
    /// Internal launcher for a verified generated collector package.
    #[command(hide = true)]
    ToolRun(super::tooling_commands::ToolRunArgs),
    /// Compile a saved report into a portable, executable EAL/4 package.
    Export(ExportArgs),
    /// Export a checked ASPIC+ viewer snapshot from a retained EAL execution.
    ExportAspic(aspic::ExportAspicArgs),
    /// Check EAL source using the official host; no evidence is collected.
    Validate(SourceCommand),
    /// Plan evidence requirements for an EAL claim; no evidence is collected.
    Plan(PlanArgs),
    /// Collect observations for a claim using the explicitly configured tools registry.
    Collect(CollectArgs),
    /// Assess with existing observations through the host's reason operation.
    Assess(AssessArgs),
    /// Recover a stored assessment or one claim's explanation.
    Explain(StoredArgs),
    /// Recover a bounded packet from a stored assessment.
    Packet(PacketArgs),
    /// Describe the language and methods exposed by the configured EAL server.
    Describe(HostOutput),
    /// Internal bounded adapter for an explicitly configured EAL collector.
    #[command(hide = true)]
    Observe(ObserveArgs),
}

#[derive(Args)]
pub struct ReplanArgs {
    /// Saved graph analysis; its request, argument and requirements are preserved.
    #[arg(long)]
    input: PathBuf,
    /// Current operator capability planning configuration.
    #[arg(long)]
    evidence_config: PathBuf,
    /// Explicit per-requirement missing-age clarification.
    #[arg(long)]
    freshness_clarifications: Option<PathBuf>,
    /// New report retaining the prior planning and tooling provenance.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Args)]
pub struct FreshnessCheckArgs {
    /// Saved graph analysis.
    #[arg(long)]
    input: PathBuf,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Saved canonical analysis with its complete support elaboration.
    #[arg(long)]
    input: PathBuf,
    /// Operator-authorised direct operation and reviewer bindings.
    #[arg(long)]
    tools: PathBuf,
    /// New directory containing case.eal, tools.toml and the build receipt.
    #[arg(short, long)]
    output: PathBuf,
    /// Explicit freshness policy for review obligations.
    #[arg(long)]
    export_policy: Option<PathBuf>,
}

#[derive(Args)]
pub struct HostOptions {
    /// Installed official eal-host executable. Never a shell command string.
    #[arg(long, env = "ENGARG_EAL_HOST", default_value = "eal-host")]
    eal_host: PathBuf,
    #[arg(long, default_value = ".")]
    workspace: PathBuf,
    #[arg(long)]
    database: Option<PathBuf>,
    /// Explicit local tools TOML registry, passed to eal-host as --registry.
    #[arg(long, conflicts_with = "url")]
    tools: Option<PathBuf>,
    /// Explicit local collector credential/configuration variable NAME. Repeat as needed.
    /// The name must appear in inherit_env of the selected operator registry.
    #[arg(long, requires = "tools")]
    collector_env: Vec<String>,
    /// Operator-declared public configuration among forwarded variable names.
    #[arg(long, requires = "tools")]
    public_collector_env: Vec<String>,
    /// Existing Streamable HTTP endpoint; omit to use the host's stdio transport.
    #[arg(long)]
    url: Option<String>,
    /// Name of an environment variable holding the EAL HTTP bearer token.
    #[arg(long)]
    token_env: Option<String>,
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout_secs: u64,
}

impl HostOptions {
    fn config(self) -> HostConfig {
        HostConfig {
            executable: self.eal_host,
            workspace: self.workspace,
            database: self.database,
            registry: self.tools,
            collector_env: self.collector_env,
            public_collector_env: self.public_collector_env,
            url: self.url,
            token_env: self.token_env,
            timeout: Duration::from_secs(self.timeout_secs),
        }
    }
}

#[derive(Args)]
pub struct HostOutput {
    #[command(flatten)]
    host: HostOptions,
    /// New file for the complete host JSON envelope. Omit for stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum InputFormat {
    Auto,
    Eal,
    Json,
}

#[derive(Args)]
pub struct SourceInput {
    /// EAL source or an EngArg EAL export bundle.
    #[arg(long)]
    input: PathBuf,
    /// Auto treats a leading JSON object as a bundle. Eal preserves raw text.
    #[arg(long, value_enum, default_value = "auto")]
    input_format: InputFormat,
}

#[derive(Args)]
pub struct SourceCommand {
    #[command(flatten)]
    source: SourceInput,
    #[command(flatten)]
    destination: HostOutput,
}

#[derive(Args)]
pub struct PlanArgs {
    #[command(flatten)]
    source: SourceInput,
    /// Canonical claim ID for a bundle, or the exact EAL claim name for raw source.
    #[arg(long, default_value = "current_claim")]
    claim: String,
    #[command(flatten)]
    destination: HostOutput,
}

#[derive(Args)]
pub struct AssessArgs {
    #[command(flatten)]
    source: SourceInput,
    /// Context JSON object. Otherwise use bundle context, or {} for raw EAL.
    #[arg(long)]
    context: Option<PathBuf>,
    /// Explicit assessment time in RFC 3339 form, interpreted by the EAL host.
    #[arg(long)]
    now: String,
    /// Existing collection identifier. This command does not create a collection.
    #[arg(long)]
    collection_id: Option<String>,
    #[command(flatten)]
    destination: HostOutput,
}

#[derive(Args)]
pub struct CollectArgs {
    #[command(flatten)]
    source: SourceInput,
    /// Canonical claim ID for a bundle, or the exact EAL claim name for raw source.
    #[arg(long, default_value = "current_claim")]
    claim: String,
    /// Context JSON object. Otherwise use bundle context, or {} for raw EAL.
    #[arg(long)]
    context: Option<PathBuf>,
    #[command(flatten)]
    destination: HostOutput,
}

#[derive(Args)]
pub struct ObserveArgs {
    #[arg(long)]
    tool: String,
    /// Operator-owned retained review inputs, required for review_json.
    #[arg(long)]
    inputs: Option<PathBuf>,
}

#[derive(Args)]
pub struct StoredArgs {
    #[arg(long)]
    assessment_id: String,
    /// Exact EAL claim name. Omit to retrieve the complete stored assessment.
    #[arg(long)]
    claim: Option<String>,
    #[command(flatten)]
    destination: HostOutput,
}

#[derive(Args)]
pub struct PacketArgs {
    #[arg(long)]
    assessment_id: String,
    #[arg(long)]
    claim: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(4096..=16_777_216))]
    max_bytes: Option<u64>,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=4096))]
    max_statement_bytes: Option<u64>,
    #[command(flatten)]
    destination: HostOutput,
}

struct SourceDocument {
    source: String,
}

fn operation_context(explicit: Option<&Path>, host: &HostOptions) -> Result<Value, Error> {
    if let Some(path) = explicit {
        return super::read_json(path, MAX_INPUT_BYTES);
    }
    let Some(path) = &host.tools else {
        return Ok(json!({}));
    };
    let bytes = engarg::bounded_file::read(path, 1_048_576, "tools manifest")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::InvalidInput("tools manifest requires UTF-8".into()))?;
    let manifest =
        eal_collector_contracts::RegistryDocument::from_toml(text).map_err(Error::InvalidInput)?;
    manifest.validate().map_err(Error::InvalidInput)?;
    serde_json::to_value(manifest.context).map_err(Error::from)
}

fn read_text(path: &Path) -> Result<String, Error> {
    let bytes = engarg::bounded_file::read(path, MAX_INPUT_BYTES, "EAL input")?;
    String::from_utf8(bytes).map_err(|_| Error::InvalidInput("EAL input must be UTF-8".into()))
}

fn source_document(args: &SourceInput) -> Result<SourceDocument, Error> {
    let source = read_text(&args.input)?;
    if matches!(args.input_format, InputFormat::Json)
        || (matches!(args.input_format, InputFormat::Auto) && source.trim_start().starts_with('{'))
    {
        return Err(Error::InvalidInput("JSON export bundles are retired. Run engarg build with the canonical report and execute the resulting case.eal".into()));
    }
    Ok(SourceDocument { source })
}

fn require_text(label: &str, text: &str) -> Result<(), Error> {
    if text.trim().is_empty() {
        return Err(Error::InvalidInput(format!("{label} must be nonempty")));
    }
    Ok(())
}

fn mapped_claim(_document: &SourceDocument, requested: &str) -> Result<String, Error> {
    require_text("claim", requested)?;
    Ok(requested.into())
}

/// Canonicalising the existing parent detects relative and symlink aliases before
/// creating any outputs. Exclusive creation remains the final race-safe guard.
fn new_path(path: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(path)?;
    if std::fs::symlink_metadata(&absolute).is_ok() {
        return Err(Error::InvalidInput("output file already exists".into()));
    }
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::InvalidInput("output requires a file name".into()))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::InvalidInput("output requires an existing parent directory".into()))?
        .canonicalize()?;
    Ok(parent.join(name))
}

struct PendingFile {
    path: PathBuf,
    file: Option<File>,
    keep: bool,
}

impl PendingFile {
    fn create(path: PathBuf) -> Result<Self, Error> {
        let file = super::private_file(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            keep: false,
        })
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| Error::InvalidInput("output file is already closed".into()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        // Windows also requires closing the handle before removing a failed file.
        drop(self.file.take());
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn stdout(bytes: &[u8]) -> Result<(), Error> {
    let out = std::io::stdout();
    out.lock().write_all(bytes)?;
    Ok(())
}

fn json_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn execute_export(args: ExportArgs) -> Result<(), Error> {
    let raw: Value = super::read_json(&args.input, MAX_INPUT_BYTES)?;
    let report = report::read_direct(&raw)?;
    let policy = match args.export_policy.as_deref() {
        Some(path) => super::read_json(path, 1_048_576)?,
        None => Default::default(),
    };
    let receipt = engarg::package::build(
        &report.request,
        &report.argument,
        report
            .support_elaboration
            .as_ref()
            .expect("validated direct report"),
        &args.tools,
        &args.output,
        &policy,
    )?;
    stdout(&json_bytes(&receipt)?)
}

async fn execute_replan(args: ReplanArgs) -> Result<(), Error> {
    let raw: Value = super::read_json(&args.input, MAX_INPUT_BYTES)?;
    let EvidencePlanningReport::Graph(previous) = report::classify_evidence_planning(&raw)? else {
        return Err(Error::InvalidInput(
            "eal replan requires a saved graph analysis with unchanged support requirements".into(),
        ));
    };
    let executable = std::env::current_exe()?;
    report::validate_replanning_for_update(&raw, &executable)?;
    engarg::tooling::direct::revalidate_report(&raw, &executable)?;
    let canonical = report::read_direct(&raw)?;
    engarg::domain::validate_request(&canonical.request)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    engarg::domain::validate_argument(&canonical.request, &canonical.argument).map_err(
        |errors| {
            Error::InvalidInput(
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        },
    )?;
    let clarification: Option<report::freshness::FreshnessClarification> = args
        .freshness_clarifications
        .as_ref()
        .map(|path| super::read_json(path, MAX_INPUT_BYTES))
        .transpose()?;
    let mut elaboration = previous.elaboration.clone();
    if let Some(clarification) = &clarification {
        report::freshness::apply(&mut elaboration, clarification)?;
    }
    // Historical identities were admitted above. Validate current collection
    // requirements after applying only the explicitly authorised missing ages.
    let mut errors =
        engarg::support_requirements::validate_elaboration(&canonical.argument, &elaboration);
    errors.extend(engarg::support_requirements::validate_freshness(
        &elaboration,
    ));
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    let mut output = PendingFile::create(new_path(&args.output)?)?;
    let planner = super::support_commands::admit(&args.evidence_config).await?;
    let routes = planner.resolve(&canonical.argument, &elaboration).await?;
    let mut updated = raw.clone();
    updated["evidence_planning"]["elaboration"] = serde_json::to_value(&elaboration)?;
    updated["evidence_planning"]["routes"] = serde_json::to_value(routes)?;
    if raw.get("support_elaboration").is_some() {
        updated["support_elaboration"] = serde_json::to_value(&elaboration)?;
        updated["direct_bindings"] =
            serde_json::to_value(engarg::direct_planning::DirectPlan::unbound(&elaboration))?;
    }
    // Manually authored plans can omit provider history. Preserve that absence
    // rather than attaching a history-based review without its source records.
    if raw.get("transcript").is_some() {
        let review = engarg::support_requirements::review_acquisition_design(
            &elaboration,
            &canonical.transcript,
            MAX_INPUT_BYTES,
        )?;
        if let Some(warning) = review.warning() {
            eprintln!("engarg: warning: {warning}");
        }
        updated["evidence_planning"]["design_review"] = serde_json::to_value(&review)?;
        if raw.get("design_review").is_some() {
            updated["design_review"] = serde_json::to_value(review)?;
        }
    }
    report::retain_replanning_with_freshness(&raw, &mut updated, clarification)?;
    report::validate_replanning(&updated, &executable)?;
    let bytes = json_bytes(&updated)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(Error::Budget(
            "replanned report exceeds its 16 MiB retained-input allowance".into(),
        ));
    }
    output.write(&bytes)?;
    output.keep = true;
    Ok(())
}

fn semantic_failure(envelope: &Value) -> bool {
    if envelope.get("is_error") == Some(&Value::Bool(true))
        || envelope.get("result").and_then(|r| r.get("valid")) == Some(&Value::Bool(false))
    {
        return true;
    }
    if envelope.get("result").is_some_and(Value::is_null)
        && let Some(content) = envelope.get("content").and_then(Value::as_array)
    {
        return content
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .any(|value| value.get("valid") == Some(&Value::Bool(false)));
    }
    false
}

async fn host_operation(destination: HostOutput, request: Value) -> Result<(), Error> {
    let mut output = destination
        .output
        .as_deref()
        .map(new_path)
        .transpose()?
        .map(PendingFile::create)
        .transpose()?;
    let response = invoke(&destination.host.config(), &request).await?;
    let bytes = json_bytes(&response)?;
    if let Some(file) = &mut output {
        file.write(&bytes)?;
        file.keep = true;
    } else {
        stdout(&bytes)?;
    }
    if semantic_failure(&response) {
        let operation = request["operation"].as_str().unwrap_or("operation");
        let location = output.as_ref().map_or_else(
            || "written to stdout".to_owned(),
            |file| format!("saved to {}", file.path.display()),
        );
        let hint = if response
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| {
                error.contains("methods must have the form package.module:function")
            }) {
            "; incompatible host/server method loader: select matching Rust EAL binaries supporting --methods builtin"
        } else {
            ""
        };
        return Err(Error::Provider(format!(
            "EAL {operation} failed; complete JSON response {location}{hint}"
        )));
    }
    Ok(())
}

pub async fn execute(command: EalCommand) -> Result<(), Error> {
    match command {
        EalCommand::Replan(args) => execute_replan(args).await,
        EalCommand::FreshnessCheck(args) => {
            let raw: Value = super::read_json(&args.input, MAX_INPUT_BYTES)?;
            let EvidencePlanningReport::Graph(planning) = report::classify_evidence_planning(&raw)?
            else {
                return Err(Error::InvalidInput(
                    "freshness check requires a saved graph analysis".into(),
                ));
            };
            report::validate_replanning(&raw, &std::env::current_exe()?)?;
            let missing = engarg::support_requirements::validate_freshness(&planning.elaboration);
            if !missing.is_empty() {
                return Err(Error::InvalidInput(format!(
                    "SUPPORT_FRESHNESS_CLARIFICATION: explicit positive freshness bounds required for: {}",
                    missing.join(", ")
                )));
            }
            println!("Graph freshness contracts are present.");
            Ok(())
        }
        EalCommand::Repair(args) => repair::execute(args).await,
        EalCommand::Revise(args) => super::package_commands::revise(args),
        EalCommand::Implement(args) => super::tooling_commands::implement(args).await,
        EalCommand::ToolchainCheck(args) => super::tooling_commands::preflight::check(args),
        EalCommand::ToolReplay(args) => super::tooling_commands::replay::replay(args).await,
        EalCommand::ToolRun(args) => super::tooling_commands::tool_run(args).await,
        EalCommand::ToolExport(args) => super::tooling_commands::tool_export(args),
        EalCommand::Export(args) => execute_export(args),
        EalCommand::ExportAspic(args) => aspic::execute(args).await,
        EalCommand::Validate(args) => {
            let document = source_document(&args.source)?;
            host_operation(
                args.destination,
                json!({"operation":"validate","source":document.source}),
            )
            .await
        }
        EalCommand::Plan(args) => {
            let document = source_document(&args.source)?;
            let claim = mapped_claim(&document, &args.claim)?;
            host_operation(
                args.destination,
                json!({"operation":"plan","source":document.source,"claim":claim}),
            )
            .await
        }
        EalCommand::Collect(args) => {
            if args.destination.host.tools.is_none() && args.destination.host.url.is_none() {
                return Err(Error::InvalidInput(
                    "collection requires --tools or an explicitly configured remote --url".into(),
                ));
            }
            let document = source_document(&args.source)?;
            let claim = mapped_claim(&document, &args.claim)?;
            let context = operation_context(args.context.as_deref(), &args.destination.host)?;
            if !context.is_object() {
                return Err(Error::InvalidInput(
                    "collection context must be a JSON object".into(),
                ));
            }
            host_operation(args.destination, json!({"operation":"collect_claim","source":document.source,"claim":claim,"context":context})).await
        }
        EalCommand::Assess(args) => {
            require_text("now", &args.now)?;
            if let Some(id) = &args.collection_id {
                require_text("collection-id", id)?;
            }
            let document = source_document(&args.source)?;
            let context = operation_context(args.context.as_deref(), &args.destination.host)?;
            if !context.is_object() {
                return Err(Error::InvalidInput(
                    "assessment context must be a JSON object".into(),
                ));
            }
            let mut request = json!({"operation":"reason","source":document.source,"context":context,"now":args.now});
            if let Some(id) = args.collection_id {
                request["collection_id"] = json!(id);
            }
            host_operation(args.destination, request).await
        }
        EalCommand::Explain(args) => {
            require_text("assessment-id", &args.assessment_id)?;
            if let Some(claim) = &args.claim {
                require_text("claim", claim)?;
            }
            let mut request = json!({"operation":"explain","assessment_id":args.assessment_id});
            if let Some(claim) = args.claim {
                request["claim"] = json!(claim);
            }
            host_operation(args.destination, request).await
        }
        EalCommand::Packet(args) => {
            require_text("assessment-id", &args.assessment_id)?;
            if let Some(claim) = &args.claim {
                require_text("claim", claim)?;
            }
            let mut request = json!({"operation":"packet","assessment_id":args.assessment_id});
            if let Some(claim) = args.claim {
                request["claim"] = json!(claim);
            }
            if args.max_bytes.is_some() || args.max_statement_bytes.is_some() {
                let mut limits = json!({});
                if let Some(value) = args.max_bytes {
                    limits["max_bytes"] = json!(value);
                }
                if let Some(value) = args.max_statement_bytes {
                    limits["max_statement_bytes"] = json!(value);
                }
                request["packet_limits"] = limits;
            }
            host_operation(args.destination, request).await
        }
        EalCommand::Describe(args) => host_operation(args, json!({"operation":"describe"})).await,
        EalCommand::Observe(args) => {
            let mut bytes = Vec::new();
            std::io::stdin().take(1_048_577).read_to_end(&mut bytes)?;
            if bytes.len() > 1_048_576 {
                return Err(Error::Budget("collector input exceeds 1 MiB".into()));
            }
            let request = engarg::eal_observe::parse_request(&bytes)?;
            let response = if args.tool == "review_json" {
                let inputs = args
                    .inputs
                    .ok_or_else(|| Error::InvalidInput("review_json requires --inputs".into()))?;
                engarg::retained_collectors::observe(&args.tool, request, &inputs)?
            } else {
                if args.inputs.is_some() {
                    return Err(Error::InvalidInput("--inputs is only supported with review_json; acquisition inputs are bound in the EAL request".into()));
                }
                engarg::eal_observe::observe(&args.tool, request).await?
            };
            stdout(&json_bytes(&response)?)
        }
    }
}
