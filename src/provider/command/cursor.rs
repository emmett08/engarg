//! Cursor Agent print-mode request and terminal success-envelope admission.
use super::{Completion, CompletionRequest, Invocation, native};
use crate::Error;
use serde_json::Value;

pub(super) fn encode(
    argv: &[String],
    model: &str,
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Invocation, Error> {
    let mut argv = argv.to_vec();
    argv.extend(
        [
            "--print",
            "--mode",
            "ask",
            "--output-format",
            "json",
            "--model",
            model,
            native::INSTRUCTION,
        ]
        .map(str::to_owned),
    );
    Ok(Invocation {
        argv,
        input: native::prompt(request, max_output_tokens)?,
    })
}

pub(super) fn decode(bytes: &[u8], model: &str) -> Result<Completion, Error> {
    let value: Value = eal_collector_contracts::strict_json_exact(
        bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: bytes.len().max(1),
            ..Default::default()
        },
    )
    .map_err(|_| {
        Error::Provider("Cursor returned an invalid JSON envelope; no automatic retry".into())
    })?;
    if value["type"] != "result" || value["subtype"] != "success" || value["is_error"] != false {
        return Err(Error::Provider(
            "Cursor did not return a completed success envelope; no automatic retry".into(),
        ));
    }
    let content = value["result"].as_str().ok_or_else(|| {
        Error::Provider("Cursor success envelope has no result text; no automatic retry".into())
    })?;
    let request_id = match value.get("request_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id))
            if !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) =>
        {
            Some(id.clone())
        }
        _ => {
            return Err(Error::Provider(
                "Cursor success envelope has invalid request metadata; no automatic retry".into(),
            ));
        }
    };
    let mut completion = native::completion(content.to_owned(), "cursor", model, request_id);
    // Some versions report an actual model. The documented terminal envelope
    // does not, so retain an explicitly labelled configured identity otherwise.
    if let Some(actual) = value.get("model") {
        let actual = actual
            .as_str()
            .filter(|model| super::valid_model(model))
            .ok_or_else(|| {
                Error::Provider("Cursor returned invalid model metadata; no automatic retry".into())
            })?;
        completion.model = actual.to_owned();
    }
    Ok(completion)
}
