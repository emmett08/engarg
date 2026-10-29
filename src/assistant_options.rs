//! CLI provider admission with common settings and transport-specific construction.
use clap::{Args, ValueEnum};
use engarg::{
    Error,
    engine::AssistantRunConfig,
    provider::{
        ApiStyle, AssistantKind, CommandConfig, CommandModel, Completion, CompletionRequest,
        HttpConfig, HttpModel, HttpObserver, LanguageModel, Role, TransportConfig,
    },
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_HTTP_ENDPOINT: &str = "https://api.openai.com/v1/responses";

/// Model choices have the same meaning for each completion transport.
pub struct ModelSelection {
    pub model: String,
    pub role_models: HashMap<Role, String>,
}

/// Shared request limits; token enforcement remains specific to the protocol.
#[derive(Clone, Copy)]
pub struct CompletionLimits {
    pub timeout: Duration,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_output_tokens: u32,
    pub max_concurrent_requests: usize,
}

/// HTTP-only CLI settings. Credentials never enter retained provider metadata.
pub struct HttpOptions {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub service_tier: Option<String>,
    pub reasoning_effort: Option<String>,
    pub style: ApiStyle,
    pub max_http_retries: u8,
    pub retry_max_delay_secs: u64,
}

impl HttpOptions {
    fn validate_assistant_selection(&self) -> Result<(), Error> {
        if self.service_tier.is_some()
            || self.reasoning_effort.is_some()
            || self.endpoint != DEFAULT_HTTP_ENDPOINT
            || !matches!(self.style, ApiStyle::Responses)
        {
            return Err(Error::InvalidInput(
                "assistant providers do not accept HTTP endpoint, API, service-tier or reasoning-effort options".into(),
            ));
        }
        Ok(())
    }

    fn model(
        self,
        selection: ModelSelection,
        limits: CompletionLimits,
        observer: HttpObserver,
    ) -> Result<HttpModel, Error> {
        HttpModel::new_with_transport(
            HttpConfig {
                endpoint: self.endpoint,
                api_key: self.api_key,
                model: selection.model,
                role_models: selection.role_models,
                service_tier: self.service_tier,
                reasoning_effort: self.reasoning_effort,
                style: self.style,
                timeout: limits.timeout,
                max_output_tokens: limits.max_output_tokens,
                max_body_bytes: limits.max_output_bytes,
            },
            TransportConfig {
                max_http_retries: self.max_http_retries,
                retry_max_delay_secs: self.retry_max_delay_secs,
                max_concurrent_requests: limits.max_concurrent_requests,
            },
            observer,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ProviderKind {
    Http,
    Cursor,
    Augment,
    Command,
}

#[derive(Args)]
pub struct AssistantArgs {
    /// Model transport: HTTP API, installed Cursor/Auggie CLI, or a JSON command adapter.
    #[arg(long, value_enum, default_value = "http", env = "ENGARG_PROVIDER")]
    pub provider: ProviderKind,
    /// TOML argv and inherit_env. Required for command; optional native CLI override.
    #[arg(long, value_name = "TOML")]
    pub assistant_config: Option<PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSettings {
    pub argv: Vec<String>,
    pub inherit_env: Vec<String>,
}

pub fn read_config(path: &Path) -> Result<CommandSettings, Error> {
    let bytes = engarg::bounded_file::read(path, 65_536, "assistant configuration")?;
    parse_config(&bytes)
}

fn parse_config(bytes: &[u8]) -> Result<CommandSettings, Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidInput("assistant configuration requires UTF-8".into()))?;
    // TOML diagnostics can echo argv or private configuration. Keep them private.
    let settings: CommandSettings = toml::from_str(text)
        .map_err(|_| Error::InvalidInput("invalid assistant configuration TOML".into()))?;
    engarg::provider::command::validate_command_settings(&settings.argv, &settings.inherit_env)?;
    Ok(settings)
}

#[derive(Clone)]
pub struct AdmittedAssistant {
    kind: AssistantKind,
    settings: CommandSettings,
    configuration_sha256: Option<String>,
}

impl AssistantArgs {
    pub fn admit(&self) -> Result<Option<AdmittedAssistant>, Error> {
        let kind = match self.provider {
            ProviderKind::Http => {
                if self.assistant_config.is_some() {
                    return Err(Error::InvalidInput(
                        "--assistant-config requires an assistant provider".into(),
                    ));
                }
                return Ok(None);
            }
            ProviderKind::Cursor => AssistantKind::Cursor,
            ProviderKind::Augment => AssistantKind::Augment,
            ProviderKind::Command => AssistantKind::Command,
        };
        let (settings, configuration_sha256) = if let Some(path) = &self.assistant_config {
            let bytes = engarg::bounded_file::read(path, 65_536, "assistant configuration")?;
            let mut settings = parse_config(&bytes)?;
            // A path-qualified executable is relative to the configuration, not
            // the temporary per-call working directory. Bare names use PATH.
            let executable = Path::new(&settings.argv[0]);
            if executable.is_relative() && executable.components().count() > 1 {
                settings.argv[0] =
                    std::path::absolute(path.parent().unwrap_or(Path::new(".")).join(executable))?
                        .to_str()
                        .ok_or_else(|| {
                            Error::InvalidInput("assistant executable requires UTF-8".into())
                        })?
                        .to_owned();
            }
            (settings, Some(engarg::authoring_checkpoint::digest(&bytes)))
        } else {
            let executable = match kind {
                AssistantKind::Cursor => "agent",
                AssistantKind::Augment => "auggie",
                AssistantKind::Command => {
                    return Err(Error::InvalidInput(
                        "--provider command requires --assistant-config".into(),
                    ));
                }
            };
            (
                CommandSettings {
                    argv: vec![executable.into()],
                    inherit_env: ["PATH", "HOME"]
                        .into_iter()
                        .filter(|name| std::env::var_os(name).is_some())
                        .map(str::to_owned)
                        .collect(),
                },
                None,
            )
        };
        Ok(Some(AdmittedAssistant {
            kind,
            settings,
            configuration_sha256,
        }))
    }
}

impl AdmittedAssistant {
    pub fn metadata(&self) -> AssistantRunConfig {
        AssistantRunConfig {
            provider: self.kind,
            configuration_sha256: self.configuration_sha256.clone(),
        }
    }

    pub fn endpoint(&self) -> String {
        format!("command:{}", self.name())
    }

    fn name(&self) -> &'static str {
        match self.kind {
            AssistantKind::Cursor => "cursor",
            AssistantKind::Augment => "augment",
            AssistantKind::Command => "command",
        }
    }

    fn model(
        &self,
        selection: ModelSelection,
        limits: CompletionLimits,
    ) -> Result<CommandModel, Error> {
        CommandModel::new(CommandConfig {
            kind: self.kind,
            argv: self.settings.argv.clone(),
            inherit_env: self.settings.inherit_env.clone(),
            model: selection.model,
            role_models: selection.role_models,
            timeout: limits.timeout,
            max_input_bytes: limits.max_input_bytes,
            max_output_bytes: limits.max_output_bytes,
            max_output_tokens: limits.max_output_tokens,
            max_concurrent_requests: limits.max_concurrent_requests,
            supervisor: crate::tooling_commands::supervisor()?,
        })
    }
}

/// Selection only; each transport owns its own protocol and resource policy.
pub enum ConfiguredModel {
    Http(HttpModel),
    Assistant(CommandModel),
}

impl LanguageModel for ConfiguredModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        match self {
            Self::Http(model) => model.complete(request).await,
            Self::Assistant(model) => model.complete(request).await,
        }
    }
}

pub fn configured_model(
    assistant: Option<&AdmittedAssistant>,
    selection: ModelSelection,
    limits: CompletionLimits,
    http: HttpOptions,
    observer: HttpObserver,
) -> Result<ConfiguredModel, Error> {
    match assistant {
        Some(assistant) => {
            http.validate_assistant_selection()?;
            assistant
                .model(selection, limits)
                .map(ConfiguredModel::Assistant)
        }
        None => http
            .model(selection, limits, observer)
            .map(ConfiguredModel::Http),
    }
}
