//! Fixed domain stages whose outputs are admitted by the native ticket controller.
use super::{
    configuration::{self, Configuration, Snapshot},
    process,
};
use eal_coordination_contracts::ticket::{
    Artifact, STAGE_REQUEST_SCHEMA, STAGE_RESULT_SCHEMA, Stage, StageRequest, StageResult,
    StageStatus,
};
use engarg::{
    Error,
    eal_host::{HostConfig, invoke},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct ResultData {
    artifacts: Vec<Artifact>,
    outcome: Value,
    completed: bool,
}

fn artifact(kind: &str, path: &Path) -> Result<Artifact, Error> {
    let path = path.canonicalize()?;
    let bytes = engarg::bounded_file::read(&path, 16 * 1_048_576, "ticket artifact")?;
    Ok(Artifact {
        kind: kind.into(),
        path,
        sha256: configuration::digest(&bytes),
        bytes: bytes.len() as u64,
    })
}
fn previous(request: &StageRequest, kind: &str) -> Result<PathBuf, Error> {
    request
        .previous
        .iter()
        .find(|artifact| artifact.kind == kind)
        .map(|artifact| artifact.path.clone())
        .ok_or_else(|| Error::InvalidInput(format!("ticket stage requires {kind} artifact")))
}
fn directory(path: &Path) -> Result<(), Error> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.as_chunks::<2>()
        .0
        .iter()
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

mod author;
mod execute;
mod export;
mod integrate;

pub async fn execute() -> Result<(), Error> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(4 * 1_048_576 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1_048_576 {
        return Err(Error::Budget(
            "ticket stage input exceeds its byte bound".into(),
        ));
    }
    let value = eal_collector_contracts::strict_json_exact(&bytes, Default::default())
        .map_err(Error::InvalidInput)?;
    let request: StageRequest = serde_json::from_value(value)?;
    if request.schema_version != STAGE_REQUEST_SCHEMA {
        return Err(Error::InvalidInput(
            "unsupported ticket stage request".into(),
        ));
    }
    let spec = request.context["spec_path"]
        .as_str()
        .ok_or_else(|| Error::InvalidInput("ticket context lacks spec path".into()))?;
    let sha = request.context["spec_sha256"]
        .as_str()
        .ok_or_else(|| Error::InvalidInput("ticket context lacks spec identity".into()))?;
    let snapshot = configuration::load(Path::new(spec), sha)?;
    if request.ticket_id != snapshot.configuration.ticket_id {
        return Err(Error::InvalidInput("ticket stage identity changed".into()));
    }
    let result = match request.stage {
        Stage::Author => author::run(&request, &snapshot).await,
        Stage::Integrate => integrate::run(&request, &snapshot.configuration).await,
        Stage::Export => export::run(&request, &snapshot.configuration).await,
        Stage::Execute => execute::run(&request, &snapshot.configuration).await,
    }?;
    let response = StageResult {
        schema_version: STAGE_RESULT_SCHEMA.into(),
        ticket_id: request.ticket_id,
        stage: request.stage,
        status: if result.completed {
            StageStatus::Completed
        } else {
            StageStatus::Blocked
        },
        artifacts: result.artifacts,
        outcome: Some(result.outcome),
    };
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::InvalidInput("system clock precedes epoch".into()))?
        .as_nanos();
    let envelope = json!({"response":response,"execution":{"execution_id":format!("engarg-stage:{}:{epoch}:{}",request.call_id,std::process::id()),"model":null,"usage":null}});
    serde_json::to_writer(std::io::stdout().lock(), &envelope)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
