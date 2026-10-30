//! Adapt explicit files to EAL's pure checked ASPIC implementation.
use super::{MAX_INPUT_BYTES, json_bytes, new_path, read_text};
use clap::{Args, ValueEnum};
use eal_core::ExecutionLimits;
use engarg::Error;
use serde::Deserialize;
use serde_json::Value;
use std::{io::Write, path::PathBuf, time::Duration};

#[derive(Clone, Copy, Default, ValueEnum)]
enum MethodProfile {
    #[default]
    Builtin,
    Engineering,
    Aspic,
    All,
}

#[derive(Args)]
pub struct ExportAspicArgs {
    /// Self-contained case.eal from an EngArg executable package.
    #[arg(long)]
    input: PathBuf,
    /// Complete successful eal-host run JSON, retaining collection and assessment.
    #[arg(long)]
    execution: PathBuf,
    /// Optional EngArg assurance-receipt/2 bound to this exact native execution.
    #[arg(long)]
    verification: Option<PathBuf>,
    /// Exact EAL goal name. Otherwise use the retained execution's claim.
    #[arg(long)]
    claim: Option<String>,
    /// Original method profile; must reproduce the retained assessment.
    #[arg(long, value_enum, default_value = "builtin")]
    methods: MethodProfile,
    /// Paired official eal-core executable. Never a shell command string.
    #[arg(long, env = "ENGARG_EAL_CORE", default_value = "eal-core")]
    eal_core: PathBuf,
    /// One deadline covering native compilation and checked export.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout_secs: u64,
    /// EAL TOML file containing a [limits] table; omitted fields use EAL defaults.
    #[arg(long)]
    limits: Option<PathBuf>,
    /// New aspic-view/3 JSON file. Existing files are never replaced.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitDocument {
    limits: ExecutionLimits,
}

pub async fn execute(args: ExportAspicArgs) -> Result<(), Error> {
    let output = new_path(&args.output)?;
    let source = read_text(&args.input)?;
    let execution: Value = super::super::read_json(&args.execution, MAX_INPUT_BYTES)?;
    let limits = if let Some(path) = &args.limits {
        let bytes = engarg::bounded_file::read(path, 16 * 1024, "ASPIC limits")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::InvalidInput("ASPIC limits must be UTF-8 TOML".into()))?;
        toml::from_str::<LimitDocument>(text)
            .map_err(|error| Error::InvalidInput(format!("invalid ASPIC limits: {error}")))?
            .limits
    } else {
        ExecutionLimits::default()
    };
    let methods = match args.methods {
        MethodProfile::Builtin => "builtin",
        MethodProfile::Engineering => "engineering",
        MethodProfile::Aspic => "aspic",
        MethodProfile::All => "all",
    };
    let mut view = engarg::aspic_view::export(
        &source,
        &execution,
        args.claim.as_deref(),
        &engarg::aspic_view::Config {
            executable: args.eal_core,
            methods: methods.into(),
            timeout: Duration::from_secs(args.timeout_secs),
            limits,
        },
    )
    .await?;
    if let Some(path) = &args.verification {
        let receipt: engarg::optimal_argument::assurance::AssuranceReceipt =
            super::super::read_json(path, MAX_INPUT_BYTES)?;
        engarg::aspic_view::attach_verification(&mut view, &execution, &receipt)?;
    }
    for (field, maximum) in [("arguments", 4096), ("defeats", 65536)] {
        let count = view[field]
            .as_array()
            .ok_or_else(|| {
                Error::ToolExecution(format!(
                    "eal-core returned an invalid ASPIC {field} collection"
                ))
            })?
            .len();
        if count > maximum {
            return Err(Error::Budget(format!(
                "ASPIC {field} exceed the browser import limit of {maximum}"
            )));
        }
    }
    let bytes = json_bytes(&view)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::Budget(
            "ASPIC view exceeds the 4 MiB browser import limit".into(),
        ));
    }
    let parent = output.parent().expect("new_path has a canonical parent");
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    stage.write_all(&bytes)?;
    stage.as_file().sync_all()?;
    stage
        .persist_noclobber(&output)
        .map_err(|error| Error::Io(error.error))?;
    Ok(())
}
