//! Bounded HTTP attempts with explicit transient-error recovery and transport receipts.
use crate::{Error, provider::Role};
use reqwest::{Client, Response, Url};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::Semaphore;

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
/// Credential-free receipt. Strings are fixed application labels or allowlisted codes.
#[derive(Debug, Clone, Serialize)]
pub struct HttpEvent {
    pub role: Role,
    pub attempt: u8,
    pub status: u16,
    pub kind: &'static str,
    pub provider_code: Option<&'static str>,
    pub wait_ms: Option<u64>,
}
pub type HttpObserver = Arc<dyn Fn(&HttpEvent) -> Result<(), Error> + Send + Sync>;

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
            .connect_timeout(Duration::from_secs(15))
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
    pub async fn post(&self, body: Value, role: Role) -> Result<Value, Error> {
        // Queueing, bodies and retry waits share this one logical deadline.
        let deadline = tokio::time::Instant::now()
            .checked_add(self.timeout)
            .ok_or_else(|| {
                Error::InvalidInput("request timeout exceeds the supported clock range".into())
            })?;
        tokio::time::timeout_at(deadline, self.post_bounded(body, role, deadline))
            .await
            .map_err(|_| {
                Error::Provider("request timed out including queue and retry waits".into())
            })?
    }
    async fn post_bounded(
        &self,
        body: Value,
        role: Role,
        deadline: tokio::time::Instant,
    ) -> Result<Value, Error> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::Provider("HTTP admission closed".into()))?;
        for attempt in 1..=self.config.max_http_retries + 1 {
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Provider(
                    "request timed out including queue and retry waits".into(),
                ));
            }
            let mut builder = self.client.post(self.endpoint.clone()).json(&body);
            if let Some(key) = &self.api_key {
                builder = builder.bearer_auth(key);
            }
            let response = builder.send().await.map_err(|e| {
                Error::Provider(
                    if e.is_timeout() {
                        "request timed out"
                    } else {
                        "network request failed"
                    }
                    .into(),
                )
            })?;
            let status = response.status();
            let mut event = HttpEvent {
                role,
                attempt,
                status: status.as_u16(),
                kind: "response",
                provider_code: None,
                wait_ms: None,
            };
            (self.observer)(&event)?;
            if status.is_success() {
                let bytes = read_body(response, self.max_body_bytes).await?;
                return serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Provider("response was not JSON".into()));
            }
            if !super::retry_policy::retryable_status(status.as_u16()) {
                return Err(Error::Provider(format!(
                    "HTTP {status}; no automatic retry"
                )));
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
                    )));
                }
            }
        }
        unreachable!("the final rejected attempt exhausts the retry budget")
    }
}
async fn read_body(mut response: Response, limit: usize) -> Result<Vec<u8>, Error> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(Error::Budget("HTTP response exceeds byte limit".into()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::Provider("response read failed".into()))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(Error::Budget("HTTP response exceeds byte limit".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
