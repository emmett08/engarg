//! CLI boundary for deterministic EAL export and the official one-request host.
//!
//! These commands never call a language model or start evidence collectors.

use clap::{Args, Subcommand, ValueEnum};
use engarg::{
    Error,
    domain::{ArgumentCase, Request},
    eal_export::{EalExport, export},
    eal_host::{HostConfig, invoke},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_INPUT_BYTES: usize = 16 * 1_048_576;

#[derive(Subcommand)]
pub enum EalCommand {
    /// Export a saved canonical report as EAL/3 source or a loss-preserving bundle.
    Export(ExportArgs),
    /// Check EAL source using the official host; no evidence is collected.
    Validate(SourceCommand),
    /// Plan evidence requirements for an EAL claim; no evidence is collected.
    Plan(PlanArgs),
    /// Assess with existing observations through the host's reason operation.
    Assess(AssessArgs),
    /// Recover a stored assessment or one claim's explanation.
    Explain(StoredArgs),
    /// Recover a bounded packet from a stored assessment.
    Packet(PacketArgs),
    /// Describe the language and methods exposed by the configured EAL server.
    Describe(HostOutput),
}

#[derive(Args)]
pub struct ExportArgs {
    /// Saved EngArg report containing request and argument.
    #[arg(long)]
    input: PathBuf,
    /// New source or bundle file. Omit for stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Optional new JSON bundle containing source, mapping, context and obligations.
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "eal")]
    format: ExportFormat,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    Eal,
    Json,
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

#[derive(Deserialize)]
struct CanonicalReport {
    request: Request,
    argument: ArgumentCase,
}

struct SourceDocument {
    source: String,
    bundle: Option<EalExport>,
}

fn read_text(path: &Path) -> Result<String, Error> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(Error::Budget("EAL input exceeds 16 MiB".into()));
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidInput("EAL input must be UTF-8".into()))
}

fn source_document(args: &SourceInput) -> Result<SourceDocument, Error> {
    let text = read_text(&args.input)?;
    let bundle = matches!(args.input_format, InputFormat::Json)
        || (matches!(args.input_format, InputFormat::Auto) && text.trim_start().starts_with('{'));
    if !bundle {
        return Ok(SourceDocument {
            source: text,
            bundle: None,
        });
    }
    let bundle: EalExport = serde_json::from_str(&text).map_err(|_| {
        Error::InvalidInput(
            "input is not a valid EAL export bundle; use --input-format eal for raw source".into(),
        )
    })?;
    // This checks deterministic identity, not authenticity or empirical truth.
    let expected = export(&bundle.original_request, &bundle.original_case)?;
    if serde_json::to_value(&bundle)? != serde_json::to_value(&expected)? {
        return Err(Error::InvalidInput(
            "EAL bundle differs from the current deterministic export; re-export the canonical case, or assess edited text with --input-format eal".into(),
        ));
    }
    Ok(SourceDocument {
        source: bundle.source.clone(),
        bundle: Some(bundle),
    })
}

fn require_text(label: &str, text: &str) -> Result<(), Error> {
    if text.trim().is_empty() {
        return Err(Error::InvalidInput(format!("{label} must be nonempty")));
    }
    Ok(())
}

fn mapped_claim(document: &SourceDocument, requested: &str) -> Result<String, Error> {
    require_text("claim", requested)?;
    match &document.bundle {
        None => Ok(requested.into()),
        Some(bundle) => {
            if let Some(mapped) = bundle.claim_names.get(requested) {
                Ok(mapped.clone())
            } else if bundle.claim_names.values().any(|name| name == requested) {
                Ok(requested.into())
            } else {
                Err(Error::InvalidInput(
                    "claim is absent from the export bundle's canonical mapping".into(),
                ))
            }
        }
    }
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
    let output = args.output.as_deref().map(new_path).transpose()?;
    let manifest = args.manifest.as_deref().map(new_path).transpose()?;
    if matches!((&output, &manifest), (Some(a), Some(b)) if a == b) {
        return Err(Error::InvalidInput(
            "output and manifest require different paths".into(),
        ));
    }
    let report: CanonicalReport = super::read_json(&args.input, MAX_INPUT_BYTES)?;
    let bundle = export(&report.request, &report.argument)?;
    let manifest_bytes = json_bytes(&bundle)?;
    let primary = match args.format {
        ExportFormat::Eal => bundle.source.as_bytes(),
        ExportFormat::Json => &manifest_bytes,
    };
    // Reserve every destination before writing, so a partial create is cleaned up.
    let mut files = Vec::new();
    if let Some(path) = output {
        files.push((PendingFile::create(path)?, primary));
    }
    if let Some(path) = manifest {
        files.push((PendingFile::create(path)?, manifest_bytes.as_slice()));
    }
    for (file, bytes) in &mut files {
        file.write(bytes)?;
    }
    for (file, _) in &mut files {
        file.keep = true;
    }
    if args.output.is_none() {
        stdout(primary)?;
    }
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
        return Err(Error::Provider(
            "EAL host reported failure; the complete JSON response has been emitted".into(),
        ));
    }
    Ok(())
}

pub async fn execute(command: EalCommand) -> Result<(), Error> {
    match command {
        EalCommand::Export(args) => execute_export(args),
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
        EalCommand::Assess(args) => {
            require_text("now", &args.now)?;
            if let Some(id) = &args.collection_id {
                require_text("collection-id", id)?;
            }
            let document = source_document(&args.source)?;
            let context = match args.context {
                Some(path) => super::read_json::<Value>(&path, MAX_INPUT_BYTES)?,
                None => document
                    .bundle
                    .as_ref()
                    .map_or_else(|| json!({}), |b| b.context.clone()),
            };
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
    }
}
