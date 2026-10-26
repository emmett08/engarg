//! Bounded Docker execution for operator-selected, immutable language toolchains.
use super::{
    contracts::{SourceFile, digest_json},
    execution_diagnostics::{bounded, feedback_redactions, redact_feedback},
    profile::ToolchainProfile,
};
use crate::Error;
use eal_process::{CancellationToken, CommandRequest, StreamLimits, Supervisor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Host settings used by the trusted Docker client; these are not container grants.
pub const DOCKER_ENVIRONMENT: &[&str] = &[
    "PATH",
    "HOME",
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
    "DOCKER_CERT_PATH",
    "DOCKER_TLS_VERIFY",
    "DOCKER_API_VERSION",
    "SSH_AUTH_SOCK",
    "XDG_RUNTIME_DIR",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "USER",
];
static NEXT_CONTAINER: AtomicU64 = AtomicU64::new(0);

mod lifecycle;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerIdentity {
    pub profile_sha256: String,
    pub image_id: String,
    pub docker_sha256: String,
    pub runner_sha256: String,
    pub supervisor_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Fixture,
    Runtime,
}

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub returncode: i32,
    pub execution_error: Option<String>,
    pub elapsed_ms: u64,
}

pub trait FixtureRunner: Send + Sync {
    fn identity(&self) -> &RunnerIdentity;
    fn execute(
        &self,
        package_root: &Path,
        source_files: &[SourceFile],
        fixture_files: &[SourceFile],
        request_bytes: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error>;
}

pub struct DockerRunner {
    profile: ToolchainProfile,
    identity: RunnerIdentity,
    docker: PathBuf,
    supervisor: Supervisor,
    runner: PathBuf,
}

impl DockerRunner {
    /// Admission resolves an already-installed image. No generation or pulling occurs.
    pub fn new(profile: ToolchainProfile, supervisor: PathBuf) -> Result<Self, Error> {
        profile.validate()?;
        let docker = resolve_executable(&profile.docker)?;
        let supervisor = supervisor
            .canonicalize()
            .map_err(|_| invalid("collector supervisor is unavailable"))?;
        let runner = std::env::current_exe()?.canonicalize()?;
        let mut identity = RunnerIdentity {
            profile_sha256: digest_json(&profile)?,
            image_id: String::new(),
            docker_sha256: executable_digest(&docker)?,
            runner_sha256: executable_digest(&runner)?,
            supervisor_sha256: executable_digest(&supervisor)?,
        };
        let supervisor = Supervisor::new(supervisor);
        let request = command_request(
            &docker,
            &[
                "image".into(),
                "inspect".into(),
                "--format".into(),
                "{{.Id}}".into(),
                profile.image.clone(),
            ],
            std::env::current_dir()?,
            Vec::new(),
            Duration::from_secs(15),
            16_384,
            host_environment()?,
        );
        let outcome = supervisor.run(request, None).map_err(|error| {
            execution_failure(
                &profile,
                &format!("Docker image admission could not run: {error}"),
            )
        })?;
        if outcome.returncode != 0 || outcome.execution_error.is_some() {
            return Err(execution_failure(
                &profile,
                &format!(
                    "Docker image admission failed; selected toolchain image must already be installed and Docker must be available; exit status {}; execution error {}; stderr: {}",
                    outcome.returncode,
                    outcome
                        .execution_error
                        .as_ref()
                        .map_or("none", |error| error.label()),
                    String::from_utf8_lossy(&outcome.stderr),
                ),
            ));
        }
        let image_id = std::str::from_utf8(&outcome.stdout)
            .map_err(|_| invalid("Docker returned an invalid image identity"))?
            .trim();
        if !valid_image_id(image_id) {
            return Err(invalid("Docker returned an invalid image identity"));
        }
        identity.image_id = image_id.into();
        Ok(Self {
            profile,
            identity,
            docker,
            supervisor,
            runner,
        })
    }

    pub fn profile(&self) -> &ToolchainProfile {
        &self.profile
    }

    pub fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }

    fn check_executables(&self) -> Result<(), Error> {
        if executable_digest(&self.docker)? != self.identity.docker_sha256
            || executable_digest(&self.runner)? != self.identity.runner_sha256
            || executable_digest(self.supervisor.executable())? != self.identity.supervisor_sha256
            || digest_json(&self.profile)? != self.identity.profile_sha256
        {
            return Err(invalid(
                "toolchain executable identity changed after admission",
            ));
        }
        Ok(())
    }

    fn cleanup(&self, name: &str, cwd: &Path) -> Result<(), Error> {
        // This independent token and deadline remain usable after stage cancellation.
        let outcome = self
            .supervisor
            .run(
                command_request(
                    &self.docker,
                    &[
                        "container".into(),
                        "rm".into(),
                        "--force".into(),
                        name.into(),
                    ],
                    cwd.to_path_buf(),
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
                    &format!("collector container cleanup failed: {error}"),
                )
            })?;
        if outcome.execution_error.is_some() {
            return Err(execution_failure(
                &self.profile,
                &format!(
                    "collector container cleanup did not complete; exit status {}; execution error {}; stderr: {}",
                    outcome.returncode,
                    outcome
                        .execution_error
                        .as_ref()
                        .map_or("none", |error| error.label()),
                    String::from_utf8_lossy(&outcome.stderr),
                ),
            ));
        }
        // A failed start has no container. Other daemon failures cannot prove cleanup.
        if outcome.returncode != 0
            && !String::from_utf8_lossy(&outcome.stderr).contains("No such container")
        {
            return Err(execution_failure(
                &self.profile,
                &format!(
                    "collector container cleanup did not confirm termination; exit status {}; stderr: {}",
                    outcome.returncode,
                    String::from_utf8_lossy(&outcome.stderr),
                ),
            ));
        }
        Ok(())
    }
}

impl FixtureRunner for DockerRunner {
    fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }
    fn execute(
        &self,
        package_root: &Path,
        source_files: &[SourceFile],
        fixture_files: &[SourceFile],
        request_bytes: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        self.execute_snapshot(
            package_root,
            source_files,
            fixture_files,
            request_bytes,
            mode,
            cancellation,
        )
    }
}

fn container_script(profile: &ToolchainProfile) -> String {
    let run = argv_shell(&profile.run_argv);
    let worker = if profile.build_argv.is_empty() {
        format!("exec {run}")
    } else {
        format!("{} 1>&2 && exec {run}", argv_shell(&profile.build_argv))
    };
    // A PID1 watchdog additionally bounds ordinary worker failures. Generated
    // code shares its UID, so supervisor cancellation and forced daemon cleanup
    // remain the authoritative boundary for untrusted execution.
    // Explicit stdin redirection is required for a POSIX background command.
    format!(
        "umask 077\ntrap 'exit 124' TERM\nparent=$$\nexec 3<&0\n(exec 3<&-; {worker}) <&3 &\nworker=$!\nexec 3<&-\n(sleep {}; kill -TERM \"$parent\") &\nwatchdog=$!\nwait \"$worker\"\nstatus=$?\nkill \"$watchdog\" 2>/dev/null || :\nexit \"$status\"\n",
        profile.timeout_seconds
    )
}

fn argv_shell(argv: &[String]) -> String {
    argv.iter()
        .map(|value| format!("'{}'", value.replace("'", "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn container_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "engarg-{}-{stamp}-{}",
        std::process::id(),
        NEXT_CONTAINER.fetch_add(1, Ordering::Relaxed)
    )
}

fn command_request(
    docker: &Path,
    args: &[String],
    cwd: PathBuf,
    input: Vec<u8>,
    timeout: Duration,
    max_output_bytes: usize,
    environment: BTreeMap<String, String>,
) -> CommandRequest {
    let mut argv = vec![docker.to_string_lossy().into_owned()];
    argv.extend_from_slice(args);
    CommandRequest {
        argv,
        environment,
        cwd,
        input,
        timeout,
        max_output_bytes,
    }
}

fn host_environment() -> Result<BTreeMap<String, String>, Error> {
    let mut values = BTreeMap::new();
    for name in DOCKER_ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            let value = value
                .into_string()
                .map_err(|_| invalid("Docker environment must use UTF-8"))?;
            if value.len() > 16_384 {
                return Err(invalid("Docker environment exceeds its byte limit"));
            }
            values.insert((*name).into(), value);
        }
    }
    values.insert("LC_ALL".into(), "C".into());
    Ok(values)
}

fn valid_image_id(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn resolve_executable(value: &str) -> Result<PathBuf, Error> {
    let selected = if Path::new(value).components().count() > 1 || Path::new(value).is_absolute() {
        PathBuf::from(value)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(value))
            .find(|path| path.is_file())
            .ok_or_else(|| invalid("configured Docker executable was not found"))?
    };
    let physical = selected
        .canonicalize()
        .map_err(|_| invalid("configured executable is unavailable"))?;
    if physical.to_str().is_none() {
        return Err(invalid("configured executable requires a UTF-8 path"));
    }
    Ok(physical)
}

/// Stream executable identities without allocating the complete debug binary.
pub fn executable_digest(path: &Path) -> Result<String, Error> {
    const LIMIT: u64 = 1_073_741_824;
    let physical = path
        .canonicalize()
        .map_err(|_| invalid("runner executable is unavailable"))?;
    let mut file = secure_open(&physical)
        .map_err(|_| invalid("runner executable requires a stable regular file"))?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() == 0 || before.len() > LIMIT {
        return Err(invalid("runner executable exceeds regular-file limits"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if before.permissions().mode() & 0o111 == 0 {
            return Err(invalid("runner executable is not executable"));
        }
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65_536];
    let mut read = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read = read.saturating_add(count as u64);
        if read > LIMIT {
            return Err(Error::Budget(
                "runner executable exceeds its byte limit".into(),
            ));
        }
        digest.update(&buffer[..count]);
    }
    let after_open = file.metadata()?;
    let after_path = fs::symlink_metadata(&physical)?;
    if !same_file(&before, &after_open)
        || !same_file(&before, &after_path)
        || before.len() != read
        || before.len() != after_open.len()
        || before.modified().ok() != after_open.modified().ok()
        || before.modified().ok() != after_path.modified().ok()
    {
        return Err(invalid(
            "runner executable changed during identity inspection",
        ));
    }
    Ok(hex_bytes(&digest.finalize()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .concat()
}

fn same_file(before: &Metadata, after: &Metadata) -> bool {
    if !after.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        before.dev() == after.dev() && before.ino() == after.ino()
    }
    #[cfg(not(unix))]
    {
        before.len() == after.len() && before.modified().ok() == after.modified().ok()
    }
}

#[cfg(unix)]
fn secure_open(path: &Path) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::path::Component;
    let mut parent = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let parts: Vec<_> = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect();
    let (leaf, directories) = parts
        .split_last()
        .ok_or_else(|| std::io::Error::other("regular file required"))?;
    for directory in directories {
        parent = openat(
            &parent,
            Path::new(directory),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
    }
    Ok(openat(
        &parent,
        Path::new(leaf),
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into())
}

#[cfg(not(unix))]
fn secure_open(path: &Path) -> std::io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("regular file required"));
    }
    File::open(path)
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn execution_failure(profile: &ToolchainProfile, message: &str) -> Error {
    Error::ToolExecution(bounded(
        &redact_feedback(message, &feedback_redactions(profile)),
        4096,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_arguments_cannot_add_commands() {
        assert_eq!(
            argv_shell(&["a'b".into(), "$(touch /tmp/no)".into()]),
            "'a'\\''b' '$(touch /tmp/no)'"
        );
        assert!(valid_image_id(&format!("sha256:{}", "a".repeat(64))));
        assert!(!valid_image_id("python:latest"));
    }

    #[test]
    fn installed_executable_hash_is_bounded_and_stable() {
        let executable = std::env::current_exe().unwrap();
        let first = executable_digest(&executable).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(first, executable_digest(&executable).unwrap());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-executable");
        fs::write(&path, b"data").unwrap();
        #[cfg(unix)]
        assert!(executable_digest(&path).is_err());
    }
}
