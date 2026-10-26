//! Native coordinator transport with the shared session as its exact protocol checker.

use std::{process::Stdio, time::Duration};

use eal_coordination_contracts::{
    ClientMessage, Dispatch, MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES, RoundResult, SCHEMA_VERSION,
    ServerMessage, Session, SessionSpec,
};
use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use super::RunConfig;
use crate::Error;

const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(30);

/// The native adapter remains mandatory once configured. The in-process variant
/// supports library use and tests with the same routing and transition contract.
pub(super) enum Coordinator {
    InProcess(Session),
    Native(Box<NativeCoordinator>),
}

impl Coordinator {
    pub(super) async fn start(config: &RunConfig) -> Result<Self, Error> {
        let debate = config.debate.clone().ok_or_else(|| {
            Error::InvalidInput("EAL debate coordination requires a topology".into())
        })?;
        let spec = SessionSpec {
            schema_version: SCHEMA_VERSION.into(),
            debate,
            max_payload_bytes: config.max_input_bytes.min(MAX_PAYLOAD_BYTES),
        };
        let session = Session::new(spec.clone()).map_err(Error::from)?;
        let Some(native) = &config.coordinator else {
            return Ok(Self::InProcess(session));
        };
        let mut command = Command::new(&native.executable);
        command.env_clear();
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        command
            .arg("debate-session")
            .arg("--run")
            .arg(&native.run_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            transport_error(format!("cannot start executable ({})", error.kind()))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| transport_error("stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| transport_error("stdout unavailable"))?;
        let mut adapter = NativeCoordinator {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            contract: session,
            completed: false,
        };
        adapter.send(&ClientMessage::Start { spec }).await?;
        Ok(Self::Native(Box::new(adapter)))
    }

    pub(super) async fn dispatch(&mut self) -> Result<Option<Dispatch>, Error> {
        match self {
            Self::InProcess(session) => {
                if session.done() {
                    Ok(None)
                } else {
                    session.dispatch().map(Some).map_err(Error::from)
                }
            }
            Self::Native(adapter) => adapter.dispatch().await,
        }
    }

    pub(super) async fn accept(&mut self, result: RoundResult) -> Result<(), Error> {
        match self {
            Self::InProcess(session) => session.accept(result).map_err(Error::from),
            Self::Native(adapter) => {
                adapter
                    .contract
                    .accept(result.clone())
                    .map_err(Error::from)?;
                adapter.send(&ClientMessage::Results { result }).await
            }
        }
    }
}

pub(super) struct NativeCoordinator {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    contract: Session,
    completed: bool,
}

impl NativeCoordinator {
    async fn dispatch(&mut self) -> Result<Option<Dispatch>, Error> {
        if self.completed {
            return Ok(None);
        }
        let expected = if self.contract.done() {
            None
        } else {
            Some(self.contract.dispatch().map_err(Error::from)?)
        };
        match self.receive().await? {
            ServerMessage::Dispatch { dispatch } => {
                if expected.as_ref() != Some(&dispatch) {
                    return Err(transport_error(
                        "dispatch differs from the admitted session contract",
                    ));
                }
                Ok(Some(dispatch))
            }
            ServerMessage::Completed { rounds, calls } => {
                if expected.is_some()
                    || rounds != self.contract.rounds()
                    || calls != self.contract.completed_calls()
                {
                    return Err(transport_error(
                        "completion counts differ from accepted complete rounds",
                    ));
                }
                if let Some(mut stdin) = self.stdin.take() {
                    bounded_io(stdin.shutdown(), "closing stdin").await?;
                }
                // A completed session emits exactly one terminal frame and exits.
                let trailing =
                    bounded_frame(&mut self.stdout, "waiting for stdout closure").await?;
                if trailing.is_some() {
                    return Err(transport_error("unexpected frame after completion"));
                }
                let status = bounded_io(self.child.wait(), "waiting for process exit").await?;
                if !status.success() {
                    return Err(transport_error(
                        "process exited unsuccessfully after completion",
                    ));
                }
                self.completed = true;
                Ok(None)
            }
            ServerMessage::Failed { .. } => {
                // The journal retains details. Foreign diagnostic text may contain
                // confidential application payloads and is never echoed here.
                Err(transport_error(
                    "session rejected; inspect the coordinator run journal",
                ))
            }
        }
    }

    async fn send(&mut self, message: &impl Serialize) -> Result<(), Error> {
        let mut frame = serde_json::to_vec(message)?;
        frame.push(b'\n');
        if frame.len() > MAX_FRAME_BYTES {
            return Err(Error::Budget(format!(
                "EAL coordinator outgoing frame uses {} bytes, exceeding {MAX_FRAME_BYTES}",
                frame.len()
            )));
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| transport_error("stdin already closed"))?;
        bounded_io(
            async {
                stdin.write_all(&frame).await?;
                stdin.flush().await
            },
            "writing a protocol frame",
        )
        .await
    }

    async fn receive(&mut self) -> Result<ServerMessage, Error> {
        let frame = bounded_frame(&mut self.stdout, "reading a protocol frame")
            .await?
            .ok_or_else(|| transport_error("stdout closed before session completion"))?;
        serde_json::from_slice(&frame).map_err(|error| {
            transport_error(format!(
                "invalid JSON protocol frame at line {} column {}",
                error.line(),
                error.column()
            ))
        })
    }
}

/// Bound before allocation, accept only complete newline-terminated frames and
/// distinguish clean EOF from an interrupted partial message.
async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, Error> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|error| {
            transport_error(format!(
                "I/O failure while reading stdout ({})",
                error.kind()
            ))
        })?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(transport_error(
                    "stdout closed inside a partial protocol frame",
                ))
            };
        }
        let end = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1);
        let count = end.unwrap_or(available.len());
        if count > MAX_FRAME_BYTES.saturating_sub(frame.len()) {
            return Err(Error::Budget(format!(
                "EAL coordinator incoming frame exceeds {MAX_FRAME_BYTES} bytes"
            )));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(Some(frame));
        }
    }
}

async fn bounded_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    phase: &str,
) -> Result<Option<Vec<u8>>, Error> {
    tokio::time::timeout(TRANSPORT_TIMEOUT, read_frame(reader))
        .await
        .map_err(|_| transport_error(format!("timed out while {phase}")))?
}

async fn bounded_io<T>(
    future: impl std::future::Future<Output = std::io::Result<T>>,
    phase: &str,
) -> Result<T, Error> {
    tokio::time::timeout(TRANSPORT_TIMEOUT, future)
        .await
        .map_err(|_| transport_error(format!("timed out while {phase}")))?
        .map_err(|error| transport_error(format!("I/O failure while {phase} ({})", error.kind())))
}

fn transport_error(message: impl std::fmt::Display) -> Error {
    Error::Provider(format!("EAL coordinator: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn complete_frames_are_separate_and_clean_eof_is_distinct() {
        let mut input = &b"{\"kind\":\"one\"}\n{\"kind\":\"two\"}\n"[..];
        assert_eq!(
            read_frame(&mut input).await.unwrap(),
            Some(b"{\"kind\":\"one\"}\n".to_vec())
        );
        assert_eq!(
            read_frame(&mut input).await.unwrap(),
            Some(b"{\"kind\":\"two\"}\n".to_vec())
        );
        assert_eq!(read_frame(&mut input).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_partial_frame_is_rejected_without_echoing_its_content() {
        let mut input = &b"{\"confidential\": \"payload\"}"[..];
        let error = read_frame(&mut input).await.unwrap_err();
        assert!(error.to_string().contains("partial protocol frame"));
        assert!(!error.to_string().contains("confidential"));
        assert!(!error.to_string().contains("payload"));
    }

    #[tokio::test]
    async fn the_frame_bound_includes_the_required_newline() {
        let mut bytes = vec![b' '; MAX_FRAME_BYTES];
        bytes.push(b'\n');
        let error = read_frame(&mut bytes.as_slice()).await.unwrap_err();
        assert!(matches!(error, Error::Budget(_)));
        bytes.pop();
        bytes.pop();
        bytes.push(b'\n');
        assert_eq!(
            read_frame(&mut bytes.as_slice())
                .await
                .unwrap()
                .unwrap()
                .len(),
            MAX_FRAME_BYTES
        );
    }

    #[cfg(unix)]
    async fn rejected_process(mode: &str) -> Error {
        use crate::engine::{
            CoordinatorConfig,
            topology::{DebateConfig, Topology},
        };
        let temp = tempfile::tempdir().unwrap();
        // Execute a checked-in fixture. Launching a freshly written executable
        // can fail with ETXTBSY while parallel process creation holds a writer.
        let executable = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rejected-coordinator.sh");
        let config = RunConfig {
            debate: Some(DebateConfig {
                participants: 3,
                rounds: 1,
                topology: Topology::Ring {},
                adaptation: Default::default(),
            }),
            coordinator: Some(CoordinatorConfig {
                executable,
                run_directory: temp.path().join(mode),
            }),
            max_calls: 9,
            ..RunConfig::default()
        };
        let mut coordinator = Coordinator::start(&config).await.unwrap();
        coordinator.dispatch().await.unwrap_err()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_eof_and_partial_frames_fail_without_fallback() {
        assert!(
            rejected_process("eof")
                .await
                .to_string()
                .contains("stdout closed before")
        );
        assert!(
            rejected_process("partial")
                .await
                .to_string()
                .contains("partial protocol frame")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_terminal_or_misrouted_frames_fail_contract_checks() {
        let terminal = rejected_process("terminal").await;
        assert!(terminal.to_string().contains("completion counts"));
        let dispatch = rejected_process("dispatch").await;
        assert!(dispatch.to_string().contains("dispatch differs"));
        let failed = rejected_process("failed").await;
        assert!(failed.to_string().contains("session rejected"));
        assert!(!failed.to_string().contains("confidential"));
    }
}
