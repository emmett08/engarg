//! Admit an optimisation run and retain its bounded search and candidate executions.
use clap::Args;
use engarg::{
    Error,
    export_policy::ExportPolicy,
    optimal_argument::{self, SearchConfig, execution::ExecutionConfig},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf, time::Duration};

fn pins(paths: &[PathBuf]) -> Result<Vec<String>, Error> {
    paths
        .iter()
        .map(|path| {
            engarg::bounded_file::sha256(path, 256 * 1_048_576, "optimisation input")
                .map(|digest| digest.sha256)
        })
        .collect()
}

fn verify_inputs(
    config: &ExecutionConfig,
    paths: &[PathBuf],
    expected: &[String],
) -> Result<(), Error> {
    let mut current = engarg::package::input_files(&config.tools)?;
    current.push(config.eal_host.clone());
    if current != paths || pins(&current)? != expected {
        return Err(Error::InvalidInput(
            "optimisation tool inputs or host changed during the run".into(),
        ));
    }
    Ok(())
}

#[derive(Args)]
pub struct OptimiseArgs {
    /// Admitted authoring report with complete executable support requirements.
    #[arg(long)]
    input: PathBuf,
    /// Operator bindings used to collect NEW evidence for each candidate.
    #[arg(long)]
    tools: PathBuf,
    /// New private directory retaining all candidates and optimisation.json.
    #[arg(long)]
    output_dir: PathBuf,
    /// Number of assessed states retained after each route choice.
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u16).range(1..=64))]
    beam_width: u16,
    /// Maximum distinct candidates, including the original selection.
    #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u16).range(1..=256))]
    max_candidates: u16,
    /// Official paired EAL host; its contracts are checked before collection.
    #[arg(long, env = "ENGARG_EAL_HOST", default_value = "eal-host")]
    eal_host: PathBuf,
    /// Deadline per candidate host invocation. Failed calls are not retried.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600))]
    candidate_timeout_secs: u64,
    /// Explicit collector environment variable NAME, declared by the tools manifest.
    #[arg(long)]
    collector_env: Vec<String>,
    /// Operator-declared public names among the forwarded collector environment.
    #[arg(long)]
    public_collector_env: Vec<String>,
    /// Fixed freshness policy used for every candidate.
    #[arg(long)]
    export_policy: Option<PathBuf>,
}

fn write(path: &std::path::Path, value: &impl Serialize) -> Result<(), Error> {
    let mut file = super::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub async fn execute(args: OptimiseArgs) -> Result<(), Error> {
    let raw: Value = super::read_json(&args.input, 16 * 1_048_576)?;
    let report = engarg::report::read_direct(&raw)?;
    let policy: ExportPolicy = args
        .export_policy
        .as_deref()
        .map(|path| super::read_json(path, 1_048_576))
        .transpose()?
        .unwrap_or_default();
    policy.resolve(&report.argument)?;
    let tools = args.tools.canonicalize()?;
    let output = super::path_admission::identity(&args.output_dir)?;
    if output.symlink_metadata().is_ok() {
        return Err(Error::InvalidInput(
            "optimisation output must be a new directory".into(),
        ));
    }
    let resource_tree = super::path_admission::identity(
        &tools
            .parent()
            .expect("canonical manifest has a parent")
            .join("tools"),
    )?;
    if output.starts_with(resource_tree) {
        return Err(Error::InvalidInput(
            "optimisation output must be outside the operator tools resource tree".into(),
        ));
    }
    // Check local resources and the selected runtime before creating a run or
    // contacting an evidence source. This does not collect observations.
    let mut input_files = engarg::package::input_files(&tools)?;
    let eal_host = if args.eal_host.is_absolute() || args.eal_host.components().count() > 1 {
        args.eal_host.canonicalize()?
    } else {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(&args.eal_host))
            .find(|path| path.is_file())
            .ok_or_else(|| Error::InvalidInput("selected eal-host not found on PATH".into()))?
            .canonicalize()?
    };
    input_files.push(eal_host.clone());
    let input_pins = pins(&input_files)?;
    let target = engarg::runtime_compatibility::preflight(Some(&eal_host)).await?;
    let mut directory = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(&output)?;
    write(&output.join("input-report.json"), &report)?;
    let config = SearchConfig {
        beam_width: usize::from(args.beam_width),
        max_candidates: usize::from(args.max_candidates),
    };
    write(
        &output.join("run.json"),
        &json!({
            "schema":"engarg.argument-optimisation-run/1",
            "input_report_sha256":eal_collector_contracts::execution::canonical_digest(&report).map_err(Error::InvalidInput)?,
            "search":config,"target_runtime":target,
            "candidate_timeout_seconds":args.candidate_timeout_secs,
            "input_files":input_files.iter().zip(&input_pins).map(|(path,digest)|json!({"path":path,"sha256":digest})).collect::<Vec<_>>(),
            "export_policy":policy,"model_calls":0,
        }),
    )?;
    let execution = ExecutionConfig {
        tools,
        output: output.clone(),
        eal_host,
        timeout: Duration::from_secs(args.candidate_timeout_secs),
        collector_env: args.collector_env,
        public_collector_env: args.public_collector_env,
        policy,
    };
    let mut index = 0;
    let search = optimal_argument::search(&report.request, &report.argument, config, |candidate| {
        let number = index;
        index += 1;
        eprintln!("engarg: assessing argument candidate {number}");
        let report = &report;
        let execution = &execution;
        let input_files = &input_files;
        let input_pins = &input_pins;
        async move {
            verify_inputs(execution, input_files, input_pins)?;
            let assessed =
                optimal_argument::execution::assess(report, &candidate, execution, number).await?;
            verify_inputs(execution, input_files, input_pins)?;
            Ok(assessed)
        }
    });
    let result = tokio::select! {
        result = search => result,
        signal = tokio::signal::ctrl_c() => {
            signal?;
            Err(Error::Budget("optimisation interrupted; completed candidate artefacts remain in the output directory".into()))
        }
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
    let assessment_complete = result.candidates.iter().all(|candidate| {
        matches!(
            candidate.assessment.status.as_str(),
            "supported" | "unsupported"
        )
    });
    let best = result.best.map(|index| &result.candidates[index]);
    let receipt = json!({
        "schema":"engarg.argument-optimisation/1",
        "claim":report.argument.current_claim,
        "scope":report.argument.scope,
        "objective":"Minimise unique selected dependency nodes among candidates supported by a valid, complete EAL assessment under unchanged authored obligations.",
        "optimality":if best.is_none() {"no_supported_candidate"}
            else if result.search_complete && assessment_complete {"minimum_within_declared_routes"}
            else {"best_found"},
        "assessment_complete":assessment_complete,
        "selected_report":best.map(|best|format!("{}/report.json",best.assessment.artifact)),
        "selected_package":best.map(|best|format!("{}/package",best.assessment.artifact)),
        "search":result,"model_calls":0,
        "limitations":[
            "Only declared alternative-route selections are searched; no new premises, warrants or claims are generated.",
            "Supported is one acceptance class, not a numerical ranking of evidential force. The result does not establish empirical truth or prose/formula correspondence.",
            "Every candidate has its own source-bound observations and assessment time. Sources may change between candidates; evidence is not a common frozen snapshot.",
            "All assumptions, defeaters, support criteria and unselected routes remain in each canonical report. Length counts unique selected dependency nodes, not words or stored bytes.",
            "The retained assessment applies at its recorded time and scope; later use may require new evidence and reassessment."
        ]
    });
    write(&output.join("optimisation.json"), &receipt)?;
    println!("{}", output.join("optimisation.json").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_pins_detect_resource_addition_content_changes_and_host_changes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("tools")).unwrap();
        let tools = root.path().join("operator.toml");
        std::fs::write(&tools, "[context]\nsite='fixture'\n").unwrap();
        let host = root.path().join("host");
        std::fs::write(&host, "synthetic executable identity").unwrap();
        let config = ExecutionConfig {
            tools,
            eal_host: host.clone(),
            output: root.path().join("run"),
            timeout: Duration::from_secs(1),
            collector_env: vec![],
            public_collector_env: vec![],
            policy: ExportPolicy::default(),
        };
        let mut paths = engarg::package::input_files(&config.tools).unwrap();
        paths.push(host.clone());
        let original = pins(&paths).unwrap();
        verify_inputs(&config, &paths, &original).unwrap();
        let resource = root.path().join("tools/added.txt");
        std::fs::write(&resource, "new resource").unwrap();
        assert!(verify_inputs(&config, &paths, &original).is_err());
        std::fs::remove_file(&resource).unwrap();
        std::fs::write(&config.tools, "[context]\nsite='changed'\n").unwrap();
        assert!(verify_inputs(&config, &paths, &original).is_err());
        std::fs::write(&config.tools, "[context]\nsite='fixture'\n").unwrap();
        std::fs::write(host, "changed executable").unwrap();
        assert!(verify_inputs(&config, &paths, &original).is_err());
    }
}
