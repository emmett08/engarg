//! Publish the executable EAL source, registry and bundle.

use super::*;

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("export");
    directory(&dir)?;
    let source = dir.join("argument.eal");
    let tools = dir.join("argument.tools.toml");
    let bundle = dir.join("argument.bundle.json");
    let mut args = vec![
        "eal".into(),
        "export".into(),
        "--input".into(),
        process::path(&previous(request, "analysis_ready")?)?,
        "--output".into(),
        process::path(&source)?,
        "--tools-output".into(),
        process::path(&tools)?,
        "--manifest".into(),
        process::path(&bundle)?,
    ];
    if let Some(path) = &config.export_policy {
        args.extend(["--export-policy".into(), process::path(path)?]);
    }
    if let Some(path) = &config.review_binding {
        args.extend(["--review-binding".into(), process::path(path)?]);
    }
    let output = process::invoke(
        &std::env::current_exe()?,
        &args,
        &request.workspace,
        config.stage_timeout_seconds,
        &dir.join("command"),
    )
    .await?;
    let mut artifacts = vec![];
    for (kind, path) in [("source", source), ("tools", tools), ("bundle", bundle)] {
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
