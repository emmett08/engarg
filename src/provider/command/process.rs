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
    let outcome = tokio::task::spawn_blocking(move || {
        // Keep the directory alive even when the async caller is cancelled.
        // Its removal follows supervised descendant cleanup in this worker.
        let _directory = directory;
        // Admission belongs to the supervised lifetime, including cancellation
        // cleanup that can outlive the async caller's deadline.
        let _permit = permit;
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            request,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: max_output_bytes,
                max_stderr_bytes: STDERR_LIMIT,
            },
        )
    })
    .await
    .map_err(|_| Error::Provider("assistant command worker failed; no automatic retry".into()))?
    .map_err(|_| Error::Provider("assistant command could not start; no automatic retry".into()))?;
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
