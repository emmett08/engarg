//! Publish the standalone EAL source, direct TOML and implementation package.
use super::*;

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("export");
    let mut args = vec![
        "build".into(),
        "--input".into(),
        process::path(&previous(request, "analysis_ready")?)?,
        "--tools".into(),
        process::path(&config.tools)?,
        "--output-dir".into(),
        process::path(&dir)?,
    ];
    if let Some(path) = &config.export_policy {
        args.extend(["--export-policy".into(), process::path(path)?]);
    }
    let output = process::invoke(
        &std::env::current_exe()?,
        &args,
        &request.workspace,
        config.stage_timeout_seconds,
        &request.workspace.join("export-command"),
    )
    .await?;
    let mut artifacts = vec![];
    for (kind, name) in [
        ("source", "case.eal"),
        ("tools", "tools.toml"),
        ("build", "build.json"),
    ] {
        let path = dir.join(name);
        if path.is_file() {
            artifacts.push(artifact(kind, &path)?);
        }
    }
    Ok(ResultData {
        artifacts,
        completed: output.success,
        outcome: json!({"phase":"export","completed":output.success}),
    })
}
