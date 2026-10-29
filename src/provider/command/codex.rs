//! Codex exec requests and one-turn JSONL result admission.
use super::{Completion, CompletionRequest, Invocation, native};
use crate::Error;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn encode(
    argv: &[String],
    model: &str,
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Invocation, Error> {
    let mut argv = argv.to_vec();
    argv.extend(
        [
            "exec",
            "--json",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--ephemeral",
            "--color",
            "never",
            "--model",
            model,
            "-",
        ]
        .map(str::to_owned),
    );
    Ok(Invocation {
        argv,
        input: native::prompt(request, max_output_tokens)?,
    })
}

pub(super) fn decode(bytes: &[u8], model: &str) -> Result<Completion, Error> {
    let mut state = Transcript::default();
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let value = eal_collector_contracts::strict_json_exact(
            line,
            eal_collector_contracts::JsonLimits {
                max_bytes: line.len().max(1),
                ..Default::default()
            },
        )
        .map_err(|_| invalid())?;
        state.accept(value)?;
    }
    if !state.completed {
        return Err(invalid());
    }
    let mut completion =
        native::completion(state.answer.ok_or_else(invalid)?, "codex", model, None);
    if let Some(actual) = state.model {
        completion.model = actual;
    }
    completion.input_tokens = state.input_tokens;
    completion.output_tokens = state.output_tokens;
    Ok(completion)
}

#[derive(Default)]
struct Transcript {
    thread: bool,
    turn: bool,
    completed: bool,
    answer: Option<String>,
    final_answer: bool,
    model: Option<String>,
    agent_items: HashSet<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

impl Transcript {
    fn accept(&mut self, value: Value) -> Result<(), Error> {
        if self.completed {
            return Err(invalid());
        }
        if let Some(actual) = value.get("model") {
            let actual = actual
                .as_str()
                .filter(|model| super::valid_model(model))
                .ok_or_else(invalid)?;
            if self
                .model
                .as_deref()
                .is_some_and(|previous| previous != actual)
            {
                return Err(invalid());
            }
            self.model = Some(actual.to_owned());
        }
        match value["type"].as_str() {
            Some("thread.started") if !self.thread => {
                identifier(&value["thread_id"])?;
                self.thread = true;
            }
            Some("turn.started") if self.thread && !self.turn => self.turn = true,
            Some("item.started" | "item.updated" | "item.completed") if self.turn => {
                let item = value
                    .get("item")
                    .filter(|item| item.is_object())
                    .ok_or_else(invalid)?;
                let id = identifier(&item["id"])?;
                let kind = item["type"].as_str().ok_or_else(invalid)?;
                if value["type"] == "item.completed" && kind == "agent_message" {
                    if item
                        .get("status")
                        .is_some_and(|status| status != "completed")
                    {
                        return Err(invalid());
                    }
                    if !self.agent_items.insert(id.to_owned()) {
                        return Err(invalid());
                    }
                    let text = item["text"].as_str().ok_or_else(invalid)?;
                    match item.get("phase") {
                        None | Some(Value::Null) => {
                            if self.final_answer {
                                return Err(invalid());
                            }
                            self.answer = Some(text.to_owned());
                        }
                        Some(Value::String(phase)) if phase == "commentary" => {}
                        Some(Value::String(phase))
                            if phase == "final_answer" && !self.final_answer =>
                        {
                            self.final_answer = true;
                            self.answer = Some(text.to_owned());
                        }
                        _ => return Err(invalid()),
                    }
                }
            }
            Some("turn.completed") if self.turn => {
                if self
                    .answer
                    .as_deref()
                    .is_none_or(|answer| answer.trim().is_empty())
                {
                    return Err(invalid());
                }
                if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
                    if !usage.is_object() {
                        return Err(invalid());
                    }
                    self.input_tokens = counter(usage, "input_tokens")?;
                    self.output_tokens = counter(usage, "output_tokens")?;
                    if let (Some(reasoning), Some(total)) = (
                        counter(usage, "reasoning_output_tokens")?,
                        self.output_tokens,
                    ) && reasoning > total
                    {
                        return Err(invalid());
                    }
                    if let (Some(cached), Some(total)) =
                        (counter(usage, "cached_input_tokens")?, self.input_tokens)
                        && cached > total
                    {
                        return Err(invalid());
                    }
                }
                self.completed = true;
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}

fn identifier(value: &Value) -> Result<&str, Error> {
    value
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(invalid)
}

fn counter(usage: &Value, name: &str) -> Result<Option<u64>, Error> {
    match usage.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(invalid),
    }
}

fn invalid() -> Error {
    Error::Provider("Codex did not return one completed valid turn; no automatic retry".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use serde_json::json;

    fn events() -> Vec<Value> {
        vec![
            json!({"type":"thread.started","thread_id":"thread_1"}),
            json!({"type":"turn.started"}),
            json!({"type":"item.completed","item":{"id":"progress","type":"agent_message","text":"Checking context","phase":"commentary"}}),
            json!({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":"{\"answer\":1}","phase":"final_answer"}}),
            json!({"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":3,"output_tokens":7}}),
        ]
    }

    fn wire(events: &[Value]) -> Vec<u8> {
        events
            .iter()
            .map(|event| serde_json::to_string(event).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    #[test]
    fn exec_receives_context_over_stdin_in_a_fresh_read_only_session() {
        let request = CompletionRequest {
            role: Role::Single,
            system: "Keep observations separate.".into(),
            user: "Literal $(command)\nnext line".into(),
            schema: json!({"type":"object"}),
        };
        let invocation = encode(&["codex".into()], "chosen", &request, 123).unwrap();
        let input: Value = serde_json::from_slice(&invocation.input).unwrap();
        assert_eq!(input["system"], request.system);
        assert_eq!(input["user"], request.user);
        assert_eq!(input["output_schema"], request.schema);
        assert_eq!(invocation.argv.last().unwrap(), "-");
        assert!(
            invocation
                .argv
                .windows(2)
                .any(|pair| pair == ["--sandbox", "read-only"])
        );
        assert!(invocation.argv.contains(&"--ephemeral".into()));
        assert!(invocation.argv.contains(&"--skip-git-repo-check".into()));
        assert!(!invocation.argv.iter().any(|arg| arg.contains("$(command)")));
    }

    #[test]
    fn only_final_agent_text_is_admitted_after_successful_turn_completion() {
        let completion = decode(&wire(&events()), "chosen").unwrap();
        assert_eq!(completion.content, "{\"answer\":1}");
        assert_eq!(completion.model, "assistant:codex:chosen");
        assert_eq!(completion.input_tokens, Some(10));
        assert_eq!(completion.output_tokens, Some(7));
        assert_eq!(completion.request_id, None);
        let mut values = events();
        values[0]["model"] = json!("actual");
        values[4].as_object_mut().unwrap().remove("usage");
        let completion = decode(&wire(&values), "chosen").unwrap();
        assert_eq!(completion.model, "actual");
        assert_eq!(completion.input_tokens, None);
    }

    #[test]
    fn documented_unphased_messages_select_the_last_completed_text() {
        let values = vec![
            json!({"type":"thread.started","thread_id":"thread_1"}),
            json!({"type":"turn.started"}),
            json!({"type":"item.completed","item":{"id":"progress","type":"agent_message","text":"Progress is not authored JSON"}}),
            json!({"type":"item.completed","item":{"id":"answer","type":"agent_message","text":"{\"answer\":1}"}}),
            json!({"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":3,"output_tokens":7,"reasoning_output_tokens":2}}),
        ];
        let completion = decode(&wire(&values), "chosen").unwrap();
        assert_eq!(completion.content, "{\"answer\":1}");
        assert_eq!(completion.output_tokens, Some(7));
    }

    #[test]
    fn failed_truncated_ambiguous_and_malformed_streams_are_rejected() {
        let values = events();
        for index in [0, 1, 3, 4] {
            let mut broken = values.clone();
            broken.remove(index);
            assert!(decode(&wire(&broken), "m").is_err(), "missing {index}");
        }
        for ending in [
            json!({"type":"error","message":"private"}),
            json!({"type":"turn.failed","error":{"message":"private"}}),
            values[4].clone(),
            values[1].clone(),
        ] {
            let mut broken = values.clone();
            broken.push(ending);
            assert!(decode(&wire(&broken), "m").is_err());
        }
        for (index, field, value) in [
            (0, "thread_id", json!(null)),
            (0, "model", json!(7)),
            (4, "usage", json!({"input_tokens":-1})),
            (
                4,
                "usage",
                json!({"input_tokens":3,"cached_input_tokens":4}),
            ),
            (4, "usage", json!({"output_tokens":"private"})),
            (4, "usage", json!({"reasoning_output_tokens":-1})),
            (
                4,
                "usage",
                json!({"output_tokens":1,"reasoning_output_tokens":2}),
            ),
        ] {
            let mut broken = values.clone();
            broken[index][field] = value;
            assert!(decode(&wire(&broken), "m").is_err());
        }
        let mut broken = values.clone();
        broken.insert(4, values[3].clone());
        assert!(decode(&wire(&broken), "m").is_err());
        let mut broken = values.clone();
        broken[3]["item"]["status"] = json!("incomplete");
        assert!(decode(&wire(&broken), "m").is_err());
        assert!(
            decode(
                b"{\"type\":\"thread.started\",\"type\":\"thread.started\",\"thread_id\":\"t\"}",
                "m"
            )
            .is_err()
        );
        assert!(decode(&[0xff], "m").is_err());
    }
}
