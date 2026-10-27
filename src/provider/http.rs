//! Bounded HTTP attempts with explicit transient-error recovery and transport receipts.
use crate::{
    Error,
    provider::{ResponseFailure, Role},
};
use reqwest::{Client, Response, Url};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::Semaphore;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// CLI transport policy, separate from logical model calls and case repair.
#[derive(Clone, Debug, Serialize)]
pub struct TransportConfig {
    pub max_http_retries: u8,
    pub retry_max_delay_secs: u64,
    /// Zero leaves library consumers unbounded. The CLI accepts 1 through 64.
    pub max_concurrent_requests: usize,
}
impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            max_http_retries: 3,
            retry_max_delay_secs: 120,
            max_concurrent_requests: 3,
        }
    }
}
impl TransportConfig {
    pub(super) fn disabled() -> Self {
        Self {
            max_http_retries: 0,
            max_concurrent_requests: 0,
            ..Self::default()
        }
    }
    fn validate(&self) -> Result<(), Error> {
        if self.max_http_retries > 10
            || self.retry_max_delay_secs == 0
            || self.retry_max_delay_secs > 3600
            || self.max_concurrent_requests > 64
        {
            return Err(Error::InvalidInput("HTTP retries require 0..10 retries, 1..3600 delay seconds and 0..64 concurrent requests".into()));
        }
        Ok(())
    }
}
/// Credential-free receipt with fixed labels and bounded response metadata.
#[derive(Debug, Clone, Serialize)]
pub struct HttpEvent {
    pub role: Role,
    pub attempt: u8,
    /// Zero when no HTTP response was received for this attempt.
    pub status: u16,
    pub kind: &'static str,
    pub provider_code: Option<&'static str>,
    pub wait_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_failure: Option<Box<ResponseFailure>>,
}
pub type HttpObserver = Arc<dyn Fn(&HttpEvent) -> Result<(), Error> + Send + Sync>;

// Preserve timeout classification until the logical-call boundary, where both
// reqwest timeouts and the shared deadline produce one durable receipt.
enum HttpFailure {
    Timeout,
    ConnectionTimeout,
    Other(Error),
}
impl From<Error> for HttpFailure {
    fn from(error: Error) -> Self {
        Self::Other(error)
    }
}
enum CallPhase {
    Admission,
    Request,
    ResponseBody,
    RetryWait,
}
impl CallPhase {
    fn label(&self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Request => "request",
            Self::ResponseBody => "response_body",
            Self::RetryWait => "retry_wait",
        }
    }
    fn timeout_kind(&self) -> &'static str {
        match self {
            Self::Admission => "admission_timeout",
            Self::Request => "request_timeout",
            Self::ResponseBody => "response_body_timeout",
            Self::RetryWait => "retry_wait_timeout",
        }
    }
}
struct CallProgress {
    attempt: u8,
    status: u16,
    phase: CallPhase,
}

pub(super) struct HttpResponse {
    pub value: Value,
    pub attempt: u8,
    pub status: u16,
}

pub(super) struct HttpTransport {
    client: Client,
    endpoint: Url,
    api_key: Option<String>,
    timeout: Duration,
    max_body_bytes: usize,
    config: TransportConfig,
    permits: Semaphore,
    observer: HttpObserver,
}
impl HttpTransport {
    pub fn new(
        endpoint: Url,
        api_key: Option<String>,
        timeout: Duration,
        max_body_bytes: usize,
        config: TransportConfig,
        observer: HttpObserver,
    ) -> Result<Self, Error> {
        config.validate()?;
        let client = Client::builder()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("engarg/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| Error::Provider("could not construct TLS client".into()))?;
        let count = if config.max_concurrent_requests == 0 {
            Semaphore::MAX_PERMITS
        } else {
            config.max_concurrent_requests
        };
        Ok(Self {
            client,
            endpoint,
            api_key,
            timeout,
            max_body_bytes,
            config,
            permits: Semaphore::new(count),
            observer,
        })
    }
    pub async fn post(&self, body: Value, role: Role) -> Result<HttpResponse, Error> {
        // Queueing, bodies and retry waits share this one logical deadline.
        let deadline = tokio::time::Instant::now()
            .checked_add(self.timeout)
            .ok_or_else(|| {
                Error::InvalidInput("request timeout exceeds the supported clock range".into())
            })?;
        let mut progress = CallProgress {
            attempt: 0,
            status: 0,
            phase: CallPhase::Admission,
        };
        let result = tokio::time::timeout_at(
            deadline,
            self.post_bounded(body, role, deadline, &mut progress),
        )
        .await
        .unwrap_or(Err(HttpFailure::Timeout));
        match result {
            Ok(value) => Ok(value),
            Err(HttpFailure::Other(error)) => Err(error),
            Err(failure @ (HttpFailure::Timeout | HttpFailure::ConnectionTimeout)) => {
                let connection_timeout = matches!(failure, HttpFailure::ConnectionTimeout);
                (self.observer)(&HttpEvent {
                    role,
                    attempt: progress.attempt,
                    status: progress.status,
                    kind: if connection_timeout {
                        "connection_timeout"
                    } else {
                        progress.phase.timeout_kind()
                    },
                    provider_code: None,
                    wait_ms: None,
                    response_failure: None,
                })?;
                Err(Error::Provider(timeout_diagnostic(
                    self.timeout,
                    role,
                    &progress,
                    connection_timeout,
                )))
            }
        }
    }
    pub fn reject_response(
        &self,
        role: Role,
        attempt: u8,
        status: u16,
        failure: Box<ResponseFailure>,
    ) -> Result<(), Error> {
        (self.observer)(&HttpEvent {
            role,
            attempt,
            status,
            kind: "response_rejected",
            provider_code: None,
            wait_ms: None,
            response_failure: Some(failure),
        })
    }

    async fn post_bounded(
        &self,
        body: Value,
        role: Role,
        deadline: tokio::time::Instant,
        progress: &mut CallProgress,
    ) -> Result<HttpResponse, HttpFailure> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::Provider("HTTP admission closed".into()))?;
        for attempt in 1..=self.config.max_http_retries + 1 {
            if tokio::time::Instant::now() >= deadline {
                return Err(HttpFailure::Timeout);
            }
            progress.attempt = attempt;
            progress.status = 0;
            progress.phase = CallPhase::Request;
            let mut builder = self.client.post(self.endpoint.clone()).json(&body);
            if let Some(key) = &self.api_key {
                builder = builder.bearer_auth(key);
            }
            let response = builder.send().await.map_err(|error| {
                if error.is_timeout() && error.is_connect() {
                    HttpFailure::ConnectionTimeout
                } else if error.is_timeout() {
                    HttpFailure::Timeout
                } else {
                    Error::Provider("network request failed; no automatic retry".into()).into()
                }
            })?;
            let status = response.status();
            progress.status = status.as_u16();
            progress.phase = CallPhase::ResponseBody;
            let mut event = HttpEvent {
                role,
                attempt,
                status: status.as_u16(),
                kind: "response",
                provider_code: None,
                wait_ms: None,
                response_failure: None,
            };
            (self.observer)(&event)?;
            if status.is_success() {
                let bytes = read_body(response, self.max_body_bytes).await?;
                let value = match serde_json::from_slice(&bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        event.kind = "invalid_response_json";
                        (self.observer)(&event)?;
                        return Err(Error::Provider(format!(
                            "{role}: response was not JSON; no automatic retry"
                        ))
                        .into());
                    }
                };
                return Ok(HttpResponse {
                    value,
                    attempt,
                    status: status.as_u16(),
                });
            }
            if !super::retry_policy::retryable_status(status.as_u16()) {
                return Err(Error::Provider(format!("HTTP {status}; no automatic retry")).into());
            }
            let headers = response.headers().clone();
            let bytes = read_body(response, self.max_body_bytes.min(65536)).await?;
            event.provider_code = super::retry_policy::error_code(&bytes);
            let permanent = super::retry_policy::permanent(event.provider_code);
            let outcome = if permanent {
                Err("quota_or_billing_limit")
            } else if attempt > self.config.max_http_retries {
                Err("retry_budget_exhausted")
            } else {
                super::retry_policy::delay(
                    &headers,
                    attempt,
                    Duration::from_secs(self.config.retry_max_delay_secs),
                    SystemTime::now(),
                )
            };
            let outcome = outcome.and_then(|delay| {
                if delay >= deadline.saturating_duration_since(tokio::time::Instant::now()) {
                    Err("request_deadline_insufficient")
                } else {
                    Ok(delay)
                }
            });
            match outcome {
                Ok(delay) => {
                    event.kind = "retry";
                    event.wait_ms = Some(delay.as_millis() as u64);
                    (self.observer)(&event)?;
                    progress.phase = CallPhase::RetryWait;
                    tokio::time::sleep(delay).await;
                }
                Err(reason) => {
                    event.kind = reason;
                    (self.observer)(&event)?;
                    let code = event.provider_code.unwrap_or(if status.as_u16() == 429 {
                        "unclassified_429"
                    } else {
                        "transient_server_error"
                    });
                    let help = if permanent {
                        "update API credits or usage/spend limits"
                    } else if self.config.max_http_retries == 0 {
                        "no automatic retry"
                    } else {
                        reason
                    };
                    return Err(Error::Provider(format!(
                        "HTTP {status} ({code}); {help}; attempts {attempt}"
                    ))
                    .into());
                }
            }
        }
        unreachable!("the final rejected attempt exhausts the retry budget")
    }
}
fn timeout_diagnostic(
    deadline: Duration,
    role: Role,
    progress: &CallProgress,
    connection_timeout: bool,
) -> String {
    let (cause, help) = if connection_timeout {
        (
            format!(
                "connection timed out (connection limit {:.3}s; configured logical-call deadline {:.3}s",
                CONNECT_TIMEOUT.as_secs_f64(),
                deadline.as_secs_f64()
            ),
            "check network reachability and proxy configuration; --request-timeout-secs does not raise the connection limit",
        )
    } else {
        (
            format!(
                "request timed out (logical-call deadline {:.3}s",
                deadline.as_secs_f64()
            ),
            "adjust --request-timeout-secs and the overall --timeout-secs if a longer wait is intended",
        )
    };
    format!(
        "{cause}; role {role}; attempt {}; phase {}); no automatic retry; inspect any retained audit/checkpoint before starting another call; {help}; cancellation does not confirm remote completion or billing",
        progress.attempt,
        progress.phase.label()
    )
}

async fn read_body(mut response: Response, limit: usize) -> Result<Vec<u8>, HttpFailure> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(Error::Budget("HTTP response exceeds byte limit".into()).into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        if error.is_timeout() {
            HttpFailure::Timeout
        } else {
            Error::Provider("response read failed; no automatic retry".into()).into()
        }
    })? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(Error::Budget("HTTP response exceeds byte limit".into()).into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_timeout_does_not_claim_that_a_longer_logical_deadline_expired() {
        let diagnostic = timeout_diagnostic(
            Duration::from_secs(600),
            Role::Sceptic,
            &CallProgress {
                attempt: 1,
                status: 0,
                phase: CallPhase::Request,
            },
            true,
        );
        assert!(diagnostic.contains("connection timed out (connection limit 15.000s"));
        assert!(diagnostic.contains("configured logical-call deadline 600.000s"));
        assert!(diagnostic.contains("check network reachability and proxy configuration"));
        assert!(diagnostic.contains("--request-timeout-secs does not raise the connection limit"));
        assert!(!diagnostic.contains("adjust --request-timeout-secs"));
        assert!(diagnostic.contains("no automatic retry"));
    }
}
