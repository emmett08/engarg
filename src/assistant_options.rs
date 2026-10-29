//! Shared admission and selection of operator-owned assistant command providers.
use clap::{Args, ValueEnum};
use engarg::{
    Error,
    engine::AssistantRunConfig,
    provider::{
        ApiStyle, AssistantKind, CommandConfig, CommandModel, Completion, CompletionRequest,
        HttpConfig, HttpModel, HttpObserver, LanguageModel, TransportConfig,
    },
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

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

    pub fn model(
        &self,
        config: HttpConfig,
        max_input_bytes: usize,
        max_concurrent_requests: usize,
    ) -> Result<ConfiguredModel, Error> {
        if config.service_tier.is_some()
            || config.reasoning_effort.is_some()
            || config.endpoint != "https://api.openai.com/v1/responses"
            || !matches!(config.style, ApiStyle::Responses)
        {
            return Err(Error::InvalidInput(
                "assistant providers do not accept HTTP endpoint, API, service-tier or reasoning-effort options".into(),
            ));
        }
        Ok(ConfiguredModel::Assistant(CommandModel::new(
            CommandConfig {
                kind: self.kind,
                argv: self.settings.argv.clone(),
                inherit_env: self.settings.inherit_env.clone(),
                model: config.model,
                role_models: config.role_models,
                timeout: config.timeout,
                max_input_bytes,
                max_output_bytes: config.max_body_bytes,
                max_output_tokens: config.max_output_tokens,
                max_concurrent_requests,
                supervisor: crate::tooling_commands::supervisor()?,
            },
        )?))
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
    config: HttpConfig,
    transport: TransportConfig,
    observer: HttpObserver,
    max_input_bytes: usize,
) -> Result<ConfiguredModel, Error> {
    match assistant {
        Some(assistant) => {
            assistant.model(config, max_input_bytes, transport.max_concurrent_requests)
        }
        None => {
            HttpModel::new_with_transport(config, transport, observer).map(ConfiguredModel::Http)
        }
    }
}
