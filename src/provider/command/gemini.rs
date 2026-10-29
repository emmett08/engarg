//! Gemini CLI's noninteractive final JSON envelope.
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
    // A non-TTY stdin selects headless mode. Keep approval explicit without
    // enabling experimental plan mode or granting unattended tool permissions.
    argv.extend(
        [
            "--output-format",
            "json",
            "--model",
            model,
            "--approval-mode",
            "default",
            "--extensions",
            "none",
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
        Error::Provider("Gemini returned an invalid JSON envelope; no automatic retry".into())
    })?;
    let object = value.as_object().ok_or_else(|| {
        Error::Provider("Gemini returned an invalid JSON envelope; no automatic retry".into())
    })?;
    if object.get("error").is_some_and(|error| !error.is_null()) {
        return Err(Error::Provider(
            "Gemini did not return a successful result; no automatic retry".into(),
        ));
    }
    if object.get("stats").is_some_and(|stats| !stats.is_object()) {
        return Err(Error::Provider(
            "Gemini returned invalid statistics metadata; no automatic retry".into(),
        ));
    }
    let content = object
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::Provider(
                "Gemini success envelope has no response text; no automatic retry".into(),
            )
        })?;
    // Statistics can aggregate several routed models and distinguish cached,
    // candidate and thought tokens. Do not infer one actual model or map those
    // aggregates to request usage without a supported accounting contract.
    Ok(native::completion(
        content.to_owned(),
        "gemini",
        model,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use serde_json::json;

    #[test]
    fn headless_request_preserves_schema_context_and_uses_default_approval() {
        let request = CompletionRequest {
            role: Role::Sceptic,
            system: "Retain missing evidence.".into(),
            user: "Literal $(command) 'quoted'\ncontext".into(),
            schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        };
        let invocation = encode(&["gemini".into()], "selected-model", &request, 321).unwrap();
        assert_eq!(
            invocation.argv,
            [
                "gemini",
                "--output-format",
                "json",
                "--model",
                "selected-model",
                "--approval-mode",
                "default",
                "--extensions",
                "none",
            ]
        );
        let prompt: Value = serde_json::from_slice(&invocation.input).unwrap();
        assert_eq!(prompt["role"], "sceptic");
        assert_eq!(prompt["user"], request.user);
        assert_eq!(prompt["system"], request.system);
        assert_eq!(prompt["output_schema"], request.schema);
        assert_eq!(prompt["max_output_tokens"], 321);
    }

    #[test]
    fn final_response_preserves_authored_text_and_unknown_model_accounting() {
        let completion = decode(
            br#"{"session_id":"session","response":"{malformed candidate","stats":{"models":{"routed-a":{},"routed-b":{}}},"future_field":true}"#,
            "configured",
        )
        .unwrap();
        assert_eq!(completion.content, "{malformed candidate");
        assert_eq!(completion.model, "assistant:gemini:configured");
        assert_eq!(completion.input_tokens, None);
        assert_eq!(completion.output_tokens, None);
        assert_eq!(completion.request_id, None);
        assert!(decode(br#"{"response":"{}","error":null}"#, "m").is_ok());
    }

    #[test]
    fn failed_incomplete_and_ambiguous_envelopes_never_supply_a_candidate() {
        for bytes in [
            br#"{"error":{"type":"APIError","message":"private"},"response":"{}"}"#.as_slice(),
            br#"{"response":"{}","error":false}"#.as_slice(),
            br#"{"stats":{}}"#.as_slice(),
            br#"{"response":42}"#.as_slice(),
            br#"{"response":"{}","stats":null}"#.as_slice(),
            br#"{"response":"{}","response":"changed"}"#.as_slice(),
            br#"{"response":"{}"} {"response":"changed"}"#.as_slice(),
            br#"[{"response":"{}"}]"#.as_slice(),
            b"not JSON".as_slice(),
            &[0xff],
        ] {
            let error = decode(bytes, "m").unwrap_err().to_string();
            assert!(!error.contains("private"));
        }
    }
}
