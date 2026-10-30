//! Persist bounded model exchanges before interpretation and account for failed calls.
use super::{DevelopmentCall, DevelopmentConfig, DevelopmentFailure};
use crate::{
    Error,
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde_json::{Value, json};
use std::time::Instant;

pub(super) struct Ledger {
    pub attempted: u16,
    pub calls: Vec<DevelopmentCall>,
}

pub(super) struct CallContext {
    pub round: u8,
    pub parent_index: usize,
    pub phase: &'static str,
}

pub(super) async fn complete<M: LanguageModel, O: Fn(&Value) -> Result<(), Error> + Sync>(
    model: &M,
    request: CompletionRequest,
    config: &DevelopmentConfig,
    context: CallContext,
    ledger: &mut Ledger,
    observer: &O,
) -> Result<Result<Completion, DevelopmentFailure>, Error> {
    let fail = |phase: &str, index: Option<u16>, reason: String| DevelopmentFailure {
        round: context.round,
        parent_index: context.parent_index,
        call_index: index,
        phase: phase.into(),
        reason,
    };
    if ledger.attempted >= config.max_calls {
        return Ok(Err(fail(
            "call_budget",
            None,
            "model-call budget exhausted".into(),
        )));
    }
    let bytes = serde_json::to_vec(&request)?.len();
    if bytes > config.max_input_bytes {
        return Ok(Err(fail(
            "input_limit",
            None,
            format!(
                "{} request uses {bytes} bytes, exceeding {}",
                context.phase, config.max_input_bytes
            ),
        )));
    }
    let index = ledger.attempted;
    observer(
        &json!({"event":"model_request","call_index":index,"round":context.round,
        "parent_index":context.parent_index,"phase":context.phase,"request":request}),
    )?;
    ledger.attempted += 1;
    let started = Instant::now();
    let completed = model.complete(request.clone()).await;
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut summary = DevelopmentCall {
        index,
        round: context.round,
        parent_index: context.parent_index,
        role: request.role,
        phase: context.phase.into(),
        completed: false,
        response_content_bytes: None,
        response_content_sha256: None,
        response_content_omitted: false,
        model: None,
        input_tokens: None,
        output_tokens: None,
        elapsed_ms,
        error: None,
    };
    let mut response = match completed {
        Ok(response) => response,
        Err(error) => {
            summary.error = Some(error.to_string());
            observer(&json!({"event":"model_failure","call":summary}))?;
            ledger.calls.push(summary);
            return Ok(Err(fail(context.phase, Some(index), error.to_string())));
        }
    };
    let response_bytes = response.content.len();
    let digest = crate::authoring_checkpoint::digest(response.content.as_bytes());
    let omitted = response_bytes > config.max_response_bytes;
    summary.completed = true;
    summary.response_content_bytes = Some(response_bytes);
    summary.response_content_sha256 = Some(digest);
    summary.response_content_omitted = omitted;
    summary.model = Some(response.model.clone());
    summary.input_tokens = response.input_tokens;
    summary.output_tokens = response.output_tokens;
    if omitted {
        // Preserve the same bounded overflow policy as initial authoring. A
        // completion above the admitted byte ceiling is never parsed or fed back.
        response.content.clear();
    }
    observer(&json!({"event":"model_completion","call":summary,"completion":response}))?;
    ledger.calls.push(summary);
    if omitted {
        return Ok(Err(fail(
            "response_limit",
            Some(index),
            format!(
                "{} completion uses {response_bytes} bytes, exceeding {}; content omitted with digest retained",
                context.phase, config.max_response_bytes
            ),
        )));
    }
    Ok(Ok(response))
}
