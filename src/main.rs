use clap::{Args, Parser, Subcommand, ValueEnum};
use engarg::{
    Error,
    domain::{ArgumentCase, Request, case_schema, validate_case, validate_request},
    engine::{Mode, ProviderRunConfig, RunConfig},
    presentations,
    provider::{ApiStyle, HttpConfig, HttpModel, Role},
    render,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

mod eal_commands;

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
        command: eal_commands::EalCommand,
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
    #[arg(long, default_value_t = 1_048_576)]
    max_input_bytes: usize,
    #[arg(long, default_value_t = 131_072)]
    max_response_bytes: usize,
    #[arg(long, default_value_t = 10_000)]
    max_output_tokens: u32,
    #[arg(long, default_value_t = 120)]
    request_timeout_secs: u64,
    #[arg(long, default_value_t = 480)]
    timeout_secs: u64,
    /// Private JSONL audit, including completed calls even if a later stage fails.
    #[arg(long)]
    audit: Option<PathBuf>,
    #[command(flatten)]
    output: Output,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Json,
    #[value(alias = "template")]
    Markdown,
    Outline,
    Prose,
    #[value(alias = "expandable")]
    Html,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Request,
    Case,
}

#[derive(Args)]
struct Output {
    #[arg(long, value_enum, default_value = "markdown")]
    format: Format,
    /// Maximum visible outline depth. Deeper arguments get separate records.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=6))]
    outline_depth: u8,
    /// New output file. Existing files are preserved. Omit for stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> Result<T, Error> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::Budget("input file exceeds byte limit".into()));
    }
    Ok(serde_json::from_slice(&bytes)?)
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
        Format::Markdown => render::with_audit(render::markdown(request, argument), &value)?,
        Format::Outline => {
            presentations::outline(request, argument, usize::from(output.outline_depth))
        }
        Format::Prose => presentations::prose(request, argument),
        Format::Html => presentations::html(request, argument),
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
    Ok(content)
}

#[derive(Serialize)]
struct StartAudit<'a> {
    event: &'static str,
    version: &'static str,
    request: &'a Request,
    config: &'a RunConfig,
    #[serde(flatten)]
    provider_config: &'a ProviderRunConfig,
}

fn audit_event(file: &Option<Mutex<File>>, event: &impl Serialize) -> Result<(), Error> {
    if let Some(file) = file {
        let mut f = file
            .lock()
            .map_err(|_| Error::Provider("audit lock poisoned".into()))?;
        serde_json::to_writer(&mut *f, event)?;
        f.write_all(b"\n")?;
        f.flush()?;
    }
    Ok(())
}

async fn analyse(args: Analyse) -> Result<(), Error> {
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
    let model = HttpModel::new(HttpConfig {
        endpoint: args.endpoint,
        api_key: key,
        model: args.model,
        service_tier: args.service_tier,
        reasoning_effort: args.reasoning_effort,
        role_models,
        style: args.api,
        timeout: Duration::from_secs(args.request_timeout_secs),
        max_output_tokens: args.max_output_tokens,
        // Provider envelopes also contain metadata. The content itself has a separate limit.
        max_body_bytes: args
            .max_response_bytes
            .saturating_mul(2)
            .saturating_add(65536),
    })?;
    let audit = args
        .audit
        .as_ref()
        .map(|path| private_file(path).map(Mutex::new))
        .transpose()?;
    audit_event(
        &audit,
        &StartAudit {
            event: "start",
            version: env!("CARGO_PKG_VERSION"),
            request: &request,
            config: &config,
            provider_config: &provider_config,
        },
    )?;
    let observer = |record: &engarg::engine::CallRecord| {
        audit_event(&audit, &json!({"event":"completion","record":record}))
    };
    let run = engarg::engine::run_with_observer(&model, &request, &config, &observer);
    let result = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(args.timeout_secs),run)=> match result {Ok(result)=>result,Err(_)=>Err(Error::Budget("run deadline exceeded; provider cancellation does not guarantee billing cancellation".into()))},
        result=tokio::signal::ctrl_c()=>{result?; Err(Error::Budget("interrupted; provider cancellation does not guarantee billing cancellation".into()))},
    };
    match result {
        Ok(mut report) => {
            report.provider_config = Some(provider_config);
            let content = report_content(&args.output, &request, &report.argument, &report)?;
            match emit(&args.output, content) {
                Ok(()) => audit_event(
                    &audit,
                    &json!({"event":"success","input_sha256":report.input_sha256}),
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
        Command::Eal { command } => eal_commands::execute(command).await,
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
        Command::Schema { kind } => {
            let schema = match kind {
                SchemaKind::Request => serde_json::to_value(schemars::schema_for!(Request))?,
                SchemaKind::Case => case_schema(),
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
