//! CLI boundary for deterministic EAL export and the official one-request host.
//!
//! Export and inspection never collect evidence. Collection is an explicit command.

use clap::{Args, Subcommand, ValueEnum};
use engarg::{
    Error,
    acquisition::{self, AcquisitionPlan},
    domain::{ArgumentCase, Request},
    eal_acquisition::{export_with_policy, reconstruct_bundle},
    eal_export::EalExport,
    eal_host::{HostConfig, invoke},
    engine::{CallRecord, RouteResolver},
    provider::Role,
    report::{self, EvidencePlanningReport},
    support_export::{self, SupportExport, SupportExportLocations},
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
    /// Admit operator-qualified verified tools into a new connection configuration.
    RegisterTools(super::tooling_commands::registration::RegisterToolsArgs),
    /// Rebuild evidence routes from a saved graph report without model calls.
    Replan(ReplanArgs),
    /// Implement missing collectors from a saved analysis using frozen fixtures.
    Implement(super::tooling_commands::ImplementArgs),
    /// Internal launcher for a verified generated collector package.
    #[command(hide = true)]
    ToolRun(super::tooling_commands::ToolRunArgs),
    /// Prepare private collector inputs and executable bindings without collecting evidence.
    Prepare(PrepareArgs),
    /// Export a saved canonical report as EAL/3 source or a loss-preserving bundle.
    Export(ExportArgs),
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
    /// New report retaining the prior planning and tooling provenance.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Args)]
pub struct PrepareArgs {
    /// Saved canonical report containing request, argument and acquisition proposals.
    #[arg(long)]
    input: PathBuf,
    /// New private directory, normally under .engarg-local/ or engarg-runs/.
    #[arg(long)]
    output_dir: PathBuf,
    /// Current per-need preparation declarations overriding saved acquisition plans.
    #[arg(long)]
    acquisition_plan: Option<PathBuf>,
    #[command(flatten)]
    operator: OperatorInputsArgs,
    /// Operator-selected collector catalogue. Otherwise use the saved run's catalogue.
    #[arg(long)]
    collector_catalogue: Option<PathBuf>,
    /// JSON export freshness policy, resolved and retained in the v3 bundle.
    #[arg(long)]
    export_policy: Option<PathBuf>,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Saved EngArg report containing request and argument.
    #[arg(long)]
    input: PathBuf,
    /// New source or bundle file. Omit for stdout and companion files beside input.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Optional new JSON bundle containing source, mapping, context and obligations.
    #[arg(long)]
    manifest: Option<PathBuf>,
    /// JSON acquisition-plan entries overriding retained bindings by evidence need ID.
    #[arg(long)]
    acquisition_plan: Option<PathBuf>,
    #[command(flatten)]
    operator: OperatorInputsArgs,
    /// New private directory for current-case input files and the preparation report.
    #[arg(long, requires = "operator_inputs")]
    preparation_output_dir: Option<PathBuf>,
    /// Operator-selected collector catalogue. Otherwise use the saved run's catalogue.
    #[arg(long)]
    collector_catalogue: Option<PathBuf>,
    /// JSON export freshness policy, resolved and retained in the v3 bundle.
    #[arg(long)]
    export_policy: Option<PathBuf>,
    /// Companion tools registry. Defaults beside output, or beside input for stdout.
    #[arg(long)]
    tools_output: Option<PathBuf>,
    /// Companion EAL source for JSON output. Defaults to the output or input stem with .eal.
    #[arg(long)]
    source_output: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "eal")]
    format: ExportFormat,
}

#[derive(Args)]
pub struct OperatorInputsArgs {
    /// Operator scope and retained artifact references, read without collecting evidence.
    #[arg(long)]
    operator_inputs: Option<PathBuf>,
    /// Base for relative artifact paths when operator-inputs is an exact retained snapshot.
    #[arg(long, requires = "operator_inputs")]
    operator_input_base: Option<PathBuf>,
}

impl OperatorInputsArgs {
    fn load(&self) -> Result<Option<(Value, PathBuf)>, Error> {
        let Some(path) = &self.operator_inputs else {
            return Ok(None);
        };
        let value = super::read_json(path, 1_048_576)?;
        let base = match &self.operator_input_base {
            Some(base) => base.canonicalize()?,
            None => std::path::absolute(path)?
                .parent()
                .ok_or_else(|| Error::InvalidInput("operator inputs require a parent".into()))?
                .canonicalize()?,
        };
        if !base.is_dir() {
            return Err(Error::InvalidInput(
                "operator input base must be a directory".into(),
            ));
        }
        Ok(Some((value, base)))
    }
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

#[derive(Deserialize)]
struct CanonicalReport {
    request: Request,
    argument: ArgumentCase,
    #[serde(default)]
    transcript: Vec<CallRecord>,
    #[serde(default)]
    run_config: Option<engarg::engine::RunConfig>,
    #[serde(default)]
    evidence_planning: Option<engarg::engine::SupportPlanning<engarg::route_planning::RoutePlan>>,
}

struct SourceDocument {
    source: String,
    bundle: Option<EalExport>,
    support: Option<SupportExport>,
}

impl SourceDocument {
    fn context(&self) -> Value {
        self.support
            .as_ref()
            .map(|bundle| bundle.context.clone())
            .or_else(|| self.bundle.as_ref().map(|bundle| bundle.context.clone()))
            .unwrap_or_else(|| json!({}))
    }
}

fn read_text(path: &Path) -> Result<String, Error> {
    let bytes = engarg::bounded_file::read(path, MAX_INPUT_BYTES, "EAL input")?;
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
            support: None,
        });
    }
    let value = eal_collector_contracts::strict_json_exact(
        text.as_bytes(),
        eal_collector_contracts::JsonLimits {
            max_bytes: MAX_INPUT_BYTES,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    if value["schema_version"]
        .as_str()
        .is_some_and(|schema| schema.starts_with("engarg.support-export/"))
    {
        let bundle: SupportExport = serde_json::from_value(value)?;
        let rebuilt = support_export::reconstruct_bundle(&bundle)?;
        return Ok(SourceDocument {
            source: rebuilt.source.clone(),
            bundle: None,
            support: Some(rebuilt),
        });
    }
    let bundle: EalExport = serde_json::from_value(value).map_err(|_| {
        Error::InvalidInput(
            "input is not a valid EAL export bundle; use --input-format eal for raw source".into(),
        )
    })?;
    // This checks deterministic identity, not authenticity or empirical truth.
    let expected = reconstruct_bundle(&bundle)?;
    if serde_json::to_value(&bundle)? != serde_json::to_value(&expected)? {
        return Err(Error::InvalidInput(
            "EAL bundle differs from the current deterministic export; re-export the canonical case, or assess edited text with --input-format eal".into(),
        ));
    }
    Ok(SourceDocument {
        source: bundle.source.clone(),
        bundle: Some(bundle),
        support: None,
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
    if let Some(bundle) = &document.support {
        return bundle
            .claim_names
            .get(requested)
            .cloned()
            .or_else(|| {
                bundle
                    .claim_names
                    .values()
                    .any(|name| name == requested)
                    .then(|| requested.into())
            })
            .ok_or_else(|| {
                Error::InvalidInput(
                    "claim is absent from the support bundle's canonical mapping".into(),
                )
            });
    }
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

/// Own only a newly created private directory, removing incomplete preparation.
struct PreparationDirectory {
    path: PathBuf,
    keep: bool,
}

impl PreparationDirectory {
    fn create(path: &Path) -> Result<Self, Error> {
        let path = new_path(path)?;
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self { path, keep: false })
    }
}

impl Drop for PreparationDirectory {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
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

fn selected_catalogue(
    path: Option<&Path>,
    report: &CanonicalReport,
) -> Result<Option<engarg::collector_catalogue::CollectorCatalogue>, Error> {
    if let Some(path) = path {
        let bytes = engarg::bounded_file::read(path, 1_048_576, "collector catalogue")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::InvalidInput("collector catalogue must be UTF-8".into()))?;
        return Ok(Some(
            engarg::collector_catalogue::CollectorCatalogue::from_json(text.as_bytes())?,
        ));
    }
    Ok(report
        .run_config
        .as_ref()
        .and_then(|config| config.collector_catalogue.clone()))
}

fn retained_plan(report: &CanonicalReport) -> Result<Vec<AcquisitionPlan>, Error> {
    let Some(record) = report
        .transcript
        .iter()
        .rev()
        .find(|record| matches!(record.role, Role::Single | Role::Adjudicator))
    else {
        return Ok(Vec::new());
    };
    let response: Value = serde_json::from_str(&record.response.content)?;
    if response.get("acquisition_plan").is_none() && response.get("argument").is_none() {
        return Ok(Vec::new());
    }
    let planned = acquisition::parse_completion(&record.response.content)?;
    if serde_json::to_value(&planned.argument)? != serde_json::to_value(&report.argument)? {
        return Err(Error::InvalidInput(
            "retained acquisition plan belongs to a different canonical argument".into(),
        ));
    }
    let errors = acquisition::validate_with_catalogue(
        &report.argument,
        &planned.acquisition_plan,
        report
            .run_config
            .as_ref()
            .and_then(|config| config.collector_catalogue.as_ref()),
    );
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    Ok(planned.acquisition_plan)
}

fn merge_catalogues(
    left: Option<engarg::collector_catalogue::CollectorCatalogue>,
    right: engarg::collector_catalogue::CollectorCatalogue,
) -> Result<engarg::collector_catalogue::CollectorCatalogue, Error> {
    let mut descriptors = std::collections::BTreeMap::<String, Value>::new();
    for catalogue in left.into_iter().chain(std::iter::once(right)) {
        for descriptor in catalogue.collectors.iter() {
            let value = serde_json::to_value(descriptor)?;
            if let Some(previous) = descriptors.insert(descriptor.id.clone(), value.clone())
                && previous != value
            {
                return Err(Error::InvalidInput(format!(
                    "conflicting collector descriptor {}",
                    descriptor.id,
                )));
            }
        }
    }
    engarg::collector_catalogue::CollectorCatalogue::from_json(&serde_json::to_vec(&json!({
        "schema": engarg::collector_catalogue::SCHEMA,
        "acquisition_contract": "eal.acquisition/1",
        "collectors": descriptors.into_values().collect::<Vec<_>>(),
    }))?)
}

/// Apply only registrations whose retained source and observed test receipt
/// still verify. Source, context and TOML are then projected in one normal pass.
fn export_inputs(
    input: &Path,
    selected: Option<&Path>,
) -> Result<
    (
        CanonicalReport,
        Vec<AcquisitionPlan>,
        Option<engarg::collector_catalogue::CollectorCatalogue>,
    ),
    Error,
> {
    let raw: Value = super::read_json(input, MAX_INPUT_BYTES)?;
    report::validate_replanning(&raw, &std::env::current_exe()?)?;
    if matches!(
        report::classify_evidence_planning(&raw)?,
        EvidencePlanningReport::Graph(_)
    ) {
        engarg::tooling::capability::revalidate_report(&raw, &std::env::current_exe()?)?;
        let report: CanonicalReport = serde_json::from_value(raw)?;
        return Ok((report, Vec::new(), None));
    }
    let registration = engarg::tooling::registration(&raw)?;
    let canonical = if raw.get("request").is_some() {
        raw.clone()
    } else if registration.is_some()
        && raw.get("original_request").is_some()
        && raw.get("original_case").is_some()
    {
        // A saved EAL bundle has no argument-call transcript to fabricate.
        json!({
            "request": raw["original_request"],
            "argument": raw["original_case"],
            "transcript": [],
            "run_config": null,
        })
    } else {
        raw.clone()
    };
    let report: CanonicalReport = serde_json::from_value(canonical)?;
    let catalogue = selected_catalogue(selected, &report)?;
    match registration {
        None => {
            let plan = retained_plan(&report)?;
            Ok((report, plan, catalogue))
        }
        Some(registration) => {
            let catalogue = merge_catalogues(catalogue, registration.catalogue)?;
            let plan = acquisition::override_plans_with_catalogue(
                &report.argument,
                &registration.original_plans,
                registration.overrides,
                Some(&catalogue),
            )?;
            Ok((report, plan, Some(catalogue)))
        }
    }
}

enum ExportArtifact {
    Primary,
    Manifest,
    Tools,
    Source,
}

fn export_destinations(args: &ExportArgs) -> Result<Vec<(PathBuf, ExportArtifact)>, Error> {
    if matches!(args.format, ExportFormat::Eal) && args.source_output.is_some() {
        return Err(Error::InvalidInput(
            "source-output is only used with --format json".into(),
        ));
    }
    let stem = args.output.as_ref().unwrap_or(&args.input);
    let tools_path = args
        .tools_output
        .clone()
        .unwrap_or_else(|| stem.with_extension("tools.toml"));
    let source_path =
        (matches!(args.format, ExportFormat::Json) || args.output.is_none()).then(|| {
            args.source_output
                .clone()
                .unwrap_or_else(|| stem.with_extension("eal"))
        });
    let destinations = [
        args.output
            .as_deref()
            .map(|path| (path, ExportArtifact::Primary)),
        args.manifest
            .as_deref()
            .map(|path| (path, ExportArtifact::Manifest)),
        Some((tools_path.as_path(), ExportArtifact::Tools)),
        source_path
            .as_deref()
            .map(|path| (path, ExportArtifact::Source)),
    ]
    .into_iter()
    .flatten()
    .map(|(path, artifact)| new_path(path).map(|path| (path, artifact)))
    .collect::<Result<Vec<_>, _>>()?;
    let mut unique = std::collections::BTreeSet::new();
    if destinations.iter().any(|(path, _)| !unique.insert(path)) {
        return Err(Error::InvalidInput(
            "all export outputs require different paths".into(),
        ));
    }
    Ok(destinations)
}

fn execute_export(args: ExportArgs) -> Result<(), Error> {
    if args.operator.operator_inputs.is_some() != args.preparation_output_dir.is_some() {
        return Err(Error::InvalidInput(
            "export requires both --operator-inputs and --preparation-output-dir for current-case preparation".into(),
        ));
    }
    let destinations = export_destinations(&args)?;
    let (report, mut plan, catalogue) =
        export_inputs(&args.input, args.collector_catalogue.as_deref())?;
    if report.evidence_planning.is_some() {
        return execute_support_export(args, destinations, &report);
    }
    if let Some(path) = args.acquisition_plan {
        let overrides: Vec<AcquisitionPlan> = super::read_json(&path, MAX_INPUT_BYTES)?;
        plan = acquisition::override_plans_with_catalogue(
            &report.argument,
            &plan,
            overrides,
            catalogue.as_ref(),
        )?;
    }
    let policy = match args.export_policy.as_deref() {
        Some(path) => super::read_json(path, 1_048_576)?,
        None => engarg::export_policy::ExportPolicy::default(),
    };
    let operator = args.operator.load()?;
    let mut preparation = args
        .preparation_output_dir
        .as_deref()
        .map(PreparationDirectory::create)
        .transpose()?;
    let (bundle, tools_toml, ancillary) =
        if let (Some((inputs, base)), Some(directory)) = (&operator, &preparation) {
            let mut prepared = engarg::eal_prepare::prepare_with_operator_inputs(
                &report.request,
                &report.argument,
                &plan,
                &directory.path,
                &std::env::current_exe()?,
                catalogue.as_ref(),
                &policy,
                Some((inputs, base)),
            )?;
            let bundle: EalExport = serde_json::from_slice(
                &prepared
                    .remove("argument.bundle.json")
                    .expect("prepared bundle"),
            )?;
            prepared.remove("argument.eal");
            let tools = String::from_utf8(
                prepared
                    .remove("argument.tools.toml")
                    .expect("prepared registry"),
            )
            .map_err(|_| Error::InvalidOutput("prepared registry requires UTF-8".into()))?;
            (bundle, tools, prepared)
        } else {
            let artifacts = export_with_policy(
                &report.request,
                &report.argument,
                &plan,
                catalogue.as_ref(),
                &policy,
            )?;
            (artifacts.bundle, artifacts.tools_toml, Default::default())
        };
    let manifest_bytes = json_bytes(&bundle)?;
    let primary = match args.format {
        ExportFormat::Eal => bundle.source.as_bytes(),
        ExportFormat::Json => &manifest_bytes,
    };
    // Reserve every destination before writing, so a partial create is cleaned up.
    let mut outputs = destinations
        .into_iter()
        .map(|(path, artifact)| {
            let bytes = match artifact {
                ExportArtifact::Primary => primary,
                ExportArtifact::Manifest => manifest_bytes.as_slice(),
                ExportArtifact::Tools => tools_toml.as_bytes(),
                ExportArtifact::Source => bundle.source.as_bytes(),
            };
            (path, bytes.to_vec())
        })
        .collect::<Vec<_>>();
    if let Some(directory) = &preparation {
        for (name, bytes) in ancillary {
            if Path::new(&name).components().count() != 1
                || !matches!(
                    Path::new(&name).components().next(),
                    Some(std::path::Component::Normal(_))
                )
            {
                return Err(Error::InvalidOutput("invalid prepared file name".into()));
            }
            outputs.push((directory.path.join(name), bytes));
        }
    }
    let mut files = outputs
        .into_iter()
        .map(|(path, bytes)| PendingFile::create(path).map(|file| (file, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    for (file, bytes) in &mut files {
        file.write(bytes)?;
    }
    if args.output.is_none() {
        stdout(primary)?;
    }
    for (file, _) in &mut files {
        file.keep = true;
    }
    if let Some(directory) = &mut preparation {
        directory.keep = true;
    }
    Ok(())
}

fn support_artifacts(
    report: &CanonicalReport,
    execution_path: &Path,
    review_binding: Option<eal_collector_contracts::CommandBinding>,
    policy: &engarg::export_policy::ExportPolicy,
) -> Result<engarg::support_export::SupportExportArtifacts, Error> {
    let planning = report
        .evidence_planning
        .as_ref()
        .expect("support path selected");
    if planning.schema != "engarg.support-planning/1"
        || planning.routes.schema != engarg::route_planning::ROUTE_PLAN_SCHEMA
    {
        return Err(Error::InvalidInput(
            "unsupported saved support planning schema".into(),
        ));
    }
    let configuration = &planning.routes.configuration;
    configuration.validate()?;
    support_export::export(
        &report.request,
        &report.argument,
        &planning.elaboration,
        &planning.routes.execution,
        &planning.routes.projections,
        &SupportExportLocations {
            collector: configuration.collector.clone(),
            execution_plan: execution_path.into(),
            connections: configuration.connections.clone(),
            store: configuration.store.clone(),
            environment: configuration.environment.clone(),
            review_binding,
        },
        policy,
    )
}

async fn execute_replan(args: ReplanArgs) -> Result<(), Error> {
    let raw: Value = super::read_json(&args.input, MAX_INPUT_BYTES)?;
    let EvidencePlanningReport::Graph(previous) = report::classify_evidence_planning(&raw)? else {
        return Err(Error::InvalidInput(
            "eal replan requires a saved graph analysis with unchanged support requirements".into(),
        ));
    };
    let executable = std::env::current_exe()?;
    report::validate_replanning(&raw, &executable)?;
    engarg::tooling::capability::revalidate_report(&raw, &executable)?;
    let canonical: CanonicalReport = serde_json::from_value(raw.clone())?;
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
    let errors = engarg::support_requirements::validate_elaboration(
        &canonical.argument,
        &previous.elaboration,
    );
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    let mut output = PendingFile::create(new_path(&args.output)?)?;
    let planner = super::support_commands::admit(&args.evidence_config).await?;
    let routes = planner
        .resolve(&canonical.argument, &previous.elaboration)
        .await?;
    let mut updated = raw.clone();
    updated["evidence_planning"]["routes"] = serde_json::to_value(routes)?;
    report::retain_replanning(&raw, &mut updated)?;
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

fn support_evidence_plan(
    report: &CanonicalReport,
    artifacts: &engarg::support_export::SupportExportArtifacts,
) -> Value {
    json!({"schema":"engarg.support-evidence-plan/1","case_digest":artifacts.bundle.authored.revision,
        "execution_plan_sha256":artifacts.bundle.execution_plan_sha256,"evidence_needs":report.argument.evidence_plan,
        "support_elaboration":artifacts.bundle.support_elaboration,"routes":report.evidence_planning.as_ref().expect("support path selected").routes,
        "pending":artifacts.bundle.pending,"independent_review_obligations":artifacts.bundle.authored.obligations,
        "alternative_projection":artifacts.bundle.alternative_projection,
        "collection_budget":engarg::support_export::collection_budget(&artifacts.bundle)})
}

fn execute_support_export(
    args: ExportArgs,
    destinations: Vec<(PathBuf, ExportArtifact)>,
    report: &CanonicalReport,
) -> Result<(), Error> {
    if args.acquisition_plan.is_some()
        || args.collector_catalogue.is_some()
        || args.operator.operator_inputs.is_some()
        || args.preparation_output_dir.is_some()
    {
        return Err(Error::InvalidInput("support graphs retain qualified routes and operator configuration; flat acquisition plans, catalogues and operator-inputs cannot override them".into()));
    }
    let stem = args.output.as_ref().unwrap_or(&args.input);
    let execution_path = new_path(&stem.with_extension("execution-plan.json"))?;
    let evidence_path = new_path(&stem.with_extension("evidence-plan.json"))?;
    let acquisition_requests_path = new_path(&stem.with_extension("acquisition-requests.json"))?;
    let review_requests_path = new_path(&stem.with_extension("review-requests.json"))?;
    let policy = match args.export_policy.as_deref() {
        Some(path) => super::read_json(path, 1_048_576)?,
        None => Default::default(),
    };
    let artifacts = support_artifacts(report, &execution_path, None, &policy)?;
    let manifest = json_bytes(&artifacts.bundle)?;
    let primary = match args.format {
        ExportFormat::Eal => artifacts.bundle.source.as_bytes(),
        ExportFormat::Json => &manifest,
    };
    let mut outputs = destinations
        .into_iter()
        .map(|(path, kind)| {
            let bytes = match kind {
                ExportArtifact::Primary => primary,
                ExportArtifact::Manifest => manifest.as_slice(),
                ExportArtifact::Tools => artifacts.tools_toml.as_bytes(),
                ExportArtifact::Source => artifacts.bundle.source.as_bytes(),
            };
            (path, bytes.to_vec())
        })
        .collect::<Vec<_>>();
    outputs.extend([
        (execution_path, json_bytes(&artifacts.execution_plan)?),
        (
            evidence_path,
            json_bytes(&support_evidence_plan(report, &artifacts))?,
        ),
        (
            acquisition_requests_path,
            json_bytes(&artifacts.acquisition_requests)?,
        ),
        (
            review_requests_path,
            json_bytes(&artifacts.review_requests)?,
        ),
    ]);
    let mut unique = std::collections::BTreeSet::new();
    if outputs.iter().any(|(path, _)| !unique.insert(path.clone())) {
        return Err(Error::InvalidInput(
            "all support export outputs require different paths".into(),
        ));
    }
    let mut files = outputs
        .into_iter()
        .map(|(path, bytes)| PendingFile::create(path).map(|file| (file, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    for (file, bytes) in &mut files {
        file.write(bytes)?;
    }
    if args.output.is_none() {
        stdout(primary)?;
    }
    for (file, _) in &mut files {
        file.keep = true;
    }
    Ok(())
}

fn prepare_support_files(
    report: &CanonicalReport,
    directory: &Path,
    policy: &engarg::export_policy::ExportPolicy,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, Error> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let review_inputs = directory.join("review-inputs.json");
    let text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::InvalidInput("support preparation paths require UTF-8".into()))
    };
    let review_binding = eal_collector_contracts::CommandBinding {
        kind: eal_collector_contracts::ToolKind::Command,
        version: "1".into(),
        argv: vec![
            text(&executable)?,
            "eal".into(),
            "observe".into(),
            "--tool".into(),
            "review_json".into(),
            "--inputs".into(),
            text(&review_inputs)?,
        ],
        timeout_seconds: 60,
        max_output_bytes: 1_048_576,
        inherit_env: Vec::new(),
        parallel_safe: false,
    };
    let artifacts = support_artifacts(
        report,
        &directory.join("argument.execution-plan.json"),
        Some(review_binding),
        policy,
    )?;
    Ok(std::collections::BTreeMap::from([
        (
            "argument.eal".into(),
            artifacts.bundle.source.as_bytes().to_vec(),
        ),
        (
            "argument.bundle.json".into(),
            json_bytes(&artifacts.bundle)?,
        ),
        (
            "argument.tools.toml".into(),
            artifacts.tools_toml.as_bytes().to_vec(),
        ),
        (
            "argument.execution-plan.json".into(),
            json_bytes(&artifacts.execution_plan)?,
        ),
        (
            "evidence-plan.json".into(),
            json_bytes(&support_evidence_plan(report, &artifacts))?,
        ),
        (
            "acquisition-requests.json".into(),
            json_bytes(&artifacts.acquisition_requests)?,
        ),
        (
            "review-requests.json".into(),
            json_bytes(&artifacts.review_requests)?,
        ),
        (
            "review-inputs.json".into(),
            json_bytes(&json!({"schema":"engarg.review-inputs/1","reviews":[]}))?,
        ),
        (
            "context.json".into(),
            json_bytes(&artifacts.bundle.context)?,
        ),
    ]))
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
        EalCommand::RegisterTools(args) => {
            super::tooling_commands::registration::register_tools(args).await
        }
        EalCommand::Replan(args) => execute_replan(args).await,
        EalCommand::Implement(args) => super::tooling_commands::implement(args).await,
        EalCommand::ToolRun(args) => super::tooling_commands::tool_run(args).await,
        EalCommand::Prepare(args) => {
            let (report, mut plan, catalogue) =
                export_inputs(&args.input, args.collector_catalogue.as_deref())?;
            if report.evidence_planning.is_some()
                && (args.acquisition_plan.is_some()
                    || args.collector_catalogue.is_some()
                    || args.operator.operator_inputs.is_some())
            {
                return Err(Error::InvalidInput("support graphs retain qualified routes and operator configuration; flat acquisition plans, catalogues and operator-inputs cannot override them".into()));
            }
            if let Some(path) = args.acquisition_plan.as_deref() {
                plan = acquisition::override_plans_with_catalogue(
                    &report.argument,
                    &plan,
                    super::read_json(path, MAX_INPUT_BYTES)?,
                    catalogue.as_ref(),
                )?;
            }
            let operator = args.operator.load()?;
            if args.output_dir.exists() {
                return Err(Error::InvalidInput(
                    "prepare output directory already exists".into(),
                ));
            }
            if let Some(parent) = args.output_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::create_dir(&args.output_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&args.output_dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let result = (|| {
                let directory = args.output_dir.canonicalize()?;
                let policy = match args.export_policy.as_deref() {
                    Some(path) => super::read_json(path, 1_048_576)?,
                    None => engarg::export_policy::ExportPolicy::default(),
                };
                let files = if report.evidence_planning.is_some() {
                    prepare_support_files(&report, &directory, &policy)?
                } else {
                    engarg::eal_prepare::prepare_with_operator_inputs(
                        &report.request,
                        &report.argument,
                        &plan,
                        &directory,
                        &std::env::current_exe()?,
                        catalogue.as_ref(),
                        &policy,
                        operator
                            .as_ref()
                            .map(|(inputs, base)| (inputs, base.as_path())),
                    )?
                };
                for (name, bytes) in files {
                    let mut file = PendingFile::create(directory.join(name))?;
                    file.write(&bytes)?;
                    file.keep = true;
                }
                Ok::<_, Error>(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&args.output_dir);
            }
            result
        }
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
        EalCommand::Collect(args) => {
            if args.destination.host.tools.is_none() && args.destination.host.url.is_none() {
                return Err(Error::InvalidInput(
                    "collection requires --tools or an explicitly configured remote --url".into(),
                ));
            }
            let document = source_document(&args.source)?;
            let claim = mapped_claim(&document, &args.claim)?;
            let context = match args.context {
                Some(path) => super::read_json::<Value>(&path, MAX_INPUT_BYTES)?,
                None => document.context(),
            };
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
            let context = match args.context {
                Some(path) => super::read_json::<Value>(&path, MAX_INPUT_BYTES)?,
                None => document.context(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_catalogue_preserves_runtime_contract_and_identical_descriptors() {
        let bytes = include_bytes!("../contracts/collectors/native.json");
        let original = engarg::collector_catalogue::CollectorCatalogue::from_json(bytes).unwrap();
        let merged = merge_catalogues(Some(original.clone()), original.clone()).unwrap();
        let actual = serde_json::to_value(&merged).unwrap();
        let expected = serde_json::to_value(&original).unwrap();
        assert_eq!(actual["schema"], expected["schema"]);
        assert_eq!(
            actual["acquisition_contract"],
            expected["acquisition_contract"]
        );
        assert_eq!(merged.collectors.len(), original.collectors.len());
        merged.validate().unwrap();
    }
}
