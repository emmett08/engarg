//! Author a schema-checked argument through the selected native topology.
use super::*;

pub(super) async fn run(request: &StageRequest, snapshot: &Snapshot) -> Result<ResultData, Error> {
    let config = &snapshot.configuration;
    let dir = request.workspace.join("author");
    directory(&dir)?;
    let input = dir.join("request.json");
    let topology = dir.join("debate.json");
    let analysis = dir.join("analysis.json");
    configuration::write_json(&input, &config.request)?;
    configuration::write_json(&topology, &config.debate)?;
    let mut args = vec![
        "analyse".into(),
        "--input".into(),
        process::path(&input)?,
        "--evidence-planning".into(),
        "direct".into(),
        "--tools".into(),
        process::path(&config.tools)?,
        "--topology-config".into(),
        process::path(&topology)?,
        "--eal-coordinate".into(),
        process::path(&snapshot.coordinator)?,
        "--coordination-run".into(),
        process::path(&dir.join("debate-journal"))?,
        "--format".into(),
        "json".into(),
        "--output".into(),
        process::path(&analysis)?,
        "--audit".into(),
        process::path(&dir.join("authoring.audit.jsonl"))?,
    ];
    if option(&config.authoring_args, "--max-calls").is_none() {
        let repairs: u32 = option(&config.authoring_args, "--max-repairs")
            .unwrap_or_else(|| "0".into())
            .parse()
            .map_err(|_| Error::InvalidInput("invalid repair allowance".into()))?;
        args.extend([
            "--max-calls".into(),
            (config.debate.specialist_calls()? + 3 + repairs).to_string(),
        ]);
    }
    args.extend(config.authoring_args.clone());
    let output = process::invoke(
        &std::env::current_exe()?,
        &args,
        &request.workspace,
        config.stage_timeout_seconds,
        &dir.join("command"),
    )
    .await?;
    let artifacts = if analysis.is_file() {
        vec![artifact("analysis", &analysis)?]
    } else {
        vec![]
    };
    Ok(ResultData {
        artifacts,
        completed: output.success,
        outcome: json!({"phase":"author", "completed":output.success}),
    })
}
