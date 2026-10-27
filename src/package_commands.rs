//! CLI input admission for the portable executable package compiler.
use clap::Args;
use engarg::{
    Error, domain::ArgumentCase, export_policy::ExportPolicy,
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
    let report = engarg::report::read_direct(&raw)?;
    let request = &report.request;
    let argument = &report.argument;
    let elaboration = report
        .support_elaboration
        .as_ref()
        .expect("validated direct report");
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
        request,
        argument,
        elaboration,
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
    /// Complete revised support requirements; required when executable argument semantics change.
    #[arg(long)]
    support_elaboration: Option<PathBuf>,
    /// Operator manifest used to recompute bindings for the revised operation set.
    #[arg(long)]
    tools: Option<PathBuf>,
    /// Inspect exact change contexts without accepting the revision.
    #[arg(long)]
    preview: bool,
    /// New revision file. Previous evidence and assessments remain untouched.
    #[arg(long)]
    output: PathBuf,
}

pub fn revise(args: ReviseArgs) -> Result<(), Error> {
    use std::io::Write;
    let raw: serde_json::Value = super::read_json(&args.input, 16 * 1_048_576)?;
    let report = if raw
        .get("evidence_planning")
        .is_some_and(|value| !value.is_null())
    {
        engarg::report::read_direct(&raw)?
    } else {
        engarg::report::read(&raw)?
    };
    let argument: ArgumentCase = super::read_json(&args.argument, 16 * 1_048_576)?;
    let result = if args.preview {
        engarg::engine::support::revision::preview(&report, &argument)?
    } else {
        let support: Option<SupportElaboration> = args
            .support_elaboration
            .as_ref()
            .map(|path| super::read_json(path, 16 * 1_048_576))
            .transpose()?;
        let bindings = args
            .tools
            .as_ref()
            .map(|path| {
                let support = support
                    .as_ref()
                    .or(report.support_elaboration.as_ref())
                    .ok_or_else(|| {
                        Error::InvalidInput("--tools requires complete support_elaboration".into())
                    })?;
                engarg::direct_planning::DirectPlanner::from_tools(path)?
                    .resolve_operations(&argument, support)
            })
            .transpose()?;
        serde_json::to_value(engarg::engine::support::revision::revise_with_support(
            &report, argument, support, bindings,
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
