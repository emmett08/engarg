//! Domain adapter from durable EAL tickets to existing authoring and evidence commands.
mod configuration;
mod process;
mod stages;

use clap::Args;
use eal_coordination_contracts::ticket::{Artifact, TICKET_SCHEMA, TicketSpec};
use engarg::Error;
use serde_json::json;
use std::path::PathBuf;

#[derive(Args)]
pub struct TicketArgs {
    /// Operator-owned engarg.ticket/1 configuration.
    #[arg(long)]
    pub spec: PathBuf,
    /// New private workspace retaining every stage and artifact.
    #[arg(long)]
    pub run: PathBuf,
    /// Installed native coordinator. Defaults to the host's sibling executable.
    #[arg(long)]
    pub eal_coordinate: Option<PathBuf>,
}

pub async fn run(args: TicketArgs) -> Result<(), Error> {
    let frozen = configuration::prepare(args)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let mut inputs = frozen.inputs;
    inputs.push(Artifact {
        kind: "ticket_configuration".into(),
        path: frozen.snapshot.clone(),
        sha256: frozen.snapshot_sha256.clone(),
        bytes: frozen.snapshot_bytes,
    });
    let spec = TicketSpec {
        schema_version: TICKET_SCHEMA.into(),
        ticket_id: frozen.configuration.ticket_id.clone(),
        workspace: frozen.workspace.clone(),
        worker_argv: vec![process::path(&executable)?, "ticket-stage".into()],
        environment: frozen.configuration.environment.clone(),
        timeout_seconds: frozen.configuration.timeout_seconds,
        stage_timeout_seconds: frozen.configuration.stage_timeout_seconds,
        context: json!({"spec_path": frozen.snapshot, "spec_sha256": frozen.snapshot_sha256}),
        inputs,
    };
    let path = frozen.workspace.join("ticket-spec.json");
    configuration::write_json(&path, &spec)?;
    let output = process::invoke_selected(
        &frozen.coordinator,
        &[
            "ticket-run".into(),
            "--spec".into(),
            process::path(&path)?,
            "--run".into(),
            process::path(&frozen.workspace.join("journal"))?,
        ],
        &frozen.workspace,
        frozen.configuration.timeout_seconds.saturating_add(5),
        &frozen.workspace.join("coordinator"),
        &frozen.configuration.environment,
    )
    .await?;
    let report: serde_json::Value = eal_collector_contracts::strict_json_exact(
        &output.stdout,
        eal_collector_contracts::JsonLimits::default(),
    )
    .map_err(Error::InvalidOutput)?;
    configuration::write_json(&frozen.workspace.join("ticket-result.json"), &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if output.success && report["status"] == "completed" {
        Ok(())
    } else {
        Err(Error::Ticket(
            "ticket is blocked, failed or uncertain; inspect its retained result and journal"
                .into(),
        ))
    }
}

pub async fn stage() -> Result<(), Error> {
    stages::execute().await
}
