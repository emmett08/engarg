//! Bounded supervised subprocess calls with private exact output receipts.
use engarg::Error;
use std::{collections::BTreeMap, path::Path, time::Duration};

pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
}

pub fn path(path: &Path) -> Result<String, Error> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::InvalidInput("ticket paths require UTF-8".into()))
}

pub async fn invoke(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    seconds: u64,
    receipt: &Path,
) -> Result<Output, Error> {
    invoke_environment(
        executable,
        args,
        cwd,
        seconds,
        receipt,
        std::env::vars().collect(),
    )
    .await
}

pub async fn invoke_selected(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    seconds: u64,
    receipt: &Path,
    names: &[String],
) -> Result<Output, Error> {
    let mut environment = BTreeMap::new();
    for name in names {
        environment.insert(
            name.clone(),
            std::env::var(name).map_err(|_| {
                Error::InvalidInput("ticket environment variable unavailable".into())
            })?,
        );
    }
    invoke_environment(executable, args, cwd, seconds, receipt, environment).await
}
async fn invoke_environment(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    seconds: u64,
    receipt: &Path,
    environment: BTreeMap<String, String>,
) -> Result<Output, Error> {
    let supervisor = crate::tooling_commands::supervisor()?;
    let request = eal_process::CommandRequest {
        argv: std::iter::once(path(executable)?)
            .chain(args.iter().cloned())
            .collect(),
        environment,
        cwd: cwd.to_owned(),
        input: Vec::new(),
        timeout: Duration::from_secs(seconds),
        max_output_bytes: 16 * 1_048_576,
    };
    let cancellation = eal_process::CancellationToken::default();
    struct Guard(eal_process::CancellationToken);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let _guard = Guard(cancellation.clone());
    let outcome = tokio::task::spawn_blocking(move || {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            request,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: 16 * 1_048_576,
                max_stderr_bytes: 65_536,
            },
        )
    })
    .await
    .map_err(|_| Error::Provider("ticket command worker failed".into()))?
    .map_err(|_| Error::Provider("ticket command could not start".into()))?;
    use std::io::Write;
    for (extension, bytes) in [("stdout", &outcome.stdout), ("stderr", &outcome.stderr)] {
        let mut file = crate::private_file(&receipt.with_extension(extension))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    Ok(Output {
        success: outcome.returncode == 0 && outcome.execution_error.is_none(),
        stdout: outcome.stdout,
    })
}
