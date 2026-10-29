//! Copilot CLI's documented buffered response-text protocol.
use super::{Completion, CompletionRequest, Invocation, native};
use crate::Error;

pub(super) fn encode(
    argv: &[String],
    model: &str,
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Invocation, Error> {
    let mut argv = argv.to_vec();
    // Piped input selects noninteractive mode. Adding -p would discard it.
    // Use the documented final text interface: normal JSONL event envelopes
    // are not a published stable contract, unlike workflow.result records.
    argv.extend(
        [
            "--silent",
            "--stream",
            "off",
            "--output-format",
            "text",
            "--no-ask-user",
            "--disable-builtin-mcps",
            "--deny-tool",
            "read,write,shell,url,memory",
            "--model",
            model,
        ]
        .map(str::to_owned),
    );
    Ok(Invocation {
        argv,
        input: native::prompt(request, max_output_tokens)?,
    })
}

pub(super) fn decode(bytes: &[u8], model: &str) -> Result<Completion, Error> {
    let content = std::str::from_utf8(bytes).map_err(|_| {
        Error::Provider("Copilot returned non-UTF-8 completion text; no automatic retry".into())
    })?;
    // The supervisor has already required a successful process exit. Preserve
    // the response for the common authored-JSON checks, including repair.
    Ok(native::completion(
        content.to_owned(),
        "copilot",
        model,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use serde_json::{Value, json};

    #[test]
    fn prompt_uses_stdin_and_buffered_noninteractive_text_without_permission_bypass() {
        let request = CompletionRequest {
            role: Role::Constructor,
            system: "Retain missing evidence.".into(),
            user: "Literal $(command) 'quoted'\ncontext".into(),
            schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        };
        let invocation = encode(
            &["operator-copilot".into(), "--no-color".into()],
            "selected-model",
            &request,
            123,
        )
        .unwrap();
        assert_eq!(
            invocation.argv,
            [
                "operator-copilot",
                "--no-color",
                "--silent",
                "--stream",
                "off",
                "--output-format",
                "text",
                "--no-ask-user",
                "--disable-builtin-mcps",
                "--deny-tool",
                "read,write,shell,url,memory",
                "--model",
                "selected-model",
            ]
        );
        let prompt: Value = serde_json::from_slice(&invocation.input).unwrap();
        assert_eq!(prompt["user"], request.user);
        assert_eq!(prompt["system"], request.system);
        assert_eq!(prompt["output_schema"], request.schema);
        assert_eq!(prompt["max_output_tokens"], 123);
    }

    #[test]
    fn response_text_preserves_the_candidate_and_does_not_invent_metadata() {
        for text in ["{\"answer\":true}\n", "incomplete authored candidate"] {
            let completion = decode(text.as_bytes(), "selected-model").unwrap();
            assert_eq!(completion.content, text);
            assert_eq!(completion.model, "assistant:copilot:selected-model");
            assert_eq!(completion.input_tokens, None);
            assert_eq!(completion.output_tokens, None);
            assert_eq!(completion.request_id, None);
        }
        assert!(decode(&[0xff], "model").is_err());
    }
}
