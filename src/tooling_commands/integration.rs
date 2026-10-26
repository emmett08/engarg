//! Connect approved execution sources and verified packages to a retained graph analysis.
mod commands;
mod configuration;

use clap::Args;
use engarg::{
    Error,
    report::{self, EvidencePlanningReport},
    route_planning::{GapKind, PlanningConfig, RoutePlan},
    tooling::{capability::ImplementedCapability, contracts::digest_json},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub struct IntegrateArgs {
    #[arg(long)]
    pub input: PathBuf,
    #[arg(long)]
    pub evidence_config: PathBuf,
    #[arg(long)]
    pub output_dir: PathBuf,
    /// Explicit operator-approved eal.evidence-config/1 connection additions.
    #[arg(long)]
    pub mcp_registrations: Option<PathBuf>,
    /// Operator policy executable and arguments as a JSON array. Reads one qualification request.
    #[arg(long)]
    pub qualification_argv: Option<String>,
    /// Operator provider and toolchain options for eal implement, as a JSON array.
    #[arg(long)]
    pub implementation_args: Option<String>,
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=3600))]
    pub qualification_timeout_secs: u64,
    #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86400))]
    pub timeout_secs: u64,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn retain(path: &Path, value: &impl serde::Serialize) -> Result<(), Error> {
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
fn routes(raw: &Value) -> Result<RoutePlan, Error> {
    let EvidencePlanningReport::Graph(planning) = report::classify_evidence_planning(raw)? else {
        return Err(invalid("integration requires a graph analysis"));
    };
    Ok(planning.routes)
}
fn implementation_gaps(plan: &RoutePlan) -> bool {
    plan.gaps.iter().any(|gap| {
        matches!(
            gap.kind,
            GapKind::OperationImplementation | GapKind::TransformationImplementation
        )
    })
}
fn readiness(plan: &RoutePlan) -> Result<(), Error> {
    if plan.discovery.incomplete
        || !plan.gaps.is_empty()
        || plan.routes.iter().any(|route| {
            !route.configuration_gaps.is_empty()
                || route.selected.as_ref().is_none_or(|selected| {
                    !route.candidates.iter().any(|candidate| {
                        candidate.capability_id == *selected && candidate.executable
                    })
                })
        })
    {
        return Err(invalid(
            "integration remains blocked by unresolved evidence routes; inspect analysis.ready.json",
        ));
    }
    plan.execution.validate().map_err(invalid)
}

struct Integration {
    directory: PathBuf,
    analysis: PathBuf,
    planning: PlanningConfig,
    connections: Value,
    input_digests: Value,
    activated_mcp: Vec<String>,
    registered_tools: Vec<String>,
}
impl Integration {
    fn publish_configuration(&mut self) -> Result<PathBuf, Error> {
        let connections = self.directory.join("connections.ready.json");
        let planning = self.directory.join("planning.ready.json");
        self.planning.connections = connections.clone();
        retain(&connections, &self.connections)?;
        retain(&planning, &self.planning)?;
        Ok(planning)
    }
    fn publish_status(&self, status: &str, diagnostics: Vec<String>) -> Result<(), Error> {
        retain(
            &self.directory.join("registration.json"),
            &json!({
                "schema":"engarg.ticket-integration/1", "status":status,
                "input_digests":self.input_digests,
                "registered_mcp_connections":self.activated_mcp,
                "registered_native_providers":self.registered_tools,
                "analysis":self.directory.join("analysis.ready.json"),
                "planning":self.directory.join("planning.ready.json"),
                "connections":self.directory.join("connections.ready.json"),
                "diagnostics":diagnostics,
            }),
        )
    }
}

pub async fn integrate(args: IntegrateArgs) -> Result<(), Error> {
    let input = args.input.canonicalize()?;
    let original: Value = crate::read_json(&input, 16 * 1_048_576)?;
    report::validate_replanning(&original, &std::env::current_exe()?)?;
    routes(&original)?;
    let source_config = args.evidence_config.canonicalize()?;
    let mut planning: PlanningConfig = crate::read_json(&source_config, 4 * 1_048_576)?;
    planning.validate()?;
    let base = source_config
        .parent()
        .ok_or_else(|| invalid("planning configuration has no parent"))?;
    for path in [
        &mut planning.collector,
        &mut planning.connections,
        &mut planning.store,
    ] {
        if !path.is_absolute() {
            *path = base.join(&*path);
        }
    }
    planning.collector = planning.collector.canonicalize()?;
    let mut connections: Value = crate::read_json(&planning.connections, 16 * 1_048_576)?;
    let additions = args
        .mcp_registrations
        .as_ref()
        .map(|path| crate::read_json::<Value>(path, 16 * 1_048_576))
        .transpose()?;
    let activated_mcp = configuration::activate(&mut connections, additions.as_ref())?;
    configuration::environment(&connections, &mut planning.environment)?;
    let mut directory = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(&args.output_dir)?;
    let output = args.output_dir.canonicalize()?;
    let input_digests = json!({"analysis":digest_json(&original)?,"planning":digest_json(&serde_json::to_value(&planning)?)?,"connections":digest_json(&connections)?,"mcp_registrations":additions.as_ref().map(digest_json).transpose()?});
    let mut work = Integration {
        directory: output,
        analysis: input,
        planning,
        connections,
        input_digests,
        activated_mcp,
        registered_tools: Vec::new(),
    };
    let result = tokio::select! {
        result = tokio::time::timeout(std::time::Duration::from_secs(args.timeout_secs), run(&mut work, &args)) => {
            result.map_err(|_| Error::Budget("ticket integration deadline exhausted".into())).and_then(|result| result)
        }
        interrupted = tokio::signal::ctrl_c() => {
            interrupted?;
            Err(Error::Budget("ticket integration interrupted".into()))
        }
        terminated = super::termination_signal() => {
            terminated?;
            Err(Error::Budget("ticket integration terminated".into()))
        }
    };
    if let Err(error) = result {
        // Keep a consistent inspection surface even when operator qualification
        // or a source is unavailable. No blocked report authorises collection.
        let ready = work.directory.join("analysis.ready.json");
        if !ready.exists() {
            retain(
                &ready,
                &crate::read_json::<Value>(&work.analysis, 16 * 1_048_576)?,
            )?;
        }
        if !work.directory.join("planning.ready.json").exists() {
            work.publish_configuration()?;
        }
        let text = error.to_string();
        work.publish_status("blocked", vec![text.chars().take(512).collect()])?;
        return Err(error);
    }
    work.publish_status("completed", Vec::new())?;
    println!(
        "{}",
        json!({"status":"completed","registration":work.directory.join("registration.json"),"analysis":work.directory.join("analysis.ready.json"),"planning":work.directory.join("planning.ready.json"),"connections":work.directory.join("connections.ready.json")})
    );
    Ok(())
}

async fn replan(
    work: &mut Integration,
    config: &Path,
    output: &Path,
    timeout: u64,
) -> Result<(), Error> {
    commands::engarg(
        &work.directory,
        &format!(
            "replan-{}",
            output.file_stem().unwrap_or_default().to_string_lossy()
        ),
        vec![
            "eal".into(),
            "replan".into(),
            "--input".into(),
            work.analysis.to_string_lossy().into_owned(),
            "--evidence-config".into(),
            config.to_string_lossy().into_owned(),
            "--output".into(),
            output.to_string_lossy().into_owned(),
        ],
        timeout,
    )
    .await?;
    work.analysis = output.to_owned();
    Ok(())
}

async fn run(work: &mut Integration, args: &IntegrateArgs) -> Result<(), Error> {
    let initial_connections = work.directory.join("connections.activated.json");
    let initial_planning = work.directory.join("planning.activated.json");
    work.planning.connections = initial_connections.clone();
    retain(&initial_connections, &work.connections)?;
    retain(&initial_planning, &work.planning)?;
    let activated_analysis = work.directory.join("analysis.activated.json");
    replan(
        work,
        &initial_planning,
        &activated_analysis,
        args.timeout_secs,
    )
    .await?;
    let raw: Value = crate::read_json(&work.analysis, 16 * 1_048_576)?;
    let plan = routes(&raw)?;
    if plan.discovery.incomplete
        || plan.gaps.iter().any(|gap| {
            !matches!(
                gap.kind,
                GapKind::OperationImplementation | GapKind::TransformationImplementation
            )
        })
    {
        return Err(invalid(
            "integration requires admitted connections and resolved non-implementation obligations before compilation",
        ));
    }
    if implementation_gaps(&plan) {
        if plan.discovery.incomplete {
            return Err(invalid(
                "tool implementation is blocked until approved discovery completes",
            ));
        }
        let existing = super::registration::packages(&raw, &std::env::current_exe()?)?;
        let existing_identities = existing
            .values()
            .map(|record| record.capability_sha256.as_str())
            .collect::<BTreeSet<_>>();
        let covered = plan
            .gaps
            .iter()
            .filter_map(|gap| gap.implementation.as_ref())
            .map(|request| request.identity())
            .collect::<Result<Vec<_>, _>>()?;
        let packages = if !covered.is_empty()
            && covered
                .iter()
                .all(|id| existing_identities.contains(id.as_str()))
        {
            existing
        } else {
            let options = args.implementation_args.as_deref().ok_or_else(|| {
                invalid("missing compiled tools require operator implementation arguments")
            })?;
            let options = commands::implementation_arguments(options)?;
            let output = work.directory.join("tooling");
            let mut argv = vec![
                "eal".into(),
                "implement".into(),
                "--input".into(),
                work.analysis.to_string_lossy().into_owned(),
                "--tool-output-dir".into(),
                output.to_string_lossy().into_owned(),
            ];
            argv.extend(options);
            commands::engarg(&work.directory, "implement", argv, args.timeout_secs).await?;
            work.analysis = output.join("analysis.with-tools.json");
            let generated: Value = crate::read_json(&work.analysis, 16 * 1_048_576)?;
            super::registration::packages(&generated, &std::env::current_exe()?)?
        };
        register(work, args, &packages).await?;
    }
    let ready_config = work.publish_configuration()?;
    let ready_analysis = work.directory.join("analysis.ready.json");
    replan(work, &ready_config, &ready_analysis, args.timeout_secs).await?;
    let final_report: Value = crate::read_json(&work.analysis, 16 * 1_048_576)?;
    readiness(&routes(&final_report)?)
}

async fn register(
    work: &mut Integration,
    args: &IntegrateArgs,
    packages: &BTreeMap<String, ImplementedCapability>,
) -> Result<(), Error> {
    if packages.is_empty() {
        return Err(invalid(
            "tool verification produced no registrable packages",
        ));
    }
    let argv = args
        .qualification_argv
        .as_deref()
        .ok_or_else(|| invalid("verified custom tools require an operator qualification policy"))?;
    let present = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    let supervisor = super::supervisor()?.canonicalize()?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let mut providers = Vec::new();
    let mut operations = BTreeMap::new();
    for (index, record) in packages.values().enumerate() {
        let template = engarg::tooling::generated_registration::native_provider_template(
            record,
            &executable,
            &supervisor,
            &present,
        )?;
        let prepared = super::native_provider::prepare(
            &work.planning.collector,
            &supervisor,
            &template,
            &work
                .directory
                .join(format!("provider-{index}.request.json")),
        )
        .await?;
        providers.push(prepared);
        operations.insert(
            record.collector.id.clone(),
            engarg::tooling::generated_registration::operation_contract(record)?,
        );
    }
    let request = json!({"schema":"engarg.ticket-qualification/1","analysis":work.analysis,"providers":providers,"operation_contracts":operations});
    let qualified = commands::qualification(
        &work.directory,
        argv,
        &request,
        args.qualification_timeout_secs,
    )
    .await?;
    let qualifications = work.directory.join("qualifications.json");
    retain(&qualifications, &qualified)?;
    let input_config = work.directory.join("planning.for-registration.json");
    retain(&input_config, &work.planning)?;
    let registered = work.directory.join("planning.registered.json");
    commands::engarg(
        &work.directory,
        "register-tools",
        vec![
            "eal".into(),
            "register-tools".into(),
            "--input".into(),
            work.analysis.to_string_lossy().into_owned(),
            "--qualifications".into(),
            qualifications.to_string_lossy().into_owned(),
            "--evidence-config".into(),
            input_config.to_string_lossy().into_owned(),
            "--output".into(),
            registered.to_string_lossy().into_owned(),
        ],
        args.timeout_secs,
    )
    .await?;
    work.planning = crate::read_json(&registered, 4 * 1_048_576)?;
    work.connections = crate::read_json(&work.planning.connections, 16 * 1_048_576)?;
    work.registered_tools = packages.keys().cloned().collect();
    Ok(())
}
