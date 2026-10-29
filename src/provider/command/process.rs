//! Supervised assistant process lifetime, cancellation and independent stream bounds.
use super::Invocation;
use crate::Error;
use std::{collections::BTreeMap, path::Path, time::Duration};

pub(super) const STDERR_LIMIT: usize = 65_536;

pub(super) fn timeout_error() -> Error {
    Error::Provider("assistant logical-call deadline expired; no automatic retry; cancellation does not confirm remote completion or billing".into())
}

struct CancellationGuard(eal_process::CancellationToken);
impl Drop for CancellationGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) async fn invoke(
    invocation: Invocation,
    environment: &BTreeMap<String, String>,
    supervisor: &Path,
    timeout: Duration,
    max_output_bytes: usize,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<Vec<u8>, Error> {
    let directory = tempfile::Builder::new()
        .prefix("engarg-assistant-")
        .tempdir()
        .map_err(|_| Error::Provider("assistant working directory could not be created".into()))?;
    let request = eal_process::CommandRequest {
        argv: invocation.argv,
        environment: environment.clone(),
        cwd: directory.path().to_owned(),
        input: invocation.input,
        timeout,
        max_output_bytes: max_output_bytes + STDERR_LIMIT,
    };
    let supervisor = supervisor.to_owned();
    let cancellation = eal_process::CancellationToken::default();
    let _guard = CancellationGuard(cancellation.clone());
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker = std::thread::Builder::new()
        .name("engarg-assistant".into())
        .spawn(move || {
            let result = {
                // These owners can outlive the logical call and its runtime.
                // Release them after supervisor acknowledgement, before the
                // async caller can observe a completed result.
                let _permit = permit;
                let _directory = directory;
                run_retained(
                    supervisor,
                    request,
                    &cancellation,
                    eal_process::StreamLimits {
                        max_stdout_bytes: max_output_bytes,
                        max_stderr_bytes: STDERR_LIMIT,
                    },
                )
            };
            let _ = sender.send(result);
        })
        .map_err(|_| {
            Error::Provider("assistant command worker could not start; no automatic retry".into())
        })?;
    // A model semaphore bounds these dedicated workers. Runtime shutdown must
    // not join a supervisor that is still holding its cleanup capability.
    drop(worker);
    let outcome = receiver
        .await
        .map_err(|_| Error::Provider("assistant command worker failed; no automatic retry".into()))?
        .map_err(|_| {
            Error::Provider("assistant command could not start; no automatic retry".into())
        })?;
    if let Some(error) = outcome.execution_error {
        return Err(match error {
            eal_process::ExecutionError::Timeout => timeout_error(),
            eal_process::ExecutionError::OutputLimit => {
                Error::Budget("assistant output exceeds byte limit; no automatic retry".into())
            }
            eal_process::ExecutionError::Cancelled => {
                Error::Provider("assistant command cancelled; no automatic retry".into())
            }
            _ => Error::Provider("assistant command execution failed; no automatic retry".into()),
        });
    }
    if outcome.returncode != 0 {
        return Err(Error::Provider(
            "assistant command exited unsuccessfully; no automatic retry".into(),
        ));
    }
    Ok(outcome.stdout)
}

/// A process result can precede supervisor exit after a protocol or cleanup
/// error. Keep ownership until the supervisor also closes its retention FD.
fn run_retained(
    supervisor: std::path::PathBuf,
    request: eal_process::CommandRequest,
    cancellation: &eal_process::CancellationToken,
    limits: eal_process::StreamLimits,
) -> Result<eal_process::ProcessOutcome, eal_process::ProcessError> {
    #[cfg(unix)]
    {
        let retained = SupervisionLease::new()?;
        let result = eal_process::Supervisor::new(supervisor)
            .run_with_stream_limits_and_cancellation(
                request,
                Some(&retained),
                cancellation,
                limits,
            );
        drop(retained);
        result
    }
    #[cfg(not(unix))]
    {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            request,
            None,
            cancellation,
            limits,
        )
    }
}

#[cfg(unix)]
struct SupervisionLease {
    shutdown: std::os::unix::net::UnixStream,
    retained: Option<std::os::unix::net::UnixStream>,
}

#[cfg(unix)]
impl SupervisionLease {
    fn new() -> std::io::Result<Self> {
        let (shutdown, retained) = std::os::unix::net::UnixStream::pair()?;
        Ok(Self {
            shutdown,
            retained: Some(retained),
        })
    }
}

#[cfg(unix)]
impl eal_process::Retention for SupervisionLease {
    fn duplicate_for_supervisor(&self) -> std::io::Result<std::process::Stdio> {
        // UnixStream descriptors are close-on-exec. Only the explicit duplicate
        // supplied as the supervisor's retention FD survives its exec. The
        // supervisor replaces all command stdio, so the assistant cannot hold it.
        Ok(std::process::Stdio::from(std::os::fd::OwnedFd::from(
            self.retained
                .as_ref()
                .ok_or_else(|| std::io::Error::other("supervision lease is closing"))?
                .try_clone()?,
        )))
    }
}

#[cfg(unix)]
impl Drop for SupervisionLease {
    fn drop(&mut self) {
        use std::io::Read;
        self.retained.take();
        let mut buffer = [0u8; 1024];
        loop {
            match self.shutdown.read(&mut buffer) {
                Ok(0) => return,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                // A read error does not acknowledge supervisor shutdown.
                // Retain the permit and directory while waiting for EOF.
                Err(_) => loop {
                    std::thread::park();
                },
            }
        }
    }
}
