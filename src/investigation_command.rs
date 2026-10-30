//! Run a bounded model-driven argument investigation with frozen executable checks.
use clap::Args;
use engarg::{
    Error,
    engine::{RouteResolver, RunReport},
    optimal_argument::{
        assurance::AssurancePlan,
        development::{self, CandidateInvestigator, CandidateOutcome, DevelopmentConfig},
        execution::ExecutionConfig,
        investigation::VerifiedInvestigator,
    },
    support_requirements::SupportedArgument,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf, sync::Mutex, time::Duration};

#[derive(Args)]
pub struct InvestigateArgs {
    /// An admitted argument report. Its original claim, scope and history stay fixed.
    #[arg(long)]
    input: PathBuf,
    /// Operator-owned acquisition and review bindings; transport authority is never model output.
    #[arg(long)]
    tools: PathBuf,
    /// Fixed premise, formalisation, inference and applicability verification plan JSON.
    #[arg(long)]
    verification: PathBuf,
    /// New private directory for all proposals, checks, executions and the selected result.
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, env = "ENGARG_EAL_HOST", default_value = "eal-host")]
    eal_host: PathBuf,
    #[arg(long,default_value_t=3,value_parser=clap::value_parser!(u8).range(1..=8))]
    rounds: u8,
    /// Includes proposal and independent critic calls. Failed calls consume this budget.
    #[arg(long,default_value_t=8,value_parser=clap::value_parser!(u16).range(1..=64))]
    max_calls: u16,
    #[arg(long,default_value_t=2,value_parser=clap::value_parser!(u16).range(1..=64))]
    beam_width: u16,
    #[arg(long,default_value_t=1800,value_parser=clap::value_parser!(u64).range(1..=86400))]
    timeout_secs: u64,
    #[arg(long,default_value_t=120,value_parser=clap::value_parser!(u64).range(1..=3600))]
    candidate_timeout_secs: u64,
    #[arg(long)]
    collector_env: Vec<String>,
    #[arg(long)]
    public_collector_env: Vec<String>,
    #[arg(long)]
    export_policy: Option<PathBuf>,
    #[arg(long)]
    progress: bool,
    #[command(flatten)]
    provider: crate::tooling_commands::provider::ProviderArgs,
}

fn write(path: &std::path::Path, value: &impl Serialize) -> Result<(), Error> {
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

struct FrozenInvestigator {
    inner: VerifiedInvestigator,
    files: Vec<PathBuf>,
    pins: Vec<String>,
}
impl CandidateInvestigator for FrozenInvestigator {
    async fn assess(
        &self,
        parent: &RunReport,
        proposal: SupportedArgument,
        index: usize,
    ) -> Result<CandidateOutcome, Error> {
        crate::argument_inputs::verify_inputs(&self.inner.execution, &self.files, &self.pins)?;
        let result = self.inner.assess(parent, proposal, index).await;
        crate::argument_inputs::verify_inputs(&self.inner.execution, &self.files, &self.pins)?;
        result
    }
}

pub async fn execute(args: InvestigateArgs) -> Result<(), Error> {
    let progress = crate::progress::Progress::new(args.progress);
    progress
        .run("argument investigation", execute_inner(args, &progress))
        .await
}

async fn execute_inner(
    args: InvestigateArgs,
    progress: &crate::progress::Progress,
) -> Result<(), Error> {
    progress.phase("admitting fixed target, verification contract and operator tools");
    let input: Value = crate::read_json(&args.input, 16 * 1_048_576)?;
    let original = engarg::report::read_direct(&input)?;
    let plan: AssurancePlan = crate::read_json(&args.verification, 1_048_576)?;
    plan.validate(&original)?;
    let policy = args
        .export_policy
        .as_deref()
        .map(|path| crate::read_json(path, 1_048_576))
        .transpose()?
        .unwrap_or_default();
    let tools = args.tools.canonicalize()?;
    let output = crate::argument_inputs::output(&tools, &args.output_dir)?;
    let host = crate::argument_inputs::host(&args.eal_host)?;
    let mut files = engarg::package::input_files(&tools)?;
    files.push(host.clone());
    let pins = crate::argument_inputs::pins(&files)?;
    let target = engarg::runtime_compatibility::preflight(Some(&host)).await?;
    let planner = engarg::direct_planning::DirectPlanner::from_tools(&tools)?;
    let context = json!({"tools":planner.authoring_context(),"verification_plan":plan,
        "method_descriptors":eal_core::methods::default_registry().describe()});
    let provider = args.provider.model()?;
    let provider_metadata = provider.metadata().clone();
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&output)?;
    write(&output.join("input-report.json"), &original)?;
    write(&output.join("verification-plan.json"), &plan)?;
    let config = DevelopmentConfig {
        rounds: args.rounds,
        max_calls: args.max_calls,
        beam_width: usize::from(args.beam_width),
        max_input_bytes: args.provider.max_input_bytes,
        max_response_bytes: args.provider.max_response_bytes,
    };
    write(
        &output.join("run.json"),
        &json!({"schema":"engarg.argument-investigation-run/1",
            "target_runtime":target,"provider_config":provider_metadata,"development":config,
            "input_report_sha256":eal_collector_contracts::execution::canonical_digest(&original).map_err(Error::InvalidInput)?,
            "verification_sha256":eal_collector_contracts::execution::canonical_digest(&plan).map_err(Error::InvalidInput)?,
            "candidate_timeout_seconds":args.candidate_timeout_secs,
            "input_files":files.iter().zip(&pins).map(|(path,sha256)|json!({"path":path,"sha256":sha256})).collect::<Vec<_>>()
        }),
    )?;
    let investigator = FrozenInvestigator {
        inner: VerifiedInvestigator {
            original: original.clone(),
            plan,
            execution: ExecutionConfig {
                tools,
                output: output.clone(),
                eal_host: host,
                timeout: Duration::from_secs(args.candidate_timeout_secs),
                collector_env: args.collector_env,
                public_collector_env: args.public_collector_env,
                policy,
            },
        },
        files,
        pins,
    };
    let audit = Mutex::new(crate::private_file(&output.join("audit.jsonl"))?);
    let observer = |event: &Value| -> Result<(), Error> {
        let mut file = audit
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("investigation audit lock poisoned")))?;
        serde_json::to_writer(&mut *file, event)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    };
    progress.phase("generating arguments and executing frozen premise and formalisation checks");
    let model = progress.model(provider);
    let run = development::develop(
        &model,
        &original,
        config,
        &context,
        &investigator,
        &observer,
    );
    let result = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(args.timeout_secs),run)=>match result{Ok(result)=>result,Err(_)=>Err(Error::Budget("argument investigation deadline exhausted; paid exchanges and candidate attempts remain in the output directory".into()))},
        signal=tokio::signal::ctrl_c()=>{signal?;Err(Error::Budget("argument investigation interrupted; retained exchanges remain in audit.jsonl".into()))}
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            write(
                &output.join("failure.json"),
                &json!({"error":error.to_string()}),
            )?;
            return Err(error);
        }
    };
    let best = result.best.map(|index| &result.candidates[index]);
    write(
        &output.join("investigation.json"),
        &json!({"schema":"engarg.argument-investigation/1",
            "current_claim":original.argument.current_claim,"scope":original.argument.scope,
            "result":result,"selected_report":best.map(|candidate|format!("{}/report.json",candidate.outcome.assessment.artifact)),
            "selected_package":best.map(|candidate|format!("{}/package",candidate.outcome.assessment.artifact)),
            "optimality":if best.is_some(){"best_verified_candidate_found"}else{"no_verified_supported_candidate"},
            "limitations":["The generated family is bounded; no global optimum or unrepresented premise truth is asserted.",
                "Formal checks establish declared mathematical relationships. Empirical evidence and intended-language correspondence retain their own scope and qualifications."]
        }),
    )?;
    println!("{}", output.join("investigation.json").display());
    Ok(())
}
