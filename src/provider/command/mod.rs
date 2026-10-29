//! Bounded assistant subprocesses implementing the existing completion contract.
mod augment;
mod claude;
mod codex;
mod copilot;
mod cursor;
mod gemini;
mod generic;
mod native;
mod process;

use super::{Completion, CompletionRequest, LanguageModel, Role};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Semaphore;

/// Assistant-specific wire contract, independent of the selected model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantKind {
    Cursor,
    Augment,
    Claude,
    Codex,
    Copilot,
    Gemini,
    Command,
}

impl AssistantKind {
    /// Stable protocol identifier used in retained metadata and command endpoints.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Augment => "augment",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
            Self::Command => "command",
        }
    }
}

/// Explicit operator authority and resource limits for assistant execution.
/// Environment values are captured privately and never enter completion metadata.
pub struct CommandConfig {
    pub kind: AssistantKind,
    pub argv: Vec<String>,
    pub inherit_env: Vec<String>,
    pub model: String,
    pub role_models: HashMap<Role, String>,
    pub timeout: Duration,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_output_tokens: u32,
    pub max_concurrent_requests: usize,
    pub supervisor: PathBuf,
}

pub struct CommandModel {
    config: CommandConfig,
    environment: BTreeMap<String, String>,
    permits: Arc<Semaphore>,
}

/// Admit the shared operator settings without reading environment values or starting a process.
pub fn validate_command_settings(argv: &[String], inherit_env: &[String]) -> Result<(), Error> {
    if argv.is_empty()
        || argv.len() > 64
        || argv
            .iter()
            .any(|value| value.len() > 8192 || value.contains('\0'))
        || argv[0].trim().is_empty()
    {
        return Err(Error::InvalidInput(
            "assistant requires a nonempty executable and bounded argv".into(),
        ));
    }
    eal_collector_contracts::validate_environment_names(inherit_env, 64).map_err(|_| {
        Error::InvalidInput(
            "assistant environment names must be valid, distinct and bounded".into(),
        )
    })
}

impl CommandModel {
    pub fn new(mut config: CommandConfig) -> Result<Self, Error> {
        validate_command_settings(&config.argv, &config.inherit_env)?;
        if !valid_model(&config.model)
            || config.role_models.values().any(|model| !valid_model(model))
            || config.supervisor.as_os_str().is_empty()
        {
            return Err(Error::InvalidInput(
                "assistant requires bounded argv, model identifiers and a supervisor".into(),
            ));
        }
        if config.timeout.is_zero()
            || config.max_input_bytes == 0
            || config.max_output_bytes == 0
            || config.max_output_tokens == 0
            || !(1..=64).contains(&config.max_concurrent_requests)
            || config
                .max_output_bytes
                .checked_add(process::STDERR_LIMIT)
                .is_none()
        {
            return Err(Error::InvalidInput(
                "assistant resource limits must be positive with 1..64 concurrent requests".into(),
            ));
        }
        let mut environment = BTreeMap::new();
        for name in &config.inherit_env {
            environment.insert(
                name.clone(),
                std::env::var(name).map_err(|_| {
                    Error::InvalidInput(
                        "selected assistant environment variable is unavailable or not UTF-8"
                            .into(),
                    )
                })?,
            );
        }
        // Relative executable paths belong to the operator's directory, while
        // every actual call runs in a fresh directory with no project files.
        if PathBuf::from(&config.argv[0]).components().count() > 1 {
            config.argv[0] = std::fs::canonicalize(&config.argv[0])
                .map_err(|_| Error::InvalidInput("assistant executable is unavailable".into()))?
                .into_os_string()
                .into_string()
                .map_err(|_| {
                    Error::InvalidInput("assistant executable path must be UTF-8".into())
                })?;
        }
        let permits = Arc::new(Semaphore::new(config.max_concurrent_requests));
        Ok(Self {
            config,
            environment,
            permits,
        })
    }

    async fn complete_bounded(
        &self,
        request: CompletionRequest,
        deadline: tokio::time::Instant,
    ) -> Result<Completion, Error> {
        let model = self
            .config
            .role_models
            .get(&request.role)
            .unwrap_or(&self.config.model);
        let invocation = match self.config.kind {
            AssistantKind::Cursor => cursor::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Augment => augment::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Claude => claude::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Codex => codex::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Copilot => copilot::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Gemini => gemini::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
            AssistantKind::Command => generic::encode(
                &self.config.argv,
                model,
                &request,
                self.config.max_output_tokens,
            ),
        }?;
        if invocation.input.len() > self.config.max_input_bytes {
            return Err(Error::Budget(
                "assistant request exceeds input byte limit".into(),
            ));
        }
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Provider("assistant admission closed".into()))?;
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(process::timeout_error());
        }
        let bytes = process::invoke(
            invocation,
            &self.environment,
            &self.config.supervisor,
            remaining,
            self.config.max_output_bytes,
            permit,
        )
        .await?;
        let completion = match self.config.kind {
            AssistantKind::Cursor => cursor::decode(&bytes, model),
            AssistantKind::Augment => augment::decode(&bytes, model),
            AssistantKind::Claude => claude::decode(&bytes, model),
            AssistantKind::Codex => codex::decode(&bytes, model),
            AssistantKind::Copilot => copilot::decode(&bytes, model),
            AssistantKind::Gemini => gemini::decode(&bytes, model),
            AssistantKind::Command => generic::decode(&bytes),
        }?;
        if completion.content.trim().is_empty() || completion.model.trim().is_empty() {
            return Err(Error::Provider(
                "assistant returned empty completion content or model identity; no automatic retry"
                    .into(),
            ));
        }
        Ok(completion)
    }
}

impl LanguageModel for CommandModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let deadline = tokio::time::Instant::now()
            .checked_add(self.config.timeout)
            .ok_or_else(|| {
                Error::InvalidInput("assistant timeout exceeds the supported clock range".into())
            })?;
        tokio::time::timeout_at(deadline, self.complete_bounded(request, deadline))
            .await
            .unwrap_or_else(|_| Err(process::timeout_error()))
    }
}

fn valid_model(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

pub(super) struct Invocation {
    argv: Vec<String>,
    input: Vec<u8>,
}

#[cfg(test)]
mod tests;
