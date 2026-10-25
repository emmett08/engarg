//! Bound and audit implementation model calls before retaining response content.
use crate::{
    Error,
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Instant};

#[derive(Clone, Copy)]
pub struct CompletionLimits {
    pub max_calls: u32,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
}

pub struct CompletionBudget {
    limits: CompletionLimits,
    calls: Mutex<u32>,
}

pub enum CompletionEvent<'a> {
    Request {
        call: u32,
        request: &'a CompletionRequest,
    },
    Completed {
        call: u32,
        completion: &'a Completion,
        elapsed_ms: u128,
    },
    ProviderFailed {
        call: u32,
        elapsed_ms: u128,
    },
    ResponseLimit {
        call: u32,
        completion: &'a Completion,
        response_bytes: usize,
        elapsed_ms: u128,
    },
}

impl CompletionEvent<'_> {
    pub fn record(&self) -> Value {
        match self {
            Self::Request { call, request } => {
                json!({"event":"request","call":call,"request":request})
            }
            Self::Completed {
                call,
                completion,
                elapsed_ms,
            } => {
                json!({"event":"completion","call":call,"completion":completion,"elapsed_ms":elapsed_ms})
            }
            Self::ProviderFailed { call, elapsed_ms } => {
                json!({"event":"provider_failure","call":call,"elapsed_ms":elapsed_ms})
            }
            Self::ResponseLimit {
                call,
                completion,
                response_bytes,
                elapsed_ms,
            } => {
                // Provider identifiers are metadata, but still bounded independently
                // of the rejected body so a hostile response cannot flood the audit.
                fn bounded(text: &str) -> String {
                    text.chars().take(256).collect()
                }
                json!({"event":"response_limit","call":call,"response_bytes":response_bytes,"elapsed_ms":elapsed_ms,"response_content_omitted":true,
                    "model":bounded(&completion.model),"request_id":completion.request_id.as_deref().map(bounded),
                    "input_tokens":completion.input_tokens,"output_tokens":completion.output_tokens})
            }
        }
    }
}

impl CompletionBudget {
    pub fn new(limits: CompletionLimits) -> Result<Self, Error> {
        if limits.max_calls > 256 || limits.max_input_bytes == 0 || limits.max_response_bytes == 0 {
            return Err(Error::InvalidInput(
                "invalid implementation completion budget".into(),
            ));
        }
        Ok(Self {
            limits,
            calls: Mutex::new(0),
        })
    }
    pub fn calls(&self) -> Result<u32, Error> {
        self.calls
            .lock()
            .map(|calls| *calls)
            .map_err(|_| Error::Provider("implementation call budget unavailable".into()))
    }
    pub async fn complete<M, A>(
        &self,
        model: &M,
        request: CompletionRequest,
        audit: A,
    ) -> Result<Completion, Error>
    where
        M: LanguageModel,
        A: Fn(CompletionEvent<'_>) -> Result<(), Error> + Send + Sync,
    {
        let call = {
            let mut calls = self
                .calls
                .lock()
                .map_err(|_| Error::Provider("implementation call budget unavailable".into()))?;
            if *calls >= self.limits.max_calls {
                return Err(Error::Budget(
                    "tool implementation model call budget exhausted".into(),
                ));
            }
            if serde_json::to_vec(&request)?.len() > self.limits.max_input_bytes {
                return Err(Error::Budget(
                    "tool implementation model request exceeds its input byte budget".into(),
                ));
            }
            let call = *calls + 1;
            audit(CompletionEvent::Request {
                call,
                request: &request,
            })?;
            *calls = call;
            call
        };
        let started = Instant::now();
        let completion = match model.complete(request).await {
            Ok(completion) => completion,
            Err(error) => {
                audit(CompletionEvent::ProviderFailed {
                    call,
                    elapsed_ms: started.elapsed().as_millis(),
                })?;
                return Err(error);
            }
        };
        if completion.content.len() > self.limits.max_response_bytes {
            audit(CompletionEvent::ResponseLimit {
                call,
                completion: &completion,
                response_bytes: completion.content.len(),
                elapsed_ms: started.elapsed().as_millis(),
            })?;
            return Err(Error::Budget(
                "tool implementation model response exceeds its byte budget".into(),
            ));
        }
        audit(CompletionEvent::Completed {
            call,
            completion: &completion,
            elapsed_ms: started.elapsed().as_millis(),
        })?;
        Ok(completion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use std::sync::atomic::{AtomicU32, Ordering};
    struct Model {
        calls: AtomicU32,
        content: String,
    }
    impl LanguageModel for Model {
        async fn complete(&self, _: CompletionRequest) -> Result<Completion, Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Completion {
                content: self.content.clone(),
                model: "fixture".into(),
                input_tokens: Some(31),
                output_tokens: Some(1024),
                request_id: Some("fixture-request".into()),
                service_tier: None,
                reasoning_effort: None,
            })
        }
    }
    fn request() -> CompletionRequest {
        CompletionRequest {
            role: Role::Single,
            system: "bounded".into(),
            user: "operation".into(),
            schema: json!({}),
        }
    }
    #[tokio::test]
    async fn oversized_responses_are_rejected_before_audit_content_is_retained() {
        let model = Model {
            calls: AtomicU32::new(0),
            content: "unbounded-private-model-text".repeat(100),
        };
        let budget = CompletionBudget::new(CompletionLimits {
            max_calls: 1,
            max_input_bytes: 1024,
            max_response_bytes: 8,
        })
        .unwrap();
        let records = Mutex::new(Vec::new());
        let result = budget
            .complete(&model, request(), |event| {
                records.lock().unwrap().push(event.record());
                Ok(())
            })
            .await;
        assert!(matches!(result, Err(Error::Budget(_))));
        assert_eq!(budget.calls().unwrap(), 1);
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        let records = records.into_inner().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["event"], "response_limit");
        assert_eq!(records[1]["input_tokens"], 31);
        assert_eq!(records[1]["output_tokens"], 1024);
        assert_eq!(records[1]["request_id"], "fixture-request");
        assert_eq!(records[1]["model"], "fixture");
        assert!(
            records
                .iter()
                .all(|record| record.get("completion").is_none())
        );
        assert!(
            !serde_json::to_string(&records)
                .unwrap()
                .contains("unbounded-private-model-text")
        );
    }
    #[tokio::test]
    async fn failed_request_audit_admits_no_model_call_or_budget_charge() {
        let model = Model {
            calls: AtomicU32::new(0),
            content: "small".into(),
        };
        let budget = CompletionBudget::new(CompletionLimits {
            max_calls: 1,
            max_input_bytes: 1024,
            max_response_bytes: 16,
        })
        .unwrap();
        assert!(
            budget
                .complete(&model, request(), |_| Err(Error::InvalidInput(
                    "audit failed".into()
                )))
                .await
                .is_err()
        );
        assert_eq!(budget.calls().unwrap(), 0);
        assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    }
}
