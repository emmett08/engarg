//! Bounded one-request adapter to the official `eal-host` MCP client.
//!
//! `eal-host` performs MCP initialization, tool discovery and schema validation.
//! Its sibling `eal-mcp` executable supplies the stdio server. Targets belong to
//! operator configuration; argument generation must never choose an executable.

use crate::Error;
use serde_json::Value;
use std::{
    ffi::OsString,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

const MAX_INPUT: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
const CLEANUP_GRACE: Duration = Duration::from_secs(1);

#[derive(Default)]
struct RequestBuffer {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl Write for RequestBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_INPUT.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("host request byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Configuration for one official host invocation.
///
/// The deadline passed to the host covers the complete MCP exchange. The outer
/// process deadline adds one second so the host can cancel and close its server.
/// Beyond that deadline EngArg kills and reaps the host. Requests are not retried.
pub struct HostConfig {
    pub executable: PathBuf,
    pub workspace: PathBuf,
    pub database: Option<PathBuf>,
    /// Explicit local collector registry. Remote servers own their registry configuration.
    pub registry: Option<PathBuf>,
    /// Names of explicitly forwarded collector settings. Values never enter reports.
    pub collector_env: Vec<String>,
    pub url: Option<String>,
    pub token_env: Option<String>,
    pub timeout: Duration,
}

struct Target {
    workspace: PathBuf,
    database: Option<PathBuf>,
    registry: Option<PathBuf>,
    collector_env: Vec<(String, OsString)>,
    token: Option<OsString>,
}

impl HostConfig {
    fn validate(&self) -> Result<Target, Error> {
        if self.executable.as_os_str().is_empty() || self.executable.to_str().is_none() {
            return Err(invalid(
                "eal-host executable must be a non-empty UTF-8 path",
            ));
        }
        if self.timeout.is_zero() || self.timeout > MAX_TIMEOUT {
            return Err(invalid(
                "eal-host timeout must be positive and at most 3600 seconds",
            ));
        }
        let workspace = self
            .workspace
            .canonicalize()
            .map_err(|_| invalid("eal-host workspace must be an existing directory"))?;
        if !workspace.is_dir() || workspace.to_str().is_none() {
            return Err(invalid("eal-host workspace must be a UTF-8 directory"));
        }
        let database = self.database.as_deref().map(absolute_path).transpose()?;
        let registry = self
            .registry
            .as_deref()
            .map(|path| {
                let resolved = path
                    .canonicalize()
                    .map_err(|_| invalid("eal-host registry must be an existing file"))?;
                if !resolved.is_file() || resolved.to_str().is_none() {
                    return Err(invalid("eal-host registry must be a UTF-8 file path"));
                }
                Ok(resolved)
            })
            .transpose()?;
        if self.url.is_some() && registry.is_some() {
            return Err(invalid(
                "a local tools registry cannot configure a remote EAL server",
            ));
        }
        let collector_env = collector_environment(&self.collector_env, registry.is_some())?;
        if let Some(url) = &self.url {
            validate_url(url, self.token_env.is_some())?;
        } else if self.token_env.is_some() {
            return Err(invalid("token-env requires an HTTP MCP URL"));
        }
        let token = self
            .token_env
            .as_deref()
            .map(|name| {
                if !environment_name(name) {
                    return Err(invalid("token-env must name an environment variable"));
                }
                let value = std::env::var_os(name)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        invalid("the configured bearer token variable is missing or empty")
                    })?;
                if value
                    .to_str()
                    .is_none_or(|v| !v.bytes().all(|b| (33..=126).contains(&b)))
                {
                    return Err(invalid(
                        "the configured bearer token has invalid characters",
                    ));
                }
                Ok(value)
            })
            .transpose()?;
        Ok(Target {
            workspace,
            database,
            registry,
            collector_env,
            token,
        })
    }
}

fn collector_environment(
    names: &[String],
    local_registry: bool,
) -> Result<Vec<(String, OsString)>, Error> {
    if !names.is_empty() && !local_registry {
        return Err(invalid(
            "collector-env requires an explicit local tools registry",
        ));
    }
    let mut values = Vec::new();
    for name in names {
        if !environment_name(name)
            || !matches!(
                name.as_str(),
                "KUBECONFIG"
                    | "GH_TOKEN"
                    | "GH_HOST"
                    | "ENGARG_COLLECTOR_TOKEN"
                    | "ENGARG_COLLECTOR_ORIGIN"
            )
        {
            return Err(invalid("unsupported collector-env name"));
        }
        let value = std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid("a configured collector environment variable is missing or empty")
            })?;
        values.push((name.clone(), value));
    }
    Ok(values)
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.to_owned())
}

fn absolute_path(path: &Path) -> Result<PathBuf, Error> {
    if path.as_os_str().is_empty() || path.to_str().is_none() {
        return Err(invalid("eal-host database must be a non-empty UTF-8 path"));
    }
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|_| invalid("cannot resolve eal-host database path"))
    }
}

fn environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn validate_url(value: &str, authenticated: bool) -> Result<(), Error> {
    let invalid_url = || invalid("MCP URL must be HTTP(S), without credentials, query or fragment");
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid_url());
    }
    let url = reqwest::Url::parse(value).map_err(|_| invalid_url())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err(invalid_url());
    }
    let host = url.host_str().unwrap_or_default();
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !loopback && (url.scheme() != "https" || !authenticated) {
        return Err(invalid(
            "remote MCP connections require HTTPS and token-env authentication",
        ));
    }
    Ok(())
}

fn command(config: &HostConfig, target: &Target) -> Command {
    let mut command = Command::new(&config.executable);
    // The server must not inherit unrelated provider credentials or ambient
    // EAL_MCP_* configuration. Keep only runtime and certificate locations.
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in &target.collector_env {
        command.env(name, value);
    }
    command
        .arg("--workspace")
        .arg(&target.workspace)
        .args([
            "--exposure",
            "operator",
            "--methods",
            "builtin",
            "--timeout",
        ])
        .arg(config.timeout.as_secs_f64().to_string());
    if let Some(database) = &target.database {
        command.arg("--database").arg(database);
    }
    if let Some(registry) = &target.registry {
        command.arg("--registry").arg(registry);
    }
    if let Some(url) = &config.url {
        command.arg("--url").arg(url);
    }
    if let (Some(name), Some(token)) = (&config.token_env, &target.token) {
        command.arg("--token-env").arg(name).env(name, token);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

enum ExchangeError {
    Io,
    Limit(&'static str),
}

async fn read_limited(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
    stream: &'static str,
) -> Result<Vec<u8>, ExchangeError> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|_| ExchangeError::Io)?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit.saturating_sub(bytes.len()) {
            return Err(ExchangeError::Limit(stream));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

async fn terminate(child: &mut Child) {
    // Tokio's kill also waits, so a completed cancellation reaps the process.
    // kill_on_drop remains a fallback if the OS cannot complete termination.
    let _ = tokio::time::timeout(CLEANUP_GRACE, child.kill()).await;
}

/// Send one bounded request through the official host and retain its envelope.
///
/// A valid error envelope is returned for exit status 1 or 2 so callers can emit
/// diagnostics before reporting failure. Callers must check `is_error`, as well
/// as operation-specific results such as `result.valid`. Invalid envelopes and
/// inconsistent exit statuses are transport failures. Child stderr is never
/// included in an error or returned to the caller.
pub async fn invoke(config: &HostConfig, request: &Value) -> Result<Value, Error> {
    let target = config.validate()?;
    if request
        .as_object()
        .and_then(|object| object.get("operation"))
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(invalid("eal-host request requires an operation string"));
    }
    let mut buffer = RequestBuffer::default();
    let serialization = serde_json::to_writer(&mut buffer, request);
    if buffer.exceeded {
        return Err(Error::Budget("eal-host request exceeds 2 MiB".to_owned()));
    }
    serialization?;
    let input = buffer.bytes;
    let mut child = command(config, &target).spawn().map_err(|_| {
        Error::Provider("could not start eal-host; check its installation".to_owned())
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Provider("eal-host stdin is unavailable".to_owned()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Provider("eal-host stdout is unavailable".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Provider("eal-host stderr is unavailable".to_owned()))?;
    let exchange = tokio::time::timeout(config.timeout + CLEANUP_GRACE, async {
        let write = async {
            stdin
                .write_all(&input)
                .await
                .map_err(|_| ExchangeError::Io)?;
            stdin.shutdown().await.map_err(|_| ExchangeError::Io)?;
            // EOF is part of the host protocol: it reads one complete document.
            drop(stdin);
            Ok(())
        };
        let ((), stdout, _) = tokio::try_join!(
            write,
            read_limited(stdout, MAX_OUTPUT, "stdout"),
            read_limited(stderr, MAX_STDERR, "stderr"),
        )?;
        let status = child.wait().await.map_err(|_| ExchangeError::Io)?;
        Ok::<_, ExchangeError>((status, stdout))
    })
    .await;
    let (status, stdout) = match exchange {
        Ok(Ok(value)) => value,
        failure => {
            terminate(&mut child).await;
            return Err(match failure {
                Err(_) => {
                    Error::Provider("eal-host exceeded its deadline and was terminated".to_owned())
                }
                Ok(Err(ExchangeError::Limit(stream))) => {
                    Error::Budget(format!("eal-host {stream} exceeded its byte limit"))
                }
                _ => Error::Provider("eal-host process I/O failed".to_owned()),
            });
        }
    };
    let mut response: Value = serde_json::from_slice(&stdout)
        .map_err(|_| Error::Provider("eal-host returned invalid JSON".to_owned()))?;
    validate_response(&response, request, status)?;
    if let Some(token) = target.token.as_deref().and_then(|value| value.to_str()) {
        redact(&mut response, token)?;
    }
    Ok(response)
}

fn validate_response(response: &Value, request: &Value, status: ExitStatus) -> Result<(), Error> {
    let invalid_response =
        || Error::Provider("eal-host returned an invalid response envelope".to_owned());
    let is_error = response
        .as_object()
        .and_then(|object| object.get("is_error"))
        .and_then(Value::as_bool)
        .ok_or_else(invalid_response)?;
    if !matches!(
        (status.code(), is_error),
        (Some(0), false) | (Some(1 | 2), true)
    ) {
        return Err(Error::Provider(
            "eal-host exit status disagrees with its response".to_owned(),
        ));
    }
    if is_error && response.get("error").is_some_and(Value::is_string) {
        return Ok(());
    }
    if response.get("operation") != request.get("operation")
        || !response
            .get("protocol_version")
            .is_some_and(Value::is_string)
        || response.get("result").is_none()
        || !response.get("content").is_some_and(Value::is_array)
    {
        return Err(invalid_response());
    }
    Ok(())
}

fn redact(value: &mut Value, token: &str) -> Result<(), Error> {
    match value {
        Value::String(text) => *text = text.replace(token, "[REDACTED]"),
        Value::Array(values) => {
            for value in values {
                redact(value, token)?;
            }
        }
        Value::Object(object) => {
            if object.keys().any(|key| key.contains(token)) {
                return Err(Error::Provider(
                    "eal-host response contains a credential in a field name".to_owned(),
                ));
            }
            for value in object.values_mut() {
                redact(value, token)?;
            }
        }
        _ => {}
    }
    Ok(())
}
