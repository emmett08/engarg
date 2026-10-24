//! Bounded one-request adapter to the official `eal-host` MCP client.
//!
//! `eal-host` performs MCP initialization, tool discovery and schema validation.
//! Its sibling `eal-mcp` executable supplies the stdio server. Targets belong to
//! operator configuration; argument generation must never choose an executable.

use crate::Error;
use eal_process::{CommandRequest, ExecutionError};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_INPUT: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
const CLEANUP_GRACE: Duration = Duration::from_secs(1);
const MAX_REGISTRY: usize = 1024 * 1024;
const MAX_COLLECTOR_ENV: usize = 64;
const MAX_ENV_VALUE: usize = 16 * 1024;

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
    /// Explicitly public configuration, declared by the operator alone.
    pub public_collector_env: Vec<String>,
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
    sensitivity: eal_collector_contracts::OperatorEnvironmentPolicy,
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
        let sensitivity = eal_collector_contracts::OperatorEnvironmentPolicy {
            schema: eal_collector_contracts::OPERATOR_ENVIRONMENT_POLICY.into(),
            forward: self.collector_env.clone(),
            public: self.public_collector_env.clone(),
        };
        sensitivity.validate().map_err(Error::InvalidInput)?;
        if self
            .token_env
            .as_ref()
            .is_some_and(|name| sensitivity.public.contains(name))
        {
            return Err(invalid("bearer token sensitivity cannot be downgraded"));
        }
        if let Some(registry) = &registry {
            crate::bounded_file::read(registry, MAX_REGISTRY, "eal-host registry")?;
        }
        let collector_env = collector_environment(&self.collector_env, registry.as_deref())?;
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
            sensitivity,
        })
    }
}

fn collector_environment(
    names: &[String],
    registry: Option<&Path>,
) -> Result<Vec<(String, OsString)>, Error> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let Some(registry) = registry else {
        return Err(invalid(
            "collector-env requires an explicit local tools registry",
        ));
    };
    if names.len() > MAX_COLLECTOR_ENV {
        return Err(invalid("collector-env exceeds 64 variable names"));
    }
    let allowed = registry_environment(registry)?;
    let mut selected = BTreeSet::new();
    let mut values = Vec::new();
    for name in names {
        if !environment_name(name) || !selected.insert(name) || !allowed.contains(name) {
            return Err(invalid(
                "collector-env requires distinct names declared in the registry's inherit_env",
            ));
        }
        let value = std::env::var_os(name)
            .filter(|value| {
                value.to_str().is_some_and(|value| {
                    !value.is_empty() && value.len() <= MAX_ENV_VALUE.saturating_sub(name.len())
                })
            })
            .ok_or_else(|| {
                invalid(
                    "a configured collector environment variable requires bounded nonempty UTF-8",
                )
            })?;
        values.push((name.clone(), value));
    }
    Ok(values)
}

/// Only operator command bindings can grant forwarding through this process.
/// The official host remains responsible for complete registry validation.
fn registry_environment(path: &Path) -> Result<BTreeSet<String>, Error> {
    let bytes = crate::bounded_file::read(path, MAX_REGISTRY, "eal-host registry")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| invalid("eal-host registry must contain UTF-8 TOML"))?;
    let registry =
        eal_collector_contracts::RegistryDocument::from_toml(text).map_err(Error::InvalidInput)?;
    let mut allowed = BTreeSet::new();
    for binding in registry.tools.values() {
        if let Some(inherited) = &binding.inherit_env {
            allowed.extend(inherited.iter().cloned());
        }
    }
    Ok(allowed)
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
    eal_collector_contracts::environment_name(value)
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

fn command(
    config: &HostConfig,
    target: &Target,
    input: Vec<u8>,
    timeout: Duration,
) -> Result<CommandRequest, Error> {
    let mut environment = std::collections::BTreeMap::new();
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
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.into(), value);
        }
    }
    for (name, value) in &target.collector_env {
        environment.insert(
            name.clone(),
            value.to_str().expect("validated UTF-8").into(),
        );
    }
    let mut argv = vec![
        config.executable.to_str().expect("validated UTF-8").into(),
        "--workspace".into(),
        target.workspace.to_str().expect("validated UTF-8").into(),
        "--exposure".into(),
        "operator".into(),
        "--methods".into(),
        "builtin".into(),
        "--timeout".into(),
        timeout.as_secs_f64().to_string(),
    ];
    for (flag, path) in [
        ("--database", target.database.as_ref()),
        ("--registry", target.registry.as_ref()),
    ] {
        if let Some(path) = path {
            argv.extend([
                flag.into(),
                path.to_str()
                    .ok_or_else(|| invalid("host path requires UTF-8"))?
                    .into(),
            ]);
        }
    }
    if let Some(url) = &config.url {
        argv.extend(["--url".into(), url.clone()]);
    }
    if let (Some(name), Some(token)) = (&config.token_env, &target.token) {
        argv.extend(["--token-env".into(), name.clone()]);
        environment.insert(
            name.clone(),
            token.to_str().expect("validated UTF-8").into(),
        );
    }
    Ok(CommandRequest {
        argv,
        environment,
        cwd: target.workspace.clone(),
        input,
        timeout: timeout + CLEANUP_GRACE,
        max_output_bytes: MAX_OUTPUT + MAX_STDERR,
    })
}

/// Send one bounded request through the official host and retain its envelope.
///
/// A valid error envelope is returned for exit status 1 or 2 so callers can emit
/// diagnostics before reporting failure. Callers must check `is_error`, as well
/// as operation-specific results such as `result.valid`. Invalid envelopes and
/// inconsistent exit statuses are transport failures. Child stderr is never
/// included in an error or returned to the caller.
pub async fn invoke(config: &HostConfig, request: &Value) -> Result<Value, Error> {
    let started = std::time::Instant::now();
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
    let timeout = config
        .timeout
        .checked_sub(started.elapsed())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| Error::Budget("eal-host deadline exhausted during admission".into()))?;
    let outcome = eal_adapters::command::run_with_stream_limits(
        command(config, &target, buffer.bytes, timeout)?,
        eal_process::StreamLimits {
            max_stdout_bytes: MAX_OUTPUT,
            max_stderr_bytes: MAX_STDERR,
        },
    )
    .await
    .map_err(|_| {
        Error::Provider(
            "EAL_HOST_PROCESS: bounded host execution failed; check supervisor installation".into(),
        )
    })?;
    if outcome.stdout_bytes > MAX_OUTPUT {
        return Err(Error::Budget(
            "eal-host stdout exceeded its byte limit".into(),
        ));
    }
    if outcome.stderr_bytes > MAX_STDERR {
        return Err(Error::Budget(
            "eal-host stderr exceeded its byte limit".into(),
        ));
    }
    if outcome.execution_error.is_some() {
        return Err(match outcome.execution_error {
            Some(ExecutionError::OutputLimit) => {
                Error::Budget("eal-host stdout or stderr exceeded its byte limit".into())
            }
            Some(ExecutionError::Timeout) => {
                Error::Provider("eal-host exceeded its deadline and was terminated".into())
            }
            _ => Error::Provider("eal-host bounded process execution failed".into()),
        });
    }
    let response = eal_collector_contracts::strict_json_exact(
        &outcome.stdout,
        eal_collector_contracts::JsonLimits {
            max_bytes: MAX_OUTPUT,
            max_depth: 128,
            ..Default::default()
        },
    )
    .map_err(|_| Error::Provider("eal-host returned invalid JSON".into()))?;
    let status = outcome.returncode;
    validate_response(&response, request, status)?;
    let secrets: Vec<_> = target
        .token
        .as_deref()
        .into_iter()
        .chain(
            target
                .collector_env
                .iter()
                .filter(|(name, _)| target.sensitivity.sensitive(name))
                .map(|(_, value)| value.as_os_str()),
        )
        .filter_map(|value| value.to_str())
        .collect();
    let contains_secret = eal_collector_contracts::contains_sensitive_json(
        &response,
        &secrets,
        eal_collector_contracts::SensitiveJsonLimits::default(),
    )
    .map_err(|_| {
        Error::Provider("EAL_HOST_CREDENTIAL_INSPECTION: response inspection failed".into())
    })?;
    if contains_secret {
        return Err(Error::Provider(
            "EAL_HOST_CREDENTIAL_LEAK: host response contains sensitive data".into(),
        ));
    }
    Ok(response)
}

fn validate_response(response: &Value, request: &Value, status: i32) -> Result<(), Error> {
    let invalid_response =
        || Error::Provider("eal-host returned an invalid response envelope".to_owned());
    let is_error = response
        .as_object()
        .and_then(|object| object.get("is_error"))
        .and_then(Value::as_bool)
        .ok_or_else(invalid_response)?;
    if !matches!((status, is_error), (0, false) | (1 | 2, true)) {
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
