//! Versioned JSON exchange for operator-owned assistant adapters.
use super::{Completion, CompletionRequest, Invocation};
use crate::Error;
use serde_json::json;

pub(super) fn encode(
    argv: &[String],
    model: &str,
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Invocation, Error> {
    let mut input = serde_json::to_vec(&json!({
        "schema_version": "engarg.completion.v1",
        "model": model,
        "max_output_tokens": max_output_tokens,
        "request": request,
    }))?;
    input.push(b'\n');
    Ok(Invocation {
        argv: argv.to_vec(),
        input,
    })
}

pub(super) fn decode(bytes: &[u8]) -> Result<Completion, Error> {
    let value = eal_collector_contracts::strict_json_exact(
        bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: bytes.len().max(1),
            ..Default::default()
        },
    )
    .map_err(|_| {
        Error::Provider(
            "assistant adapter returned invalid bounded JSON; no automatic retry".into(),
        )
    })?;
    serde_json::from_value(value).map_err(|_| {
        Error::Provider(
            "assistant adapter returned an invalid Completion JSON object; no automatic retry"
                .into(),
        )
    })
}
