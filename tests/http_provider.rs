use engarg::{
    Error,
    provider::{ApiStyle, CompletionRequest, HttpConfig, HttpModel, LanguageModel, Role},
};
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

fn server(status: &str, body: String, headers: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    );
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut received = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..n]);
            if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if received.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8(received).unwrap()
    });
    (format!("http://{address}/v1/responses"), handle)
}

fn model(endpoint: String, max_body_bytes: usize) -> HttpModel {
    HttpModel::new(HttpConfig {
        endpoint,
        api_key: Some("test-key".into()),
        model: "test-model".into(),
        role_models: HashMap::from([(Role::Sceptic, "other-model".into())]),
        service_tier: None,
        style: ApiStyle::Responses,
        timeout: Duration::from_secs(5),
        max_output_tokens: 128,
        max_body_bytes,
    })
    .unwrap()
}

fn request() -> CompletionRequest {
    CompletionRequest {
        role: Role::Sceptic,
        system: "fixed instructions".into(),
        user: "untrusted evidence".into(),
        schema: json!({"type":"object","properties":{},"required":[],"additionalProperties":false}),
    }
}

#[tokio::test]
async fn real_http_adapter_sends_schema_role_model_and_private_storage_setting() {
    let body=json!({"id":"run-1","model":"returned-model","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]}]}).to_string();
    let (endpoint, handle) = server("200 OK", body, "Content-Type: application/json\r\n");
    let response = model(endpoint, 8192).complete(request()).await.unwrap();
    let received = handle.join().unwrap();
    let (_, body) = received.split_once("\r\n\r\n").unwrap();
    let data: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(data["model"], "other-model");
    assert!(data.get("service_tier").is_none());
    assert_eq!(data["store"], false);
    assert_eq!(data["text"]["format"]["strict"], true);
    assert_eq!(response.model, "returned-model");
    assert_eq!(response.service_tier, None);
}

#[tokio::test]
async fn both_api_styles_preserve_the_exact_requested_model_and_tier_and_record_actual_tier() {
    for style in [ApiStyle::Responses, ApiStyle::Chat] {
        let body = match style {
            ApiStyle::Responses => json!({
                "id":"run-tier", "model":"gpt-6.1-sol", "service_tier":"default", "status":"completed",
                "output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]}]
            }),
            ApiStyle::Chat => json!({
                "id":"run-tier", "model":"gpt-6.1-sol", "service_tier":"default",
                "choices":[{"finish_reason":"stop","message":{"content":"{}"}}]
            }),
        };
        let (endpoint, handle) = server(
            "200 OK",
            body.to_string(),
            "Content-Type: application/json\r\n",
        );
        let model = HttpModel::new(HttpConfig {
            endpoint,
            api_key: Some("test-key".into()),
            model: "gpt-6.1-sol".into(),
            role_models: HashMap::new(),
            service_tier: Some("ultrafast".into()),
            style,
            timeout: Duration::from_secs(5),
            max_output_tokens: 128,
            max_body_bytes: 8192,
        })
        .unwrap();
        let completion = model.complete(request()).await.unwrap();
        let received = handle.join().unwrap();
        let (_, body) = received.split_once("\r\n\r\n").unwrap();
        let data: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(data["model"], "gpt-6.1-sol");
        assert_eq!(data["service_tier"], "ultrafast");
        assert_eq!(completion.model, "gpt-6.1-sol");
        assert_eq!(completion.service_tier.as_deref(), Some("default"));
    }
}

#[tokio::test]
async fn rate_limits_do_not_retry_or_echo_remote_error_details() {
    let (endpoint, handle) = server(
        "429 Too Many Requests",
        "sensitive echoed content".into(),
        "",
    );
    let error = model(endpoint, 8192).complete(request()).await.unwrap_err();
    handle.join().unwrap();
    assert!(error.to_string().contains("429"));
    assert!(!error.to_string().contains("sensitive"));
}

#[tokio::test]
async fn redirects_are_not_followed_with_credentials() {
    let (endpoint, handle) = server(
        "302 Found",
        String::new(),
        "Location: http://127.0.0.1:1/steal\r\n",
    );
    let error = model(endpoint, 8192).complete(request()).await.unwrap_err();
    handle.join().unwrap();
    assert!(error.to_string().contains("302"));
}

#[tokio::test]
async fn rejects_oversized_http_envelopes_before_json_decoding() {
    let (endpoint, handle) = server("200 OK", "x".repeat(2048), "");
    let error = model(endpoint, 512).complete(request()).await.unwrap_err();
    handle.join().unwrap();
    assert!(matches!(error, Error::Budget(_)));
}
