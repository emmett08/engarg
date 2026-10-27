//! Model wire formats with a separate bounded HTTP transport.

use crate::Error;
use reqwest::Url;
mod http;
mod response;
mod retry_policy;
pub use http::{HttpEvent, HttpObserver, TransportConfig};
pub use response::ResponseFailure;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, fmt, future::Future, net::IpAddr, str::FromStr, time::Duration};

/// Distinct calls receive only the context supplied by the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Constructor,
    Sceptic,
    EvidenceAuditor,
    Synthesiser,
    FinalCritic,
    Adjudicator,
    Single,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Constructor => "constructor",
            Self::Sceptic => "sceptic",
            Self::EvidenceAuditor => "evidence_auditor",
            Self::Synthesiser => "synthesiser",
            Self::FinalCritic => "final_critic",
            Self::Adjudicator => "adjudicator",
            Self::Single => "single",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "constructor" => Ok(Self::Constructor),
            "sceptic" => Ok(Self::Sceptic),
            "evidence_auditor" => Ok(Self::EvidenceAuditor),
            "synthesiser" => Ok(Self::Synthesiser),
            "final_critic" => Ok(Self::FinalCritic),
            "adjudicator" => Ok(Self::Adjudicator),
            "single" => Ok(Self::Single),
            _ => Err(Error::InvalidInput(format!("unknown role {s}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionRequest {
    pub role: Role,
    pub system: String,
    pub user: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub content: String,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub request_id: Option<String>,
    /// Provider-reported tier, which may differ from the requested tier.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Provider-reported effort when available, never inferred from the request.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

/// Small dependency-inversion boundary; implement it for another API or replay.
pub trait LanguageModel: Send + Sync {
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<Completion, Error>> + Send;
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ApiStyle {
    Responses,
    Chat,
}

/// Secrets are intentionally excluded from Debug and report serialisation.
pub struct HttpModel {
    http: http::HttpTransport,
    model: String,
    role_models: HashMap<Role, String>,
    service_tier: Option<String>,
    reasoning_effort: Option<String>,
    style: ApiStyle,
    max_output_tokens: u32,
}

pub struct HttpConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub role_models: HashMap<Role, String>,
    /// Passed through exactly when set. Availability is provider-specific.
    pub service_tier: Option<String>,
    /// Optional effort identifier. The endpoint validates model compatibility.
    pub reasoning_effort: Option<String>,
    pub style: ApiStyle,
    pub timeout: Duration,
    pub max_output_tokens: u32,
    pub max_body_bytes: usize,
}

fn validate_endpoint(endpoint: &str) -> Result<Url, Error> {
    let url = Url::parse(endpoint)
        .map_err(|_| Error::InvalidInput("endpoint must be an absolute URL".into()))?;
    let local = url.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err(Error::InvalidInput(
            "endpoint requires HTTPS, or HTTP on loopback".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidInput(
            "endpoint must have no credentials, query or fragment".into(),
        ));
    }
    Ok(url)
}

fn validate_option_identifier(name: &str, value: Option<&str>) -> Result<(), Error> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > 64 || !value.bytes().all(|byte| byte.is_ascii_graphic())
    }) {
        return Err(Error::InvalidInput(format!(
            "{name} must contain 1 to 64 visible ASCII characters"
        )));
    }
    Ok(())
}

impl HttpModel {
    pub fn new(config: HttpConfig) -> Result<Self, Error> {
        Self::new_with_transport(
            config,
            TransportConfig::disabled(),
            std::sync::Arc::new(|_| Ok(())),
        )
    }

    pub fn new_with_transport(
        config: HttpConfig,
        transport: TransportConfig,
        observer: HttpObserver,
    ) -> Result<Self, Error> {
        let endpoint = validate_endpoint(&config.endpoint)?;
        validate_option_identifier("service tier", config.service_tier.as_deref())?;
        validate_option_identifier("reasoning effort", config.reasoning_effort.as_deref())?;
        if config.model.trim().is_empty()
            || config.role_models.values().any(|m| m.trim().is_empty())
        {
            return Err(Error::InvalidInput("model names must be nonempty".into()));
        }
        if config.timeout.is_zero() || config.max_body_bytes == 0 || config.max_output_tokens == 0 {
            return Err(Error::InvalidInput(
                "transport limits must be positive".into(),
            ));
        }
        let http = http::HttpTransport::new(
            endpoint,
            config.api_key,
            config.timeout,
            config.max_body_bytes,
            transport,
            observer,
        )?;
        Ok(Self {
            http,
            model: config.model,
            role_models: config.role_models,
            service_tier: config.service_tier,
            reasoning_effort: config.reasoning_effort,
            style: config.style,
            max_output_tokens: config.max_output_tokens,
        })
    }

    fn body(&self, request: &CompletionRequest) -> Value {
        let model = self.role_models.get(&request.role).unwrap_or(&self.model);
        let format = json!({"type":"json_schema", "name":"engineering_argument", "strict":true,"schema":request.schema});
        let mut body = match self.style {
            ApiStyle::Responses => json!({
                "model":model,"instructions":request.system,"input":request.user,
                "text":{"format":format},"max_output_tokens":self.max_output_tokens,"store":false
            }),
            ApiStyle::Chat => json!({
                "model":model,"messages":[{"role":"system","content":request.system},{"role":"user","content":request.user}],
                "response_format":{"type":"json_schema","json_schema":{"name":"engineering_argument","strict":true,"schema":request.schema}},
                "max_completion_tokens":self.max_output_tokens,"store":false
            }),
        };
        if let Some(tier) = &self.service_tier {
            body["service_tier"] = json!(tier);
        }
        if let Some(effort) = &self.reasoning_effort {
            match self.style {
                ApiStyle::Responses => body["reasoning"] = json!({"effort": effort}),
                ApiStyle::Chat => body["reasoning_effort"] = json!(effort),
            }
        }
        body
    }
}

impl LanguageModel for HttpModel {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        let body = self.body(&request);
        let response = self.http.post(body, request.role).await?;
        let model = self.role_models.get(&request.role).unwrap_or(&self.model);
        match response::decode(response.value, self.style, model, self.max_output_tokens) {
            Ok(completion) => Ok(completion),
            Err(failure) => {
                let error = failure.error(request.role);
                self.http.reject_response(
                    request.role,
                    response.attempt,
                    response.status,
                    failure,
                )?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(
        value: Value,
        style: ApiStyle,
        model: &str,
    ) -> Result<Completion, Box<ResponseFailure>> {
        response::decode(value, style, model, 128)
    }

    #[test]
    fn endpoints_reject_secrets_and_unencrypted_remote_hosts() {
        for u in [
            "http://example.com/v1",
            "https://key:secret@example.com/v1",
            "https://example.com/v1?key=secret",
            "file:///tmp/model",
        ] {
            assert!(validate_endpoint(u).is_err(), "{u}");
        }
        for u in [
            "https://example.com/v1/responses",
            "http://localhost:11434/v1/chat/completions",
            "http://[::1]:9000/v1",
        ] {
            assert!(validate_endpoint(u).is_ok(), "{u}");
        }
    }

    #[test]
    fn rejects_truncation_refusal_and_malformed_responses() {
        assert!(
            decode(
                json!({"status":"incomplete","output":[]}),
                ApiStyle::Responses,
                "m"
            )
            .is_err()
        );
        assert!(decode(json!({"status":"completed","output":[{"type":"message","content":[{"type":"refusal"}]}]}),ApiStyle::Responses,"m").is_err());
        assert!(
            decode(
                json!({"choices":[{"finish_reason":"length","message":{"content":"{}"}}]}),
                ApiStyle::Chat,
                "m"
            )
            .is_err()
        );
        assert!(decode(json!({"choices":[{"finish_reason":"stop","message":{"content":"{}","refusal":"declined"}}]}),ApiStyle::Chat,"m").is_err());
    }

    #[test]
    fn parses_both_wire_formats_and_keeps_model_identity() {
        let c=decode(json!({"id":"r1","model":"actual","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]}],"usage":{"input_tokens":7,"output_tokens":2}}),ApiStyle::Responses,"fallback").unwrap();
        assert_eq!(c.model, "actual");
        assert_eq!(c.input_tokens, Some(7));
        let c = decode(
            json!({"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]}),
            ApiStyle::Chat,
            "fallback",
        )
        .unwrap();
        assert_eq!(c.model, "fallback");
        assert_eq!(c.input_tokens, None);
    }
    #[test]
    fn optional_provider_identifiers_are_bounded_without_an_enum_whitelist() {
        for name in ["service tier", "reasoning effort"] {
            for value in [
                None,
                Some("ultrafast"),
                Some("high"),
                Some("provider_vNext"),
            ] {
                assert!(validate_option_identifier(name, value).is_ok());
            }
            for value in ["", " ", "high\n", "\tpriority", "prîority"] {
                assert!(validate_option_identifier(name, Some(value)).is_err());
            }
            assert!(validate_option_identifier(name, Some(&"a".repeat(65))).is_err());
        }
    }

    #[test]
    fn completion_deserialisation_keeps_old_receipts_without_fabricating_provider_settings() {
        let completion: Completion = serde_json::from_value(json!({
            "content":"{}", "model":"recorded-model", "input_tokens":null,
            "output_tokens":null, "request_id":null
        }))
        .unwrap();
        assert_eq!(completion.service_tier, None);
        assert_eq!(completion.reasoning_effort, None);
        let decoded = decode(
            json!({
                "choices":[{"finish_reason":"stop","message":{"content":"{}"}}],
                "service_tier":42
            }),
            ApiStyle::Chat,
            "m",
        );
        assert!(decoded.is_err());
    }

    #[test]
    fn provider_reported_effort_is_optional_and_rejects_malformed_metadata() {
        for style in [ApiStyle::Responses, ApiStyle::Chat] {
            let base = match style {
                ApiStyle::Responses => {
                    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]}]})
                }
                ApiStyle::Chat => {
                    json!({"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]})
                }
            };
            assert_eq!(
                decode(base.clone(), style, "m").unwrap().reasoning_effort,
                None
            );
            for effort in [
                Value::Null,
                json!("medium"),
                json!("provider_vNext"),
                json!(42),
            ] {
                let mut response = base.clone();
                match style {
                    ApiStyle::Responses => response["reasoning"] = json!({"effort": effort}),
                    ApiStyle::Chat => response["reasoning_effort"] = effort.clone(),
                }
                let result = decode(response, style, "m");
                match effort {
                    Value::Null => assert_eq!(result.unwrap().reasoning_effort, None),
                    Value::String(effort) => {
                        assert_eq!(result.unwrap().reasoning_effort, Some(effort))
                    }
                    _ => assert!(result.is_err()),
                }
            }
            if matches!(style, ApiStyle::Responses) {
                for reasoning in [Value::Null, json!({}), json!({"summary":null})] {
                    let mut response = base.clone();
                    response["reasoning"] = reasoning;
                    assert_eq!(decode(response, style, "m").unwrap().reasoning_effort, None);
                }
                let mut invalid = base;
                invalid["reasoning"] = json!("invalid-container");
                assert!(decode(invalid, style, "m").is_err());
            }
        }
    }
}
