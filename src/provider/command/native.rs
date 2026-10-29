//! Shared text request for assistants without native JSON Schema parameters.
use super::{Completion, CompletionRequest};
use crate::Error;
use serde_json::json;

pub(super) const INSTRUCTION: &str = "Follow the complete EngArg request supplied on stdin. Return only the requested JSON value, with no Markdown fences or commentary. Do not use tools or inspect files.";

pub(super) fn prompt(
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Vec<u8>, Error> {
    Ok(serde_json::to_vec(&json!({
        "instruction": INSTRUCTION,
        "max_output_tokens": max_output_tokens,
        "system": request.system,
        "user": request.user,
        "output_schema": request.schema,
        "role": request.role,
    }))?)
}

pub(super) fn completion(
    content: String,
    assistant: &str,
    model: &str,
    request_id: Option<String>,
) -> Completion {
    Completion {
        content,
        model: format!("assistant:{assistant}:{model}"),
        input_tokens: None,
        output_tokens: None,
        request_id,
        service_tier: None,
        reasoning_effort: None,
    }
}
