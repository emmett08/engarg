//! CLI input admission for the portable executable package compiler.
use clap::Args;
use engarg::{
    Error,
    domain::{ArgumentCase, Request},
    export_policy::ExportPolicy,
    support_requirements::SupportElaboration,
};
use std::path::PathBuf;

#[derive(Args)]
pub struct BuildArgs {
    /// Saved authoring report with request, argument and support_elaboration.
    #[arg(long)]
    input: PathBuf,
    /// Direct TOML operations keyed by acquisition ID, with an optional tools/ directory.
    #[arg(long)]
    tools: PathBuf,
    /// New directory containing case.eal, tools.toml, implementations and a build receipt.
    #[arg(long)]
    output_dir: PathBuf,
    /// Explicit freshness policy for review obligations.
    #[arg(long)]
    export_policy: Option<PathBuf>,
}

pub fn build(args: BuildArgs) -> Result<(), Error> {
    let raw: serde_json::Value = super::read_json(&args.input, 16 * 1_048_576)?;
    let request: Request = serde_json::from_value(raw["request"].clone())?;
    let argument: ArgumentCase = serde_json::from_value(raw["argument"].clone())?;
    let elaboration: SupportElaboration = serde_json::from_value(
        raw.get("support_elaboration")
            .or_else(|| raw.pointer("/evidence_planning/elaboration"))
            .ok_or_else(|| {
                Error::InvalidInput("Build requires the authored support_elaboration".into())
            })?
            .clone(),
    )?;
    let policy: ExportPolicy = args
        .export_policy
        .as_ref()
        .map(|path| {
            super::read_json(path, 1_048_576)
                .and_then(|value| serde_json::from_value(value).map_err(Error::from))
        })
        .transpose()?
        .unwrap_or_default();
    let receipt = engarg::package::build(
        &request,
        &argument,
        &elaboration,
        &args.tools,
        &args.output_dir,
        &policy,
    )?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

#[derive(Args)]
pub struct ReviseArgs {
    /// Source report whose argument is being explicitly revised.
    #[arg(long)]
    input: PathBuf,
    /// Complete proposed ArgumentCase, including retained dissent applications.
    #[arg(long)]
    argument: PathBuf,
    /// Inspect exact change contexts without accepting the revision.
    #[arg(long)]
    preview: bool,
    /// New revision file. Previous evidence and assessments remain untouched.
    #[arg(long)]
    output: PathBuf,
}

pub fn revise(args: ReviseArgs) -> Result<(), Error> {
    use std::io::Write;
    let mut raw: serde_json::Value = super::read_json(&args.input, 16 * 1_048_576)?;
    // These are report extensions, not fields of the immutable authoring run.
    for key in [
        "evidence_planning",
        "replanning",
        "support_recovery",
        "support_elaboration",
        "direct_bindings",
    ] {
        raw.as_object_mut()
            .ok_or_else(|| Error::InvalidInput("Report must be an object".into()))?
            .remove(key);
    }
    let report: engarg::engine::RunReport = serde_json::from_value(raw)?;
    let argument: ArgumentCase = super::read_json(&args.argument, 16 * 1_048_576)?;
    let result = if args.preview {
        engarg::engine::support::revision::preview(&report, &argument)?
    } else {
        serde_json::to_value(engarg::engine::support::revision::revise(
            &report, argument,
        )?)?
    };
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.output)?;
    output.write_all(&serde_json::to_vec_pretty(&result)?)?;
    println!("{}", args.output.display());
    Ok(())
}
