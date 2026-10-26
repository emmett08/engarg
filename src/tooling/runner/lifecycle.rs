//! Transfer isolated inputs to the daemon and bound their complete execution lifecycle.
use super::*;
use crate::tooling::daemon_snapshot::DaemonSnapshot;
use std::time::Instant;

impl DockerRunner {
    pub(super) fn execute_snapshot(
        &self,
        package_root: &Path,
        source_files: &[SourceFile],
        fixture_files: &[SourceFile],
        request_bytes: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        if request_bytes.len() > 1_048_576 {
            return Err(Error::Budget("collector request exceeds 1 MiB".into()));
        }
        if cancellation.is_cancelled() {
            return Err(Error::ToolExecution(
                "collector execution was cancelled".into(),
            ));
        }
        self.check_executables()?;
        let cwd = package_root.canonicalize()?;
        let name = container_name();
        let started = Instant::now();
        let deadline = started + Duration::from_secs(self.profile.timeout_seconds + 2);
        let mut snapshot = DaemonSnapshot::new(&name, source_files, fixture_files)?;
        // Check host-only runtime values before creating daemon-owned resources.
        let (mut argv, environment) = self.worker_command(&name, mode)?;
        let mut staging_attempted = false;
        let mut worker_attempted = false;
        let execution = (|| {
            staging_attempted = true;
            self.setup_command(
                &snapshot.staging_argv(&self.identity.image_id),
                Vec::new(),
                &cwd,
                deadline,
                cancellation,
                "snapshot creation",
            )?;
            let mounts = self.setup_command(
                &snapshot.inspect_argv(),
                Vec::new(),
                &cwd,
                deadline,
                cancellation,
                "snapshot volume inspection",
            )?;
            snapshot.admit_mounts(&mounts)?;
            self.setup_command(
                &snapshot.source_copy_argv(),
                snapshot.source_archive().to_vec(),
                &cwd,
                deadline,
                cancellation,
                "source upload",
            )?;
            self.setup_command(
                &snapshot.fixture_copy_argv(),
                snapshot.fixture_archive().to_vec(),
                &cwd,
                deadline,
                cancellation,
                "fixture upload",
            )?;
            argv.extend(snapshot.worker_mount_args()?);
            argv.extend([
                self.identity.image_id.clone(),
                "-c".into(),
                container_script(&self.profile),
            ]);
            let remaining = remaining(deadline, cancellation)?;
            worker_attempted = true;
            self.supervisor
                .run_with_stream_limits_and_cancellation(
                    command_request(
                        &self.docker,
                        &argv,
                        cwd.clone(),
                        request_bytes.to_vec(),
                        remaining,
                        self.profile.output_bytes.saturating_mul(2),
                        environment,
                    ),
                    None,
                    cancellation,
                    StreamLimits {
                        max_stdout_bytes: self.profile.output_bytes,
                        max_stderr_bytes: self.profile.output_bytes,
                    },
                )
                .map_err(|error| {
                    execution_failure(
                        &self.profile,
                        &format!("bounded collector execution failed: {error}"),
                    )
                })
        })();
        // Cleanup has an independent finite allowance and attempts every owned
        // resource even when execution, cancellation or earlier cleanup failed.
        let mut failures = Vec::new();
        if worker_attempted && let Err(error) = self.cleanup(&name, &cwd) {
            failures.push(error.to_string());
        }
        if staging_attempted && let Err(error) = self.cleanup_snapshot(&snapshot, &cwd) {
            failures.push(error.to_string());
        }
        if !failures.is_empty() {
            let detail = match &execution {
                Ok(outcome) => format!(
                    "exit status {}; execution error {}; stderr: {}",
                    outcome.returncode,
                    outcome
                        .execution_error
                        .as_ref()
                        .map_or("none", |e| e.label()),
                    String::from_utf8_lossy(&outcome.stderr)
                ),
                Err(error) => error.to_string(),
            };
            return Err(execution_failure(
                &self.profile,
                &format!(
                    "collector execution {detail}; cleanup also failed: {}",
                    failures.join("; ")
                ),
            ));
        }
        let outcome = execution?;
        let execution_error = if cancellation.is_cancelled() {
            Some("cancelled".into())
        } else if let Some(error) = outcome.execution_error {
            Some(error.label().into())
        } else if outcome.returncode == 124 {
            Some("timeout".into())
        } else if outcome.returncode >= 125 || outcome.returncode < 0 {
            Some("container_execution_error".into())
        } else {
            None
        };
        Ok(RunOutcome {
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            returncode: outcome.returncode,
            execution_error,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    fn setup_command(
        &self,
        argv: &[String],
        input: Vec<u8>,
        cwd: &Path,
        deadline: Instant,
        cancellation: &CancellationToken,
        stage: &str,
    ) -> Result<Vec<u8>, Error> {
        let timeout = remaining(deadline, cancellation)?.min(Duration::from_secs(15));
        let outcome = self
            .supervisor
            .run_with_stream_limits_and_cancellation(
                command_request(
                    &self.docker,
                    argv,
                    cwd.to_owned(),
                    input,
                    timeout,
                    32_768,
                    host_environment()?,
                ),
                None,
                cancellation,
                StreamLimits {
                    max_stdout_bytes: 16_384,
                    max_stderr_bytes: 16_384,
                },
            )
            .map_err(|error| {
                execution_failure(&self.profile, &format!("{stage} failed: {error}"))
            })?;
        if outcome.returncode != 0 || outcome.execution_error.is_some() {
            return Err(execution_failure(
                &self.profile,
                &format!(
                    "{stage} failed; exit status {}; execution error {}; stderr: {}",
                    outcome.returncode,
                    outcome
                        .execution_error
                        .as_ref()
                        .map_or("none", |e| e.label()),
                    String::from_utf8_lossy(&outcome.stderr)
                ),
            ));
        }
        Ok(outcome.stdout)
    }

    fn cleanup_snapshot(&self, snapshot: &DaemonSnapshot, cwd: &Path) -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let ownership = self
            .supervisor
            .run(
                command_request(
                    &self.docker,
                    &snapshot.ownership_argv(),
                    cwd.to_owned(),
                    Vec::new(),
                    Duration::from_secs(5),
                    16_384,
                    host_environment()?,
                ),
                None,
            )
            .map_err(|error| {
                execution_failure(
                    &self.profile,
                    &format!("snapshot ownership check failed: {error}"),
                )
            })?;
        if ownership.execution_error.is_some() {
            return Err(execution_failure(
                &self.profile,
                "snapshot ownership check did not complete",
            ));
        }
        if ownership.returncode != 0
            && String::from_utf8_lossy(&ownership.stderr).contains("No such")
        {
            return Ok(());
        }
        if ownership.returncode != 0
            || String::from_utf8_lossy(&ownership.stdout).trim() != snapshot.expected_owner()
        {
            return Err(execution_failure(
                &self.profile,
                "snapshot ownership was not confirmed; existing container preserved",
            ));
        }
        let allowance = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                execution_failure(&self.profile, "snapshot cleanup deadline exceeded")
            })?;
        let outcome = self
            .supervisor
            .run(
                command_request(
                    &self.docker,
                    &snapshot.cleanup_argv(),
                    cwd.to_owned(),
                    Vec::new(),
                    allowance,
                    16_384,
                    host_environment()?,
                ),
                None,
            )
            .map_err(|error| {
                execution_failure(&self.profile, &format!("snapshot cleanup failed: {error}"))
            })?;
        if outcome.execution_error.is_some() || outcome.returncode != 0 {
            return Err(execution_failure(
                &self.profile,
                &format!(
                    "snapshot cleanup failed; exit status {}; stderr: {}",
                    outcome.returncode,
                    String::from_utf8_lossy(&outcome.stderr)
                ),
            ));
        }
        Ok(())
    }

    fn worker_command(
        &self,
        name: &str,
        mode: ExecutionMode,
    ) -> Result<(Vec<String>, BTreeMap<String, String>), Error> {
        let memory = format!("{}m", self.profile.memory_mb);
        let network = match mode {
            ExecutionMode::Fixture => "none",
            ExecutionMode::Runtime => self.profile.runtime_network.as_str(),
        };
        let mut argv: Vec<String> = [
            "run",
            "--rm",
            "--pull",
            "never",
            "--name",
            name,
            "--interactive",
            "--read-only",
            "--network",
            network,
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--user",
            "65534:65534",
            "--pids-limit",
            "64",
            "--cpus",
            "1",
            "--memory",
            &memory,
            "--memory-swap",
            &memory,
            "--log-driver",
            "none",
            "--no-healthcheck",
            "--workdir",
            "/build",
            "--entrypoint",
            "/bin/sh",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,nodev,size=64m,mode=1777",
            "--tmpfs",
            "/build:rw,exec,nosuid,nodev,size=128m,mode=1777",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let mut environment = host_environment()?;
        if mode == ExecutionMode::Runtime {
            for name in &self.profile.inherit_env {
                let value = std::env::var(name).map_err(|_| {
                    invalid("a required collector runtime environment variable is missing")
                })?;
                if value.is_empty() || value.len() > 16_384 {
                    return Err(invalid(
                        "collector runtime environment values must be bounded and nonempty",
                    ));
                }
                argv.extend(["--env".into(), name.clone()]);
                environment.insert(name.clone(), value);
            }
        }
        Ok((argv, environment))
    }
}

fn remaining(deadline: Instant, cancellation: &CancellationToken) -> Result<Duration, Error> {
    if cancellation.is_cancelled() {
        return Err(Error::ToolExecution(
            "collector execution was cancelled".into(),
        ));
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            Error::ToolExecution("collector snapshot and execution deadline exceeded".into())
        })
}
