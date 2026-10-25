use clap::{Args, Parser, Subcommand, ValueEnum};
use engarg::{
    Error,
    domain::{ArgumentCase, Request, case_schema, validate_case, validate_request},
    engine::{Mode, ProviderRunConfig, RunConfig},
    presentations,
    provider::{ApiStyle, HttpConfig, HttpModel, Role, TransportConfig},
    render,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

mod eal_commands;
mod glossary_command;
mod progress;
mod support_commands;
mod tooling_commands;

#[derive(Parser)]
#[command(
    version,
    about = "Construct, challenge and qualify engineering arguments",
    long_about = "Start with a claim. EngArg develops an extended Toulmin argument, challenges its reasoning and specifies the evidence needed to establish it. Context and evidence are optional. Model judgements remain provisional."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Develop an argument and evidence plan from a claim through separate LLM calls.
    Analyse(Box<Analyse>),
    /// Convert to EAL/3 and invoke eal-mcp through its official host.
    Eal {
        #[command(subcommand)]
        command: Box<eal_commands::EalCommand>,
    },
    /// Display a labelled synthetic case without model calls or credentials.
    Demo {
        /// Show argument construction from a claim with no supplied evidence.
        #[arg(long)]
        claim_only: bool,
        #[command(flatten)]
        output: Output,
    },
    /// Render a saved JSON run in another view, without model calls.
    Render {
        #[arg(long)]
        input: PathBuf,
        #[command(flatten)]
        output: Output,
    },
    /// Print the original engineering Toulmin template used by Markdown reports.
    Template,
    /// Explain argument roles without model calls, for example glossary W.
    Glossary {
        /// Role code or concept key. Omit to show the complete glossary.
        code: Option<String>,
        /// Print the shared definitions as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Export a machine-readable input or output schema.
    Schema {
        #[arg(value_enum, default_value = "case")]
        kind: SchemaKind,
    },
    /// Check references and reporting rules for a saved case, without LLM calls.
    Validate {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        case: PathBuf,
    },
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct Source {
    /// Claim to develop. Context and evidence are optional.
    #[arg(long)]
    claim: Option<String>,
    /// JSON containing a claim and optional context/evidence. Sources are not fetched.
    #[arg(long)]
    input: Option<PathBuf>,
}

#[derive(Args)]
struct Analyse {
    #[command(flatten)]
    source: Source,
    /// Default model identifier, supplied explicitly for reproducible selection.
    #[arg(long, env = "ENGARG_MODEL")]
    model: String,
    /// Requested service tier, passed unchanged and recorded in the audit.
    #[arg(long, env = "ENGARG_SERVICE_TIER")]
    service_tier: Option<String>,
    /// Optional model-specific effort, such as low, medium, high, xhigh or max.
    /// Omit to leave the provider default and support models without this option.
    #[arg(long, env = "ENGARG_REASONING_EFFORT")]
    reasoning_effort: Option<String>,
    /// Per-role override: sceptic=MODEL. Repeat for multiple roles.
    #[arg(long, value_name = "ROLE=MODEL")]
    role_model: Vec<String>,
    #[arg(long, default_value = "hybrid")]
    mode: Mode,
    /// API protocol. The endpoint must support strict JSON Schema outputs.
    #[arg(long, value_enum, default_value = "responses")]
    api: ApiStyle,
    /// Full endpoint URL, including /v1/responses or /v1/chat/completions.
    #[arg(long, default_value = "https://api.openai.com/v1/responses")]
    endpoint: String,
    /// Environment variable NAME containing the provider key. Never pass the key.
    #[arg(long, default_value = "ENGARG_API_KEY")]
    api_key_env: String,
    #[arg(long, default_value_t = 6)]
    max_calls: u32,
    /// Maximum warrant layers from either root to a ground; unset leaves depth unbounded.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=16))]
    argument_depth: Option<u8>,
    /// Extra calls to correct invalid final model output. Reserve these in max-calls.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=3))]
    max_repairs: u8,
    #[arg(long, default_value_t = 1_048_576)]
    max_input_bytes: usize,
    #[arg(long, default_value_t = 131_072)]
    max_response_bytes: usize,
    #[arg(long, default_value_t = 10_000)]
    max_output_tokens: u32,
    /// Extra HTTP attempts after retryable 429 responses. Logical model calls stay fixed.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=10))]
    http_retries: u8,
    /// Maximum retry wait. A longer valid Retry-After stops the call instead of retrying early.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600))]
    retry_max_delay_secs: u64,
    /// Concurrent HTTP sequences. Use 1 to pace the independent hybrid specialists.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u16).range(1..=64))]
    max_concurrent_requests: u16,
    /// Deadline per logical call including HTTP admission, retry waits and response reads.
    #[arg(long, default_value_t = 120)]
    request_timeout_secs: u64,
    #[arg(long, default_value_t = 480)]
    timeout_secs: u64,
    /// Private JSONL audit, including completed calls even if a later stage fails.
    #[arg(long)]
    audit: Option<PathBuf>,
    /// Report phases, model calls and a ten-second waiting heartbeat on stderr.
    #[arg(long)]
    progress: bool,
    /// Omit acquisition tool planning when only the canonical argument is needed.
    #[arg(long)]
    no_tool_planning: bool,
    /// Tool-independent support requirements and shared evidence graph, or legacy flat planning.
    #[arg(
        long,
        value_enum,
        default_value = "flat",
        conflicts_with = "no_tool_planning"
    )]
    evidence_planning: EvidencePlanning,
    /// Operator-owned capability planning configuration for graph mode.
    #[arg(long)]
    evidence_config: Option<PathBuf>,
    /// Operator-selected versioned collector catalogue, for example eal-collector describe output.
    #[arg(long, conflicts_with = "no_tool_planning")]
    collector_catalogue: Option<PathBuf>,
    /// Generate and verify missing collector implementations after argument planning.
    #[arg(long, conflicts_with = "no_tool_planning")]
    implement_tools: bool,
    #[command(flatten)]
    tooling: tooling_commands::ToolOptions,
    #[command(flatten)]
    output: Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum EvidencePlanning {
    Flat,
    Graph,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Json,
    /// Complete argument in Markdown. Use --no-audit-appendix for a reader copy.
    #[value(alias = "template", alias = "md")]
    Markdown,
    Outline,
    Prose,
    /// One authored statement per indented line for inspecting the argument.
    #[value(alias = "plain", alias = "txt")]
    Argument,
    #[value(alias = "expandable")]
    Html,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Request,
    Case,
    Support,
    ExecutionPlan,
    Capabilities,
}

#[derive(Args)]
struct Output {
    #[arg(long, value_enum, default_value = "markdown")]
    format: Format,
    /// Maximum visible outline depth. Deeper arguments get separate records.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=6))]
    outline_depth: u8,
    /// Detailed claim levels in outline/prose/HTML. JSON and template stay complete.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=16))]
    display_depth: Option<u8>,
    /// Markdown only: omit the full JSON audit appendix; keep all argument records.
    #[arg(long)]
    no_audit_appendix: bool,
    /// New output file. Existing files are preserved. Omit for stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> Result<T, Error> {
    let bytes = engarg::bounded_file::read(path, limit, "input file")?;
    let value = eal_collector_contracts::strict_json_exact(
        &bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: limit,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    Ok(serde_json::from_value(value)?)
}

fn private_file(path: &Path) -> Result<File, Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn emit(output: &Output, content: String) -> Result<(), Error> {
    match &output.output {
        Some(path) => {
            let mut f = private_file(path)?;
            if let Err(error) = f
                .write_all(content.as_bytes())
                .and_then(|()| f.write_all(b"\n"))
                .and_then(|()| f.sync_all())
            {
                drop(f);
                let _ = std::fs::remove_file(path);
                return Err(error.into());
            }
        }
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            writeln!(lock, "{content}")?;
        }
    }
    Ok(())
}

fn render_error(error: presentations::RenderError) -> Error {
    match error {
        presentations::RenderError::InvalidStructure => Error::InvalidOutput(error.to_string()),
        presentations::RenderError::ResourceLimit(_) => Error::Budget(error.to_string()),
    }
}

fn report_content(
    output: &Output,
    request: &Request,
    argument: &ArgumentCase,
    report: &impl Serialize,
) -> Result<String, Error> {
    let value = serde_json::to_value(report)?;
    let synthetic = value["execution_mode"] == "offline_synthetic_fixture";
    let mut content = match output.format {
        Format::Json => return Ok(serde_json::to_string_pretty(&value)?),
        Format::Markdown => {
            let markdown = render::markdown(request, argument).map_err(render_error)?;
            if output.no_audit_appendix {
                markdown
            } else {
                render::with_audit(markdown, &value)?
            }
        }
        Format::Outline => presentations::outline_with_depth(
            request,
            argument,
            usize::from(output.outline_depth),
            output.display_depth,
        )
        .map_err(render_error)?,
        Format::Prose => presentations::prose_with_depth(request, argument, output.display_depth)
            .map_err(render_error)?,
        Format::Argument => presentations::plain_lines_with_depth(
            request,
            argument,
            usize::from(output.outline_depth),
            output.display_depth,
        )
        .map_err(render_error)?,
        Format::Html => presentations::html_with_depth(request, argument, output.display_depth)
            .map_err(render_error)?,
    };
    if synthetic {
        let notice = "Synthetic demonstration. No model calls, source verification or experiments were performed.";
        if matches!(output.format, Format::Html) {
            content =
                content.replacen("<body>", &format!("<body><p role=\"note\">{notice}</p>"), 1);
        } else {
            content = format!("{notice}\n\n{content}");
        }
    }
    if content.len() > presentations::RenderLimits::default().max_output_bytes {
        return Err(Error::Budget(
            "render.resource_limit: final output budget exceeded".into(),
        ));
    }
    Ok(content)
}

#[derive(Serialize)]
struct StartAudit<'a> {
    event: &'static str,
    version: &'static str,
    request: &'a Request,
    config: &'a RunConfig,
    transport_config: &'a TransportConfig,
    #[serde(flatten)]
    provider_config: &'a ProviderRunConfig,
}

fn audit_event(file: &Mutex<Option<File>>, event: &impl Serialize) -> Result<(), Error> {
    let mut file = file
        .lock()
        .map_err(|_| Error::Provider("audit lock poisoned".into()))?;
    if let Some(f) = file.as_mut() {
        serde_json::to_writer(&mut *f, event)?;
        f.write_all(b"\n")?;
        f.flush()?;
    }
    Ok(())
}

async fn analyse(args: Analyse) -> Result<(), Error> {
    let progress = progress::Progress::new(args.progress);
    progress
        .run("analysis", analyse_with_progress(args, &progress))
        .await
}

async fn analyse_with_progress(args: Analyse, progress: &progress::Progress) -> Result<(), Error> {
    let started = std::time::Instant::now();
    progress.phase("input and configuration checks");
    if args.max_input_bytes == 0 || args.max_response_bytes == 0 || args.timeout_secs == 0 {
        return Err(Error::InvalidInput(
            "resource limits must be positive".into(),
        ));
    }
    if args.output.output.as_ref().is_some_and(|p| p.exists()) {
        return Err(Error::InvalidInput("output file already exists".into()));
    }
    if matches!((&args.output.output, &args.audit), (Some(output), Some(audit)) if output == audit)
    {
        return Err(Error::InvalidInput(
            "output and audit require different paths".into(),
        ));
    }
    if args.implement_tools
        && (!matches!(args.output.format, Format::Json) || args.output.output.is_none())
    {
        return Err(Error::InvalidInput(
            "--implement-tools requires --format json and --output".into(),
        ));
    }
    let request: Request = match (args.source.claim, args.source.input) {
        (Some(claim), None) => Request {
            claim,
            context: String::new(),
            evidence: vec![],
        },
        (None, Some(path)) => read_json(&path, args.max_input_bytes)?,
        _ => {
            return Err(Error::InvalidInput(
                "supply exactly one of --claim or --input".into(),
            ));
        }
    };
    validate_request(&request).map_err(|v| Error::InvalidInput(v.join("; ")))?;
    let graph_planner = match (args.evidence_planning, args.evidence_config.as_deref()) {
        (EvidencePlanning::Graph, Some(path)) => Some(support_commands::admit(path).await?),
        (EvidencePlanning::Graph, None) => {
            return Err(Error::InvalidInput(
                "--evidence-planning graph requires --evidence-config".into(),
            ));
        }
        (EvidencePlanning::Flat, Some(_)) => {
            return Err(Error::InvalidInput(
                "--evidence-config requires --evidence-planning graph".into(),
            ));
        }
        _ => None,
    };
    if graph_planner.is_some() && args.collector_catalogue.is_some() {
        return Err(Error::InvalidInput("Graph planning uses qualified capabilities from --evidence-config; --collector-catalogue configures flat planning".into()));
    }
    // Validate graph options before paid calls. Runner admission occurs only for
    // an actual implementation gap after qualified candidate comparison.
    let tooling_session = if graph_planner.is_some() && args.implement_tools {
        args.tooling
            .capability_config(true, args.max_input_bytes, args.max_response_bytes)?;
        None
    } else {
        args.tooling.admit(
            args.implement_tools,
            args.max_input_bytes,
            args.max_response_bytes,
        )?
    };
    let mut role_models = HashMap::new();
    for text in args.role_model {
        let (role, model) = text
            .split_once('=')
            .ok_or_else(|| Error::InvalidInput("role-model requires ROLE=MODEL".into()))?;
        let role: Role = role.parse()?;
        if model.trim().is_empty() || role_models.insert(role, model.to_owned()).is_some() {
            return Err(Error::InvalidInput(
                "duplicate role override or empty model".into(),
            ));
        }
    }
    let key = std::env::var(&args.api_key_env)
        .ok()
        .filter(|s| !s.trim().is_empty());
    if args.endpoint == "https://api.openai.com/v1/responses" && key.is_none() {
        return Err(Error::InvalidInput(format!(
            "set {} to the provider API key",
            args.api_key_env
        )));
    }
    let config = RunConfig {
        mode: args.mode,
        max_calls: args.max_calls,
        max_argument_depth: args.argument_depth,
        max_repairs: args.max_repairs,
        collector_catalogue: args
            .collector_catalogue
            .as_deref()
            .map(|path| {
                let bytes = engarg::bounded_file::read(path, 1_048_576, "collector catalogue")?;
                engarg::collector_catalogue::CollectorCatalogue::from_json(&bytes)
            })
            .transpose()?,
        max_input_bytes: args.max_input_bytes,
        max_response_bytes: args.max_response_bytes,
    };
    let provider_config = ProviderRunConfig {
        endpoint: args.endpoint.clone(),
        api: format!("{:?}", args.api),
        default_model: args.model.clone(),
        requested_service_tier: args.service_tier.clone(),
        requested_reasoning_effort: args.reasoning_effort.clone(),
        role_models: role_models.clone(),
        max_output_tokens: args.max_output_tokens,
        request_timeout_secs: args.request_timeout_secs,
        timeout_secs: args.timeout_secs,
    };
    let transport_config = TransportConfig {
        max_http_retries: args.http_retries,
        retry_max_delay_secs: args.retry_max_delay_secs,
        max_concurrent_requests: usize::from(args.max_concurrent_requests),
    };
    let audit = Arc::new(Mutex::new(None::<File>));
    let transport_audit = Arc::clone(&audit);
    let transport_progress = progress.clone();
    let show_progress = args.progress;
    let http_observer = Arc::new(move |record: &engarg::provider::HttpEvent| {
        let event = json!({"event":"http_attempt","record":record});
        audit_event(&transport_audit, &event)?;
        if show_progress {
            transport_progress.http(record);
        } else if record.kind != "response" {
            eprintln!("{event}");
        }
        Ok(())
    });
    let http_config = || HttpConfig {
        endpoint: args.endpoint.clone(),
        api_key: key.clone(),
        model: args.model.clone(),
        service_tier: args.service_tier.clone(),
        reasoning_effort: args.reasoning_effort.clone(),
        role_models: role_models.clone(),
        style: args.api,
        timeout: Duration::from_secs(args.request_timeout_secs),
        max_output_tokens: args.max_output_tokens,
        max_body_bytes: args
            .max_response_bytes
            .saturating_mul(2)
            .saturating_add(65536),
    };
    let tooling_model = args
        .implement_tools
        .then(|| {
            tooling_commands::provider::independent_model(http_config(), transport_config.clone())
                .map(|model| progress.model(model))
        })
        .transpose()?;
    let model = progress.model(HttpModel::new_with_transport(
        http_config(),
        transport_config.clone(),
        http_observer,
    )?);
    *audit
        .lock()
        .map_err(|_| Error::Provider("audit lock poisoned".into()))? = args
        .audit
        .as_ref()
        .map(|path| private_file(path))
        .transpose()?;
    audit_event(
        &audit,
        &StartAudit {
            event: "start",
            version: env!("CARGO_PKG_VERSION"),
            request: &request,
            config: &config,
            transport_config: &transport_config,
            provider_config: &provider_config,
        },
    )?;
    let observer = |record: &engarg::engine::CallRecord| {
        audit_event(&audit, &json!({"event":"completion","record":record}))?;
        for warning in engarg::engine::assessment_warnings(record) {
            audit_event(
                &audit,
                &json!({
                    "event": "warning",
                    "code": "assessment_list_limit_exceeded",
                    "warning": warning,
                }),
            )?;
            // Warnings remain visible when routine progress is disabled. Only
            // fixed role/field names and counts enter terminal diagnostics.
            let _ = writeln!(
                std::io::stderr().lock(),
                "engarg: warning: {}",
                warning.message()
            );
        }
        Ok(())
    };
    progress.phase("argument authoring and evidence planning");
    let run = async {
        if let Some(planner) = &graph_planner {
            serde_json::to_value(
                engarg::engine::run_with_support(&model, &request, &config, &observer, planner)
                    .await?,
            )
            .map_err(Error::from)
        } else if args.no_tool_planning {
            serde_json::to_value(
                engarg::engine::run_with_observer(&model, &request, &config, &observer).await?,
            )
            .map_err(Error::from)
        } else {
            serde_json::to_value(
                engarg::engine::run_with_tools(&model, &request, &config, &observer).await?,
            )
            .map_err(Error::from)
        }
    };
    let result = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(args.timeout_secs).checked_sub(started.elapsed()).ok_or_else(|| Error::Budget("run deadline exhausted during admission".into()))?,run)=> match result {Ok(result)=>result,Err(_)=>Err(Error::Budget("run deadline exceeded; provider cancellation does not guarantee billing cancellation".into()))},
        result=tokio::signal::ctrl_c()=>{result?; Err(Error::Budget("interrupted; provider cancellation does not guarantee billing cancellation".into()))},
    };
    match result {
        Ok(mut report) => {
            report["provider_config"] = serde_json::to_value(provider_config)?;
            report["limitations"].as_array_mut().ok_or_else(||Error::InvalidOutput("Run limitations missing".into()))?.push(json!(format!("HTTP transport allows {} retries per logical call with a maximum retry delay of {} seconds and {} concurrent request sequences. HTTP attempts have a separate bound from the logical model-call budget.", transport_config.max_http_retries,transport_config.retry_max_delay_secs,transport_config.max_concurrent_requests)));
            let argument: ArgumentCase = serde_json::from_value(report["argument"].clone())?;
            let original = report;
            let input_sha256 = original["input_sha256"].clone();
            let final_report = match (&tooling_session, &tooling_model) {
                (session, Some(model)) => {
                    progress.phase("implementing and verifying missing tools");
                    let stage_result = if graph_planner.is_some() {
                        tooling_commands::capability::run_capability_fallback(
                            model,
                            &original,
                            &args.tooling,
                            args.max_input_bytes,
                            args.max_response_bytes,
                        )
                        .await
                    } else {
                        let session = session.as_ref().ok_or_else(|| {
                            Error::InvalidInput("Implementation session was not admitted".into())
                        })?;
                        tooling_commands::run_stage(
                            session,
                            model,
                            &original,
                            args.tooling.deadline(),
                        )
                        .await
                    };
                    match stage_result {
                        Ok(value) => value,
                        Err(error) => {
                            // A coding failure still leaves the completed canonical
                            // argument at the caller's requested normal output path.
                            let content =
                                report_content(&args.output, &request, &argument, &original)?;
                            emit(&args.output, content)?;
                            audit_event(
                                &audit,
                                &json!({
                                    "event": "failure",
                                    "stage": "tool_implementation",
                                    "error": error.to_string()
                                }),
                            )?;
                            return Err(error);
                        }
                    }
                }
                _ => original,
            };
            progress.phase("writing argument report");
            let content = report_content(&args.output, &request, &argument, &final_report)?;
            match emit(&args.output, content) {
                Ok(()) => audit_event(
                    &audit,
                    &json!({"event":"success","input_sha256":input_sha256}),
                ),
                Err(error) => {
                    audit_event(
                        &audit,
                        &json!({"event":"failure","error":error.to_string()}),
                    )?;
                    Err(error)
                }
            }
        }
        Err(error) => {
            audit_event(
                &audit,
                &json!({"event":"failure","error":error.to_string()}),
            )?;
            Err(error)
        }
    }
}

async fn execute(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Analyse(args) => analyse(*args).await,
        Command::Eal { command } => eal_commands::execute(*command).await,
        Command::Demo { claim_only, output } => {
            let (request_text, argument_text) = if claim_only {
                (
                    include_str!("../examples/claim-only-request.json"),
                    include_str!("../examples/claim-only-case.json"),
                )
            } else {
                (
                    include_str!("../examples/request.json"),
                    include_str!("../examples/case.json"),
                )
            };
            let request: Request = serde_json::from_str(request_text)?;
            let argument: ArgumentCase = serde_json::from_str(argument_text)?;
            validate_case(&request, &argument).map_err(|v| Error::InvalidOutput(v.join("; ")))?;
            let report = json!({"schema_version":"engarg.demo.v2","execution_mode":"offline_synthetic_fixture","request":request,"argument":argument,"limitations":["No model calls, evidence verification or experiments were performed. This is a synthetic demonstration."]});
            let content = report_content(&output, &request, &argument, &report)?;
            emit(&output, content)
        }
        Command::Render { input, output } => {
            let report: serde_json::Value = read_json(&input, 16 * 1_048_576)?;
            let request: Request = serde_json::from_value(
                report
                    .get("request")
                    .cloned()
                    .ok_or_else(|| Error::InvalidInput("saved report requires request".into()))?,
            )?;
            let argument: ArgumentCase =
                serde_json::from_value(report.get("argument").cloned().ok_or_else(|| {
                    Error::InvalidInput("saved report requires argument".into())
                })?)?;
            validate_case(&request, &argument).map_err(|v| Error::InvalidOutput(v.join("; ")))?;
            emit(
                &output,
                report_content(&output, &request, &argument, &report)?,
            )
        }
        Command::Template => {
            print!("{}", include_str!("../templates/engineering-toulmin.md"));
            Ok(())
        }
        Command::Glossary { code, json } => glossary_command::run(code.as_deref(), json),
        Command::Schema { kind } => {
            let schema = match kind {
                SchemaKind::Request => serde_json::to_value(schemars::schema_for!(Request))?,
                SchemaKind::Case => case_schema(),
                SchemaKind::Support => engarg::support_requirements::elaboration_schema(),
                SchemaKind::ExecutionPlan => eal_collector_contracts::execution::plan_schema(),
                SchemaKind::Capabilities => serde_json::to_value(schemars::schema_for!(
                    eal_collector_contracts::capability::CapabilityCatalogue
                ))?,
            };
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
        Command::Validate { input, case } => {
            let request: Request = read_json(&input, 1_048_576)?;
            let argument: ArgumentCase = read_json(&case, 1_048_576)?;
            validate_case(&request, &argument).map_err(|v| Error::InvalidOutput(v.join("; ")))?;
            println!(
                "Structural and reporting checks passed. Evidence truth and inferential soundness remain unverified."
            );
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match execute(Cli::parse()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("engarg: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
