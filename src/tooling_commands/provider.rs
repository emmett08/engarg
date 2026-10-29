//! Provider admission for the independent collector implementation stage.
use clap::Args;
use engarg::{
    Error,
    provider::{ApiStyle, Completion, CompletionRequest, LanguageModel},
};
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Args)]
pub struct ProviderArgs {
    #[command(flatten)]
    assistant: crate::assistant_options::AssistantArgs,
    #[arg(long, env = "ENGARG_MODEL")]
    model: String,
    #[arg(long, env = "ENGARG_SERVICE_TIER")]
    service_tier: Option<String>,
    #[arg(long, env = "ENGARG_REASONING_EFFORT")]
    reasoning_effort: Option<String>,
    #[arg(long, value_enum, default_value = "responses")]
    api: ApiStyle,
    #[arg(long, default_value = crate::assistant_options::DEFAULT_HTTP_ENDPOINT)]
    endpoint: String,
    #[arg(long, default_value = "ENGARG_API_KEY")]
    api_key_env: String,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_input_bytes: usize,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_response_bytes: usize,
    #[arg(long, default_value_t = 16_384)]
    max_output_tokens: u32,
    /// Extra HTTP attempts after 429, 500, 502, 503 or 504 responses.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=10))]
    http_retries: u8,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600))]
    retry_max_delay_secs: u64,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=86400))]
    request_timeout_secs: u64,
}

impl ProviderArgs {
    /// Requested settings excluding credentials and credential variable names.
    fn metadata(
        &self,
        assistant: Option<&crate::assistant_options::AdmittedAssistant>,
    ) -> serde_json::Value {
        let mut value = serde_json::json!({
            "model":self.model,"endpoint":self.endpoint,"api":format!("{:?}",self.api),
            "service_tier":self.service_tier,"reasoning_effort":self.reasoning_effort,
            "max_output_tokens":self.max_output_tokens,"http_retries":self.http_retries,
            "retry_max_delay_secs":self.retry_max_delay_secs,
            "request_timeout_secs":self.request_timeout_secs,
        });
        if let Some(assistant) = assistant {
            value["assistant"] =
                serde_json::to_value(assistant.metadata()).expect("assistant metadata");
            value["endpoint"] = serde_json::json!(assistant.endpoint());
            value["api"] = serde_json::json!("AssistantCommand");
            value
                .as_object_mut()
                .expect("provider metadata")
                .remove("http_retries");
            value
                .as_object_mut()
                .expect("provider metadata")
                .remove("retry_max_delay_secs");
        }
        value
    }

    pub fn model(&self) -> Result<CodingModel, Error> {
        if self.max_input_bytes == 0 || self.max_response_bytes == 0 {
            return Err(Error::InvalidInput(
                "resource limits must be positive".into(),
            ));
        }
        let assistant = self.assistant.admit()?;
        let key = assistant
            .is_none()
            .then(|| std::env::var(&self.api_key_env).ok())
            .flatten()
            .filter(|key| !key.trim().is_empty());
        let missing_key = (assistant.is_none()
            && self.endpoint == crate::assistant_options::DEFAULT_HTTP_ENDPOINT
            && key.is_none())
        .then(|| self.api_key_env.clone());
        let inner = crate::assistant_options::configured_model(
            assistant.as_ref(),
            crate::assistant_options::ModelSelection {
                model: self.model.clone(),
                role_models: HashMap::new(),
            },
            crate::assistant_options::CompletionLimits {
                timeout: Duration::from_secs(self.request_timeout_secs),
                max_input_bytes: self.max_input_bytes,
                max_output_tokens: self.max_output_tokens,
                max_output_bytes: self
                    .max_response_bytes
                    .saturating_mul(2)
                    .saturating_add(65_536),
                max_concurrent_requests: 1,
            },
            crate::assistant_options::HttpOptions {
                endpoint: self.endpoint.clone(),
                api_key: key,
                service_tier: self.service_tier.clone(),
                reasoning_effort: self.reasoning_effort.clone(),
                style: self.api,
                max_http_retries: self.http_retries,
                retry_max_delay_secs: self.retry_max_delay_secs,
            },
            Arc::new(|_| Ok(())),
        )?;
        Ok(CodingModel {
            inner,
            missing_key,
            metadata: self.metadata(assistant.as_ref()),
        })
    }
}

/// Cached reuse needs no provider credentials. A missing default-endpoint key
/// becomes an admission error only if this stage actually needs a model call.
pub struct CodingModel {
    inner: crate::assistant_options::ConfiguredModel,
    missing_key: Option<String>,
    metadata: serde_json::Value,
}

impl CodingModel {
    pub fn metadata(&self) -> &serde_json::Value {
        &self.metadata
    }
}

impl LanguageModel for CodingModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        if let Some(name) = &self.missing_key {
            return Err(Error::InvalidInput(format!(
                "set {name} to the provider API key"
            )));
        }
        self.inner.complete(request).await
    }
}
