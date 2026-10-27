//! Resolve direct bindings and implement explicit operations within operator authority.
use super::*;

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("integrate");
    directory(&dir)?;
    let source = previous(request, "analysis")?;
    let raw: Value = configuration::read(&source)?;
    let mut analysis = engarg::report::read_direct(&raw)?;
    let mut tools = engarg::package::merge_tool_exports(&config.tools, &[], &dir.join("bindings"))?;
    let mut plan = engarg::direct_planning::DirectPlanner::from_tools(&tools)?.resolve_operations(
        &analysis.argument,
        analysis
            .support_elaboration
            .as_ref()
            .expect("validated direct report"),
    )?;
    let mut artifacts = Vec::new();
    let mut implementation_succeeded = true;
    if !plan.unbound_operations.is_empty()
        && let Some(settings) = &config.implementation
    {
        let output = dir.join("implementation");
        let mut arguments = vec![
            "eal".into(),
            "implement".into(),
            "--input".into(),
            process::path(&source)?,
            "--tools".into(),
            process::path(&config.tools)?,
            "--eal-host".into(),
            process::path(&config.eal_host)?,
            "--tool-language".into(),
            settings.language.clone(),
            "--toolchain".into(),
            process::path(&settings.toolchain)?,
            "--tool-store".into(),
            process::path(&settings.store)?,
            "--tool-output-dir".into(),
            process::path(&output)?,
            "--tool-max-calls".into(),
            settings.max_calls.to_string(),
            "--tool-timeout-secs".into(),
            settings.timeout_seconds.to_string(),
        ];
        arguments.extend(settings.provider_args.clone());
        let receipt = dir.join("implementation-command");
        let result = process::invoke_selected(
            &std::env::current_exe()?,
            &arguments,
            &request.workspace,
            config.stage_timeout_seconds,
            &receipt,
            &config.environment,
        )
        .await?;
        implementation_succeeded = result.success;
        for (kind, extension) in [
            ("implementation_stdout", "stdout"),
            ("implementation_stderr", "stderr"),
        ] {
            artifacts.push(artifact(kind, &receipt.with_extension(extension))?);
        }
        let implemented = output.join("analysis.with-tools.json");
        if result.success && implemented.is_file() {
            analysis = engarg::report::read_direct(&configuration::read(&implemented)?)?;
            tools = analysis
                .tooling
                .as_ref()
                .ok_or_else(|| {
                    Error::InvalidOutput("implemented report lacks verified tool metadata".into())
                })?
                .tools
                .clone();
            plan = engarg::direct_planning::DirectPlanner::from_tools(&tools)?.resolve_operations(
                &analysis.argument,
                analysis
                    .support_elaboration
                    .as_ref()
                    .expect("validated direct report"),
            )?;
            artifacts.push(artifact("implemented_analysis", &implemented)?);
        }
    }
    let completed = implementation_succeeded && plan.unbound_operations.is_empty();
    let outcome = json!({
        "phase":"integrate", "complete":completed,
        "reason":if completed {"direct_bindings_ready"} else if !implementation_succeeded {"operation_implementation_failed"} else {"required_operation_unbound"},
        "direct_bindings":plan,
    });
    configuration::write_json(&dir.join("integration.json"), &outcome)?;
    artifacts.push(artifact("integration", &dir.join("integration.json"))?);
    if completed {
        analysis.direct_bindings = Some(plan);
        configuration::write_json(&dir.join("analysis.ready.json"), &analysis)?;
        artifacts.push(artifact(
            "analysis_ready",
            &dir.join("analysis.ready.json"),
        )?);
        artifacts.push(artifact("integrated_tools", &tools)?);
    }
    Ok(ResultData {
        artifacts,
        outcome,
        completed,
    })
}
