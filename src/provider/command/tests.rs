use super::*;
use serde_json::{Value, json};

fn request() -> CompletionRequest {
    CompletionRequest {
        role: Role::Sceptic,
        system: "Retain the evidence boundary.".into(),
        user: "A source containing literal $(commands), quotes and newlines.\nNext line.".into(),
        schema: json!({"type":"object","properties":{"answer":{"type":"string"}},"required":["answer"],"additionalProperties":false}),
    }
}

fn config() -> CommandConfig {
    CommandConfig {
        kind: AssistantKind::Command,
        argv: vec!["adapter".into()],
        inherit_env: vec![],
        model: "configured-model".into(),
        role_models: HashMap::new(),
        timeout: Duration::from_secs(5),
        max_input_bytes: 10_000,
        max_output_bytes: 10_000,
        max_output_tokens: 100,
        max_concurrent_requests: 1,
        supervisor: PathBuf::from("supervisor"),
    }
}

#[test]
fn generic_exchange_preserves_the_complete_request_without_shell_interpolation() {
    let request = request();
    let argv = vec!["operator-adapter".into(), "literal $(argument)".into()];
    let invocation = generic::encode(&argv, "role-model", &request, 123).unwrap();
    assert_eq!(invocation.argv, argv);
    let value: Value = serde_json::from_slice(&invocation.input).unwrap();
    assert_eq!(value["schema_version"], "engarg.completion.v1");
    assert_eq!(value["model"], "role-model");
    assert_eq!(value["max_output_tokens"], 123);
    assert_eq!(value["request"], serde_json::to_value(request).unwrap());
    let content = json!({"content":"{\"answer\":\"bounded\"}","model":"actual-model","input_tokens":3,"output_tokens":4,"request_id":"receipt"});
    let completion = generic::decode(&serde_json::to_vec(&content).unwrap()).unwrap();
    assert_eq!(completion.model, "actual-model");
    assert_eq!(completion.input_tokens, Some(3));
    assert_eq!(completion.output_tokens, Some(4));
}

#[test]
fn generic_exchange_rejects_extra_fields_multiple_objects_and_duplicate_keys() {
    for bytes in [
        br#"{"content":"{}","model":"m","input_tokens":null,"output_tokens":null,"request_id":null,"unknown":1}"#.as_slice(),
        br#"{"content":"{}","content":"changed","model":"m","input_tokens":null,"output_tokens":null,"request_id":null}"#.as_slice(),
        br#"{} {}"#.as_slice(),
        b"not JSON".as_slice(),
    ] {
        assert!(generic::decode(bytes).is_err());
    }
}

#[test]
fn native_adapters_use_fixed_noninteractive_flags_and_stdin_context() {
    let request = request();
    let cursor = cursor::encode(&["agent".into()], "cursor-model", &request, 100).unwrap();
    assert_eq!(
        &cursor.argv[1..8],
        [
            "--print",
            "--mode",
            "ask",
            "--output-format",
            "json",
            "--model",
            "cursor-model"
        ]
    );
    assert!(
        !cursor
            .argv
            .iter()
            .any(|arg| arg.contains("literal $(commands)"))
    );
    let augment = augment::encode(&["auggie".into()], "augment-model", &request, 100).unwrap();
    assert_eq!(
        &augment.argv[1..5],
        ["--print", "--quiet", "--model", "augment-model"]
    );
    for invocation in [cursor, augment] {
        let prompt: Value = serde_json::from_slice(&invocation.input).unwrap();
        assert_eq!(prompt["system"], request.system);
        assert_eq!(prompt["user"], request.user);
        assert_eq!(prompt["output_schema"], request.schema);
    }
}

#[test]
fn cursor_requires_a_success_envelope_and_labels_unreported_model_identity() {
    let completion = cursor::decode(br#"{"type":"result","subtype":"success","is_error":false,"result":"{}","request_id":"r1","future_field":1}"#, "configured").unwrap();
    assert_eq!(completion.content, "{}");
    assert_eq!(completion.model, "assistant:cursor:configured");
    assert_eq!(completion.request_id.as_deref(), Some("r1"));
    assert_eq!(completion.input_tokens, None);
    assert_eq!(completion.output_tokens, None);
    for value in [
        json!({"type":"result","subtype":"success","is_error":true,"result":"{}"}),
        json!({"type":"result","subtype":"error","is_error":false,"result":"{}"}),
        json!({"type":"assistant","subtype":"success","is_error":false,"result":"{}"}),
        json!({"type":"result","subtype":"success","is_error":false,"result":42}),
    ] {
        assert!(cursor::decode(&serde_json::to_vec(&value).unwrap(), "m").is_err());
    }
}

#[test]
fn augment_preserves_text_for_downstream_schema_validation_without_inventing_usage() {
    let completion = augment::decode(b"{malformed candidate", "m").unwrap();
    assert_eq!(completion.content, "{malformed candidate");
    assert_eq!(completion.model, "assistant:augment:m");
    assert_eq!(completion.input_tokens, None);
    assert_eq!(completion.output_tokens, None);
    assert!(augment::decode(&[0xff], "m").is_err());
}

#[test]
fn command_admission_rejects_invalid_limits_environment_and_identifiers() {
    assert!(CommandModel::new(config()).is_ok());
    for field in 0..7 {
        let mut value = config();
        match field {
            0 => value.argv.clear(),
            1 => value.argv[0] = "invalid\0command".into(),
            2 => value.model = "private\nvalue".into(),
            3 => value.max_concurrent_requests = 0,
            4 => value.timeout = Duration::ZERO,
            5 => value.inherit_env = vec!["INVALID=ENV".into()],
            _ => value.max_output_bytes = usize::MAX,
        }
        assert!(CommandModel::new(value).is_err());
    }
}

#[tokio::test]
async fn input_bound_rejects_before_starting_any_command() {
    let mut value = config();
    value.max_input_bytes = 1;
    let error = CommandModel::new(value)
        .unwrap()
        .complete(request())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Budget(_)));
}
