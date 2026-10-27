//! Admit completed model envelopes and retain bounded diagnostics for rejected ones.
use super::{ApiStyle, Completion, Role};
use crate::Error;
use serde::Serialize;
use serde_json::Value;

/// A rejected provider response, without generated text, refusal text or remote error messages.
/// Labels are application-owned. Response IDs are retained only in a bounded identifier form.
#[derive(Debug, Clone, Serialize)]
pub struct ResponseFailure {
    pub kind: &'static str,
    pub reason: &'static str,
    pub status: Option<&'static str>,
    pub finish_reason: Option<&'static str>,
    pub request_id: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub max_output_tokens: u32,
}

impl ResponseFailure {
    fn new(
        value: &Value,
        style: ApiStyle,
        max_output_tokens: u32,
        kind: &'static str,
        reason: &'static str,
    ) -> Self {
        let (input, output, details) = match style {
            ApiStyle::Responses => ("input_tokens", "output_tokens", "output_tokens_details"),
            ApiStyle::Chat => (
                "prompt_tokens",
                "completion_tokens",
                "completion_tokens_details",
            ),
        };
        Self {
            kind,
            reason,
            status: label(
                &value["status"],
                &[
                    "completed",
                    "incomplete",
                    "failed",
                    "cancelled",
                    "queued",
                    "in_progress",
                ],
            ),
            finish_reason: label(
                &value["choices"][0]["finish_reason"],
                &[
                    "stop",
                    "length",
                    "content_filter",
                    "tool_calls",
                    "function_call",
                ],
            ),
            request_id: value["id"]
                .as_str()
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 128
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                })
                .map(str::to_owned),
            input_tokens: value["usage"][input].as_u64(),
            output_tokens: value["usage"][output].as_u64(),
            reasoning_tokens: value["usage"][details]["reasoning_tokens"].as_u64(),
            max_output_tokens,
        }
    }

    pub(super) fn error(&self, role: Role) -> Error {
        let help = match self.reason {
            "max_output_tokens" => {
                "consider a larger --max-output-tokens budget (reasoning and visible output share it) for a new attempt"
            }
            "refusal" | "content_filter" => {
                "review the request and the provider's refusal or filtering policy"
            }
            _ if self.kind == "malformed" => "check the endpoint's selected API response format",
            _ => "inspect the response_rejected audit receipt before a new attempt",
        };
        Error::Provider(format!(
            "{role}: {} structured response ({}; max_output_tokens={}); {help}; no automatic retry",
            self.kind, self.reason, self.max_output_tokens,
        ))
    }
}

fn label(value: &Value, known: &[&'static str]) -> Option<&'static str> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(
            known
                .iter()
                .copied()
                .find(|known| value == known)
                .unwrap_or("unknown"),
        ),
        _ => Some("invalid"),
    }
}

type Rejection = (&'static str, &'static str);

pub(super) fn decode(
    value: Value,
    style: ApiStyle,
    fallback_model: &str,
    max_output_tokens: u32,
) -> Result<Completion, Box<ResponseFailure>> {
    let failure = |(kind, reason)| {
        Box::new(ResponseFailure::new(
            &value,
            style,
            max_output_tokens,
            kind,
            reason,
        ))
    };
    let content = match style {
        ApiStyle::Responses => responses_text(&value),
        ApiStyle::Chat => chat_text(&value),
    }
    .map_err(failure)?;
    if content.trim().is_empty() {
        return Err(failure(("malformed", "empty_output_text")));
    }
    let service_tier =
        optional_string(value.get("service_tier"), "invalid_service_tier").map_err(failure)?;
    let effort = match style {
        ApiStyle::Responses => match value.get("reasoning") {
            None | Some(Value::Null) => None,
            Some(Value::Object(reasoning)) => reasoning.get("effort"),
            Some(_) => return Err(failure(("malformed", "invalid_reasoning"))),
        },
        ApiStyle::Chat => value.get("reasoning_effort"),
    };
    let reasoning_effort = optional_string(effort, "invalid_reasoning_effort").map_err(failure)?;
    let (input, output) = match style {
        ApiStyle::Responses => ("input_tokens", "output_tokens"),
        ApiStyle::Chat => ("prompt_tokens", "completion_tokens"),
    };
    Ok(Completion {
        content,
        model: value["model"].as_str().unwrap_or(fallback_model).into(),
        input_tokens: value["usage"][input].as_u64(),
        output_tokens: value["usage"][output].as_u64(),
        request_id: value["id"].as_str().map(str::to_owned),
        service_tier,
        reasoning_effort,
    })
}

fn optional_string(
    value: Option<&Value>,
    reason: &'static str,
) -> Result<Option<String>, Rejection> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(("malformed", reason)),
    }
}

fn responses_text(value: &Value) -> Result<String, Rejection> {
    match value["status"].as_str() {
        Some("completed") => {}
        Some("incomplete") => {
            let reason = label(
                &value["incomplete_details"]["reason"],
                &[
                    "max_output_tokens",
                    "content_filter",
                    "max_messages",
                    "steered",
                ],
            )
            .unwrap_or("missing_incomplete_reason");
            return Err(("incomplete", reason));
        }
        Some("failed") => {
            let reason = label(
                &value["error"]["code"],
                &["server_error", "rate_limit_exceeded", "invalid_prompt"],
            )
            .unwrap_or("missing_error_code");
            return Err(("failed", reason));
        }
        Some("cancelled") => return Err(("incomplete", "cancelled")),
        Some("queued" | "in_progress") => return Err(("incomplete", "response_not_completed")),
        _ => return Err(("malformed", "invalid_status")),
    }
    let mut text = String::new();
    for item in value["output"]
        .as_array()
        .ok_or(("malformed", "invalid_output"))?
    {
        let item_type = item["type"]
            .as_str()
            .ok_or(("malformed", "invalid_output_item"))?;
        if item_type != "message" {
            // Reasoning and other non-message output contain no structured answer text.
            continue;
        }
        match item.get("status") {
            None | Some(Value::Null) => {}
            Some(Value::String(status)) if status == "completed" => {}
            Some(Value::String(status)) if status == "incomplete" => {
                return Err(("incomplete", "message_incomplete"));
            }
            _ => return Err(("malformed", "invalid_message_status")),
        }
        for block in item["content"]
            .as_array()
            .ok_or(("malformed", "invalid_message_content"))?
        {
            match block["type"].as_str() {
                Some("refusal") => return Err(("refused", "refusal")),
                Some("output_text") => text.push_str(
                    block["text"]
                        .as_str()
                        .ok_or(("malformed", "invalid_output_text"))?,
                ),
                _ => return Err(("malformed", "invalid_content_block")),
            }
        }
    }
    Ok(text)
}

fn chat_text(value: &Value) -> Result<String, Rejection> {
    let choices = value["choices"]
        .as_array()
        .filter(|choices| choices.len() == 1)
        .ok_or(("malformed", "invalid_choices"))?;
    let choice = &choices[0];
    // Refusal is explicit even when the endpoint supplies another finish reason.
    if !choice["message"]["refusal"].is_null() {
        return Err(("refused", "refusal"));
    }
    match choice["finish_reason"].as_str() {
        Some("stop") => {}
        Some("length") => return Err(("incomplete", "max_output_tokens")),
        Some("content_filter") => return Err(("incomplete", "content_filter")),
        Some("tool_calls" | "function_call") => return Err(("incomplete", "unexpected_tool_call")),
        _ => return Err(("malformed", "invalid_finish_reason")),
    }
    choice["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or(("malformed", "invalid_message_content"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn completed(text: &str) -> Value {
        json!({"status":"completed","output":[{"type":"reasoning"},{"type":"message","status":"completed","content":[{"type":"output_text","text":text}]}]})
    }

    #[test]
    fn completed_content_stays_available_for_downstream_schema_validation_and_repair() {
        for content in ["{}", "{malformed JSON", "[]"] {
            assert_eq!(
                decode(completed(content), ApiStyle::Responses, "m", 128)
                    .unwrap()
                    .content,
                content
            );
        }
    }

    #[test]
    fn unknown_labels_and_invalid_ids_never_echo_remote_values() {
        for id in [
            "private\nidentifier",
            "https://user:private@example.com/id",
            &"x".repeat(129),
        ] {
            let value = json!({"id":id,"status":"incomplete","incomplete_details":{"reason":"private provider explanation"},"output":[{"type":"reasoning","summary":"private summary"}],"usage":{"input_tokens":-1,"output_tokens":"private"}});
            let failure = decode(value, ApiStyle::Responses, "m", 128).unwrap_err();
            assert_eq!(failure.reason, "unknown");
            assert!(failure.request_id.is_none());
            assert!(failure.input_tokens.is_none());
            assert!(failure.output_tokens.is_none());
            assert!(!serde_json::to_string(&failure).unwrap().contains("private"));
            assert!(!failure.error(Role::Single).to_string().contains("private"));
        }
    }

    #[test]
    fn malformed_envelopes_and_noncompleted_states_remain_distinct() {
        for (value, kind, reason) in [
            (json!({}), "malformed", "invalid_status"),
            (json!({"status":"private"}), "malformed", "invalid_status"),
            (json!({"status":"cancelled"}), "incomplete", "cancelled"),
            (
                json!({"status":"queued"}),
                "incomplete",
                "response_not_completed",
            ),
            (
                json!({"status":"incomplete"}),
                "incomplete",
                "missing_incomplete_reason",
            ),
            (
                json!({"status":"completed","output":[null]}),
                "malformed",
                "invalid_output_item",
            ),
            (
                json!({"status":"completed","output":[{"content":[]}]}),
                "malformed",
                "invalid_output_item",
            ),
            (
                json!({"status":"completed","output":[{"type":"message","content":42}]}),
                "malformed",
                "invalid_message_content",
            ),
            (
                json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":42}]}]}),
                "malformed",
                "invalid_output_text",
            ),
            (
                json!({"status":"completed","output":[{"type":"message","content":[{"type":"unknown"}]}]}),
                "malformed",
                "invalid_content_block",
            ),
            (completed(" "), "malformed", "empty_output_text"),
        ] {
            let failure = decode(value, ApiStyle::Responses, "m", 128).unwrap_err();
            assert_eq!(failure.kind, kind);
            assert_eq!(failure.reason, reason);
        }
        for (field, expected) in [
            ("service_tier", "invalid_service_tier"),
            ("reasoning_effort", "invalid_reasoning_effort"),
        ] {
            let mut value =
                json!({"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]});
            value[field] = json!(42);
            assert_eq!(
                decode(value, ApiStyle::Chat, "m", 128).unwrap_err().reason,
                expected
            );
        }
    }
}
