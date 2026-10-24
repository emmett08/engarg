//! Provider admission for the independent collector implementation stage.
use clap::Args;
use engarg::{
    Error,
    provider::{ApiStyle, Completion, CompletionRequest, HttpConfig, HttpModel, LanguageModel, TransportConfig},
};
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Args)]
pub struct ProviderArgs {
    #[arg(long, env = "ENGARG_MODEL")]
    model: String,
    #[arg(long, env = "ENGARG_SERVICE_TIER")]
    service_tier: Option<String>,
    #[arg(long, env = "ENGARG_REASONING_EFFORT")]
    reasoning_effort: Option<String>,
    #[arg(long, value_enum, default_value = "responses")]
    api: ApiStyle,
    #[arg(long, default_value = "https://api.openai.com/v1/responses")]
    endpoint: String,
    #[arg(long, default_value = "ENGARG_API_KEY")]
    api_key_env: String,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_input_bytes: usize,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_response_bytes: usize,
    #[arg(long, default_value_t = 16_384)]
    max_output_tokens: u32,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=10))]
    http_retries: u8,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600))]
    retry_max_delay_secs: u64,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=86400))]
    request_timeout_secs: u64,
}

impl ProviderArgs {
    pub fn model(&self) -> Result<CodingModel, Error> {
        if self.max_input_bytes == 0 || self.max_response_bytes == 0 {
            return Err(Error::InvalidInput("resource limits must be positive".into()));
        }
        let key = std::env::var(&self.api_key_env).ok().filter(|key| !key.trim().is_empty());
        let missing_key = (self.endpoint == "https://api.openai.com/v1/responses" && key.is_none())
            .then(|| self.api_key_env.clone());
        let inner = independent_model(
            HttpConfig {
                endpoint: self.endpoint.clone(),
                api_key: key,
                model: self.model.clone(),
                service_tier: self.service_tier.clone(),
                reasoning_effort: self.reasoning_effort.clone(),
                role_models: HashMap::new(),
                style: self.api,
                timeout: Duration::from_secs(self.request_timeout_secs),
                max_output_tokens: self.max_output_tokens,
                max_body_bytes: self.max_response_bytes.saturating_mul(2).saturating_add(65_536),
            },
            TransportConfig {
                max_http_retries: self.http_retries,
                retry_max_delay_secs: self.retry_max_delay_secs,
                max_concurrent_requests: 1,
            },
        )?;
        Ok(CodingModel { inner, missing_key })
    }
}

/// Cached reuse needs no provider credentials. A missing default-endpoint key
/// becomes an admission error only if this stage actually needs a model call.
pub struct CodingModel {
    inner: HttpModel,
    missing_key: Option<String>,
}

impl LanguageModel for CodingModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        if let Some(name) = &self.missing_key {
            return Err(Error::InvalidInput(format!("set {name} to the provider API key")));
        }
        self.inner.complete(request).await
    }
}

/// The coding stage has its own model instance, budget and audit. Argument role
/// overrides and argument HTTP events do not enter its provider exchanges.
pub fn independent_model(
    mut config: HttpConfig,
    transport: TransportConfig,
) -> Result<HttpModel, Error> {
    config.role_models.clear();
    HttpModel::new_with_transport(config, transport, Arc::new(|_| Ok(())))
}
