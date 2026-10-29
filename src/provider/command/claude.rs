//! Claude Code print-mode requests and completed result-envelope admission.
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
            "--input-format",
            "text",
            "--output-format",
            "json",
            "--model",
            model,
            // Safe mode preserves normal authentication, unlike --bare.
            "--safe-mode",
            "--tools",
            "",
            "--disallowedTools",
            "mcp__*",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--no-session-persistence",
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
    let value = eal_collector_contracts::strict_json_exact(
        bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: bytes.len().max(1),
            ..Default::default()
        },
    )
    .map_err(|_| invalid())?;
    if value["type"] != "result" || value["subtype"] != "success" || value["is_error"] != false {
        return Err(invalid());
    }
    if let Some(reason) = value.get("stop_reason")
        && !reason.is_null()
        && !matches!(reason.as_str(), Some("end_turn" | "stop_sequence"))
    {
        return Err(invalid());
    }
    let content = value["result"].as_str().ok_or_else(invalid)?;
    let mut completion = native::completion(content.to_owned(), "claude", model, None);
    if let Some(actual) = value.get("model") {
        completion.model = actual
            .as_str()
            .filter(|model| super::valid_model(model))
            .ok_or_else(invalid)?
            .to_owned();
    }
    if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
        if !usage.is_object() {
            return Err(invalid());
        }
        let input = counter(usage, "input_tokens")?;
        let creation = counter(usage, "cache_creation_input_tokens")?;
        let read = counter(usage, "cache_read_input_tokens")?;
        // Anthropic reports uncached and cached input separately. Do not
        // mislabel an incomplete set of counters as the total input usage.
        completion.input_tokens = match (input, creation, read) {
            (Some(input), Some(creation), Some(read)) => Some(
                input
                    .checked_add(creation)
                    .and_then(|sum| sum.checked_add(read))
                    .ok_or_else(invalid)?,
            ),
            _ => None,
        };
        completion.output_tokens = counter(usage, "output_tokens")?;
    }
    Ok(completion)
}

fn counter(usage: &Value, name: &str) -> Result<Option<u64>, Error> {
    match usage.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(invalid),
    }
}

fn invalid() -> Error {
    Error::Provider(
        "Claude Code did not return a completed valid result; no automatic retry".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use serde_json::json;

    fn result() -> Value {
        json!({"type":"result","subtype":"success","is_error":false,"result":"{\"answer\":1}"})
    }

    #[test]
    fn print_mode_preserves_stdin_context_and_disables_tools_without_bare_auth() {
        let request = CompletionRequest {
            role: Role::Sceptic,
            system: "Keep provenance separate.".into(),
            user: "Literal $(command)\nsecond line".into(),
            schema: json!({"type":"object"}),
        };
        let invocation = encode(&["claude".into()], "chosen", &request, 123).unwrap();
        let prompt: Value = serde_json::from_slice(&invocation.input).unwrap();
        assert_eq!(prompt["system"], request.system);
        assert_eq!(prompt["user"], request.user);
        assert_eq!(prompt["output_schema"], request.schema);
        assert_eq!(prompt["max_output_tokens"], 123);
        assert!(
            invocation
                .argv
                .windows(2)
                .any(|pair| pair == ["--tools", ""])
        );
        assert!(
            invocation
                .argv
                .windows(2)
                .any(|pair| pair == ["--disallowedTools", "mcp__*"])
        );
        assert!(invocation.argv.contains(&"--safe-mode".into()));
        assert!(!invocation.argv.contains(&"--bare".into()));
        assert!(!invocation.argv.iter().any(|arg| arg.contains("$(command)")));
    }

    #[test]
    fn result_text_and_reported_usage_are_preserved_without_inferred_model_identity() {
        let completion = decode(&serde_json::to_vec(&result()).unwrap(), "chosen").unwrap();
        assert_eq!(completion.content, "{\"answer\":1}");
        assert_eq!(completion.model, "assistant:claude:chosen");
        assert_eq!(completion.input_tokens, None);
        assert_eq!(completion.request_id, None);
        let mut value = result();
        value["model"] = json!("actual");
        value["usage"] = json!({"input_tokens":3,"cache_creation_input_tokens":4,"cache_read_input_tokens":5,"output_tokens":7});
        let completion = decode(&serde_json::to_vec(&value).unwrap(), "chosen").unwrap();
        assert_eq!(completion.model, "actual");
        assert_eq!(completion.input_tokens, Some(12));
        assert_eq!(completion.output_tokens, Some(7));
        value["usage"] = json!({"input_tokens":3,"output_tokens":7});
        assert_eq!(
            decode(&serde_json::to_vec(&value).unwrap(), "m")
                .unwrap()
                .input_tokens,
            None
        );
    }

    #[test]
    fn incomplete_refused_failed_and_malformed_results_are_rejected() {
        for (field, value) in [
            ("type", json!("assistant")),
            ("subtype", json!("error_max_turns")),
            ("is_error", json!(true)),
            ("result", json!({})),
            ("stop_reason", json!("max_tokens")),
            ("stop_reason", json!("refusal")),
            ("model", json!("invalid\nmodel")),
            ("usage", json!([])),
            ("usage", json!({"input_tokens":-1})),
            ("usage", json!({"cache_read_input_tokens":"secret"})),
            (
                "usage",
                json!({"input_tokens":u64::MAX,"cache_creation_input_tokens":1,"cache_read_input_tokens":0}),
            ),
        ] {
            let mut body = result();
            body[field] = value;
            assert!(
                decode(&serde_json::to_vec(&body).unwrap(), "m").is_err(),
                "{field}"
            );
        }
        for bytes in [
            br#"{"type":"result","type":"result","subtype":"success","is_error":false,"result":"{}"}"#.as_slice(),
            b"{} {}",
            &[0xff],
        ] {
            assert!(decode(bytes, "m").is_err());
        }
    }
}
