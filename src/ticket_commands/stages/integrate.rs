//! Activate approved evidence sources and verified implementations.
use super::*;

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("integrate");
    let mut args = vec![
        "eal".into(),
        "integrate".into(),
        "--input".into(),
        process::path(&previous(request, "analysis")?)?,
        "--evidence-config".into(),
        process::path(&config.evidence_config)?,
        "--output-dir".into(),
        process::path(&dir)?,
        "--timeout-secs".into(),
        config.stage_timeout_seconds.to_string(),
        "--implementation-args".into(),
        serde_json::to_string(&config.implementation_args)?,
    ];
    if let Some(path) = &config.mcp_registrations {
        args.extend(["--mcp-registrations".into(), process::path(path)?]);
    }
    if let Some(argv) = &config.qualification_argv {
        args.extend(["--qualification-argv".into(), serde_json::to_string(argv)?]);
    }
    let output = process::invoke(
        &std::env::current_exe()?,
        &args,
        &request.workspace,
        config.stage_timeout_seconds,
        &request.workspace.join("integrate-command"),
    )
    .await?;
    let mut artifacts = vec![];
    for (kind, name) in [
        ("analysis_ready", "analysis.ready.json"),
        ("planning", "planning.ready.json"),
        ("connections", "connections.ready.json"),
        ("registration", "registration.json"),
    ] {
        let path = dir.join(name);
        if path.is_file() {
            artifacts.push(artifact(kind, &path)?);
        }
    }
    let outcome = if dir.join("registration.json").is_file() {
        configuration::read::<Value>(&dir.join("registration.json"))?
    } else {
        json!({"phase":"integrate","reason":"integration_failed"})
    };
    let completed = output.success && outcome["status"] == "completed";
    Ok(ResultData {
        artifacts,
        outcome,
        completed,
    })
}
