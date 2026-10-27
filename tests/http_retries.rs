//! Exercise bounded HTTP recovery through a local provider with recorded requests.
#[path = "common/argument_assertions.rs"]
mod argument_assertions;

use engarg::{
    Error,
    provider::{
        ApiStyle, CompletionRequest, HttpConfig, HttpEvent, HttpModel, HttpObserver, LanguageModel,
        Role, TransportConfig,
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

#[path = "common/live_output.rs"]
mod live_output;
#[path = "common/pending_support.rs"]
mod pending_support;

#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
    headers: String,
    delay: Duration,
    body_delay: Duration,
}
impl Reply {
    fn failure(status: u16, headers: &str, body: &str) -> Self {
        Self {
            status,
            body: body.into(),
            headers: headers.into(),
            delay: Duration::ZERO,
            body_delay: Duration::ZERO,
        }
    }
    fn rate(headers: &str, body: Value) -> Self {
        Self::failure(429, headers, &body.to_string())
    }
    fn success() -> Self {
        Self{status:200,body:json!({"status":"completed","id":"response-id","model":"actual-model","output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]}]}).to_string(),headers:String::new(),delay:Duration::ZERO,body_delay:Duration::ZERO}
    }
}
#[derive(Default)]
struct State {
    requests: Mutex<Vec<Value>>,
    active: AtomicUsize,
    peak: AtomicUsize,
}
struct Server {
    endpoint: String,
    state: Arc<State>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let state = Arc::new(State::default());
        let shared = Arc::clone(&state);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut workers = Vec::new();
            let mut index = 0;
            while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let reply = replies
                            .get(index)
                            .unwrap_or_else(|| replies.last().unwrap())
                            .clone();
                        index += 1;
                        let state = Arc::clone(&shared);
                        workers.push(thread::spawn(move||{
                            let request=receive(&mut socket);
                            state.requests.lock().unwrap().push(request);
                            let active=state.active.fetch_add(1,Ordering::SeqCst)+1;
                            state.peak.fetch_max(active,Ordering::SeqCst);
                            thread::sleep(reply.delay);
                            let response=format!("HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",reply.status,reply.body.len(),reply.headers);
                            // Cancellation may close the peer after its request was recorded.
                            let _=socket.write_all(response.as_bytes());
                            thread::sleep(reply.body_delay);
                            let _=socket.write_all(reply.body.as_bytes());
                            state.active.fetch_sub(1,Ordering::SeqCst);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("provider accept failed: {error}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            endpoint,
            state,
            stop,
            thread: Some(handle),
        }
    }
    fn count(&self) -> usize {
        self.state.requests.lock().unwrap().len()
    }
    async fn wait_for_requests(&self, n: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.count() < n {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn receive(socket: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let n = socket.read(&mut buffer).unwrap();
        assert!(n > 0, "truncated request");
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() <= 1_048_576, "oversized test request");
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]);
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}
fn config(endpoint: &str, timeout: Duration) -> HttpConfig {
    HttpConfig {
        endpoint: endpoint.into(),
        api_key: Some("test-private-key".into()),
        model: "default-model".into(),
        role_models: HashMap::from([(Role::Sceptic, "role-model".into())]),
        service_tier: Some("ultrafast".into()),
        reasoning_effort: Some("high".into()),
        style: ApiStyle::Responses,
        timeout,
        max_output_tokens: 128,
        max_body_bytes: 8192,
    }
}
fn request() -> CompletionRequest {
    CompletionRequest {
        role: Role::Sceptic,
        system: "fixed instructions".into(),
        user: "sensitive test input".into(),
        schema: json!({"type":"object","properties":{},"required":[],"additionalProperties":false}),
    }
}
fn recorder() -> (HttpObserver, Arc<Mutex<Vec<HttpEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retained = Arc::clone(&events);
    (
        Arc::new(move |event| {
            retained.lock().unwrap().push(event.clone());
            Ok(())
        }),
        events,
    )
}
fn temporary() -> Value {
    json!({"error":{"type":"rate_limit_error","code":"rate_limit_exceeded","message":"sensitive remote echo"}})
}
fn model(server: &Server, policy: TransportConfig, observer: HttpObserver) -> HttpModel {
    HttpModel::new_with_transport(
        config(&server.endpoint, Duration::from_secs(5)),
        policy,
        observer,
    )
    .unwrap()
}

#[tokio::test]
async fn a_rejected_attempt_replays_identical_configuration_and_records_safe_receipts() {
    for status in [429, 500, 502, 503, 504] {
        let server = Server::new(vec![
            // Proxies can return HTML or an empty body rather than provider JSON.
            Reply::failure(status, "Retry-After: 0\r\n", "sensitive remote echo"),
            Reply::success(),
        ]);
        let (observer, events) = recorder();
        let model = model(&server, TransportConfig::default(), observer);
        let result = model.complete(request()).await.unwrap();
        assert_eq!(result.model, "actual-model");
        let requests = server.state.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0], requests[1]);
        assert_eq!(requests[0]["model"], "role-model");
        assert_eq!(requests[0]["service_tier"], "ultrafast");
        assert_eq!(requests[0]["reasoning"], json!({"effort":"high"}));
        assert_eq!(requests[0]["store"], false);
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .map(|e| (e.kind, e.attempt, e.status))
                .collect::<Vec<_>>(),
            [
                ("response", 1, status),
                ("retry", 1, status),
                ("response", 2, 200)
            ]
        );
        assert!(events[1].wait_ms.is_some_and(|n| (10..=250).contains(&n)));
        let json = serde_json::to_string(&*events).unwrap();
        assert!(!json.contains("sensitive"));
        assert!(!json.contains("test-private-key"));
    }
}

#[tokio::test]
async fn transient_statuses_share_one_retry_allowance() {
    let server = Server::new(vec![
        Reply::rate("Retry-After: 0\r\n", temporary()),
        Reply::failure(503, "Retry-After: 0\r\n", ""),
        Reply::failure(502, "Retry-After: 0\r\n", "sensitive gateway details"),
        Reply::success(),
    ]);
    let (observer, events) = recorder();
    let policy = TransportConfig {
        max_http_retries: 2,
        ..TransportConfig::default()
    };
    let failure = model(&server, policy, observer)
        .complete(request())
        .await
        .unwrap_err();
    assert!(failure.to_string().contains("502"));
    assert!(failure.to_string().contains("retry_budget_exhausted"));
    assert!(!failure.to_string().contains("sensitive"));
    assert_eq!(server.count(), 3);
    let events = events.lock().unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == "retry").count(), 2);
    assert_eq!(events.last().unwrap().kind, "retry_budget_exhausted");
}

#[tokio::test]
async fn temporary_server_failures_preserve_explicit_single_attempt_transport() {
    for status in [500, 502, 503, 504] {
        let server = Server::new(vec![Reply::failure(
            status,
            "Retry-After: 0\r\n",
            "sensitive remote echo",
        )]);
        let (observer, events) = recorder();
        let policy = TransportConfig {
            max_http_retries: 0,
            ..TransportConfig::default()
        };
        let failure = model(&server, policy, observer)
            .complete(request())
            .await
            .unwrap_err();
        assert!(failure.to_string().contains("no automatic retry"));
        assert!(!failure.to_string().contains("sensitive"));
        assert_eq!(server.count(), 1);
        assert!(!events.lock().unwrap().iter().any(|e| e.kind == "retry"));
    }
}

#[tokio::test]
async fn oversized_server_error_bodies_stop_before_retry() {
    let server = Server::new(vec![Reply::failure(
        503,
        "Retry-After: 0\r\n",
        &"sensitive diagnostic".repeat(1024),
    )]);
    let (observer, events) = recorder();
    let failure = model(&server, TransportConfig::default(), observer)
        .complete(request())
        .await
        .unwrap_err();
    assert!(matches!(failure, Error::Budget(_)));
    assert!(!failure.to_string().contains("sensitive"));
    assert_eq!(server.count(), 1);
    assert!(!events.lock().unwrap().iter().any(|e| e.kind == "retry"));
}

#[tokio::test]
async fn quota_and_spend_codes_stop_even_when_a_retry_hint_is_present() {
    for error in [
        json!({"code":"insufficient_quota"}),
        json!({"code":"credit_balance_exhausted"}),
        json!({"code":"organization_spend_limit_exceeded"}),
        json!({"code":"project_spend_limit_exceeded"}),
        json!({"code":"organization_usage_limit_exceeded"}),
        json!({"type":"insufficient_quota","code":"rate_limit_exceeded"}),
    ] {
        for status in [429, 503] {
            let server = Server::new(vec![Reply::failure(
                status,
                "Retry-After: 0\r\n",
                &json!({"error":error}).to_string(),
            )]);
            let (observer, events) = recorder();
            let failure = model(&server, TransportConfig::default(), observer)
                .complete(request())
                .await
                .unwrap_err();
            assert!(failure.to_string().contains(&status.to_string()));
            assert_eq!(server.count(), 1);
            let events = events.lock().unwrap();
            assert!(!events.iter().any(|e| e.kind == "retry"));
            assert!(events.last().unwrap().provider_code.is_some());
        }
    }
}

#[tokio::test]
async fn retry_hints_above_the_cap_or_remaining_deadline_stop_without_an_early_request() {
    let future = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(3600));
    for (hint, cap, timeout) in [
        ("2".to_owned(), 1, Duration::from_secs(5)),
        (future, 1, Duration::from_secs(5)),
        ("1".to_owned(), 120, Duration::from_millis(100)),
    ] {
        for status in [429, 503] {
            let server = Server::new(vec![Reply::failure(
                status,
                &format!("Retry-After: {hint}\r\n"),
                &temporary().to_string(),
            )]);
            let (observer, events) = recorder();
            let policy = TransportConfig {
                retry_max_delay_secs: cap,
                ..TransportConfig::default()
            };
            let model =
                HttpModel::new_with_transport(config(&server.endpoint, timeout), policy, observer)
                    .unwrap();
            assert!(model.complete(request()).await.is_err());
            assert_eq!(server.count(), 1);
            assert!(!events.lock().unwrap().iter().any(|e| e.kind == "retry"));
        }
    }
}

#[tokio::test]
async fn a_server_minimum_delay_is_observed_before_recovery() {
    let server = Server::new(vec![
        Reply::rate("Retry-After: 1\r\n", temporary()),
        Reply::success(),
    ]);
    let (observer, events) = recorder();
    let started = Instant::now();
    model(&server, TransportConfig::default(), observer)
        .complete(request())
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_eq!(server.count(), 2);
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.kind == "retry")
            .unwrap()
            .wait_ms
            .is_some_and(|ms| (1000..=1250).contains(&ms))
    );
}

#[tokio::test]
async fn retry_exhaustion_has_a_finite_attempt_count_and_redacts_error_text() {
    let server = Server::new(vec![Reply::rate("Retry-After: 0\r\n", temporary())]);
    let (observer, events) = recorder();
    let policy = TransportConfig {
        max_http_retries: 2,
        ..TransportConfig::default()
    };
    let failure = model(&server, policy, observer)
        .complete(request())
        .await
        .unwrap_err();
    assert!(failure.to_string().contains("429"));
    assert!(!failure.to_string().contains("sensitive"));
    assert_eq!(server.count(), 3);
    let events = events.lock().unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == "response").count(), 3);
    assert_eq!(events.iter().filter(|e| e.kind == "retry").count(), 2);
}

#[tokio::test]
async fn failed_audit_persistence_stops_before_another_http_attempt() {
    for failing_kind in ["response", "retry"] {
        let server = Server::new(vec![Reply::failure(
            503,
            "Retry-After: 0\r\n",
            &temporary().to_string(),
        )]);
        let observer: HttpObserver = Arc::new(move |event| {
            if event.kind == failing_kind {
                Err(Error::Provider("audit persistence failed".into()))
            } else {
                Ok(())
            }
        });
        let error = model(&server, TransportConfig::default(), observer)
            .complete(request())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("audit persistence failed"));
        assert_eq!(server.count(), 1);
    }
}

#[tokio::test]
async fn other_http_failures_and_invalid_successes_keep_single_attempt_semantics() {
    for (status, body) in [
        (400, "sensitive remote echo"),
        (401, "sensitive remote echo"),
        (501, "sensitive remote echo"),
        (505, "sensitive remote echo"),
        (200, "not JSON"),
    ] {
        let server = Server::new(vec![Reply {
            status,
            body: body.into(),
            headers: "Retry-After: 0\r\n".into(),
            delay: Duration::ZERO,
            body_delay: Duration::ZERO,
        }]);
        let (observer, events) = recorder();
        let error = model(&server, TransportConfig::default(), observer)
            .complete(request())
            .await
            .unwrap_err();
        assert_eq!(server.count(), 1);
        assert!(!error.to_string().contains("sensitive"));
        assert!(!events.lock().unwrap().iter().any(|e| e.kind == "retry"));
    }
}

#[tokio::test]
async fn configured_admission_serialises_calls_without_changing_their_results() {
    let mut success = Reply::success();
    success.delay = Duration::from_millis(40);
    let server = Server::new(vec![success]);
    let (observer, events) = recorder();
    let model = model(
        &server,
        TransportConfig {
            max_concurrent_requests: 1,
            ..TransportConfig::default()
        },
        observer,
    );
    let (a, b, c) = tokio::join!(
        model.complete(request()),
        model.complete(request()),
        model.complete(request())
    );
    for result in [a, b, c] {
        assert_eq!(result.unwrap().content, "{}");
    }
    assert_eq!(server.count(), 3);
    assert_eq!(server.state.peak.load(Ordering::SeqCst), 1);
    assert_eq!(events.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn the_logical_deadline_includes_time_waiting_for_admission() {
    let mut success = Reply::success();
    success.delay = Duration::from_millis(350);
    let server = Server::new(vec![success]);
    let (observer, _) = recorder();
    let model = Arc::new(
        HttpModel::new_with_transport(
            config(&server.endpoint, Duration::from_millis(600)),
            TransportConfig {
                max_concurrent_requests: 1,
                ..TransportConfig::default()
            },
            observer,
        )
        .unwrap(),
    );
    let first = Arc::clone(&model);
    let first = tokio::spawn(async move { first.complete(request()).await });
    server.wait_for_requests(1).await;
    let second = model.complete(request()).await;
    assert!(first.await.unwrap().is_ok());
    assert!(second.unwrap_err().to_string().contains("timed out"));
    assert_eq!(server.count(), 2);
}

#[tokio::test]
async fn cancellation_during_retry_sleep_releases_admission_without_resending() {
    let server = Server::new(vec![
        Reply::failure(503, "Retry-After: 1\r\n", ""),
        Reply::success(),
    ]);
    let (notify, ready) = tokio::sync::oneshot::channel();
    let notify = Mutex::new(Some(notify));
    let observer: HttpObserver = Arc::new(move |event| {
        if event.kind == "retry"
            && let Some(notify) = notify.lock().unwrap().take()
        {
            let _ = notify.send(());
        }
        Ok(())
    });
    let model = Arc::new(model(
        &server,
        TransportConfig {
            max_concurrent_requests: 1,
            ..TransportConfig::default()
        },
        observer,
    ));
    let first = Arc::clone(&model);
    let first = tokio::spawn(async move { first.complete(request()).await });
    ready.await.unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(server.count(), 1);
    assert_eq!(model.complete(request()).await.unwrap().content, "{}");
    assert_eq!(server.count(), 2);
}

#[tokio::test]
async fn cancelling_a_queued_call_does_not_send_it_or_leak_a_permit() {
    let mut success = Reply::success();
    success.delay = Duration::from_millis(250);
    let server = Server::new(vec![success, Reply::success()]);
    let (observer, _) = recorder();
    let model = Arc::new(model(
        &server,
        TransportConfig {
            max_concurrent_requests: 1,
            ..TransportConfig::default()
        },
        observer,
    ));
    let first = Arc::clone(&model);
    let first = tokio::spawn(async move { first.complete(request()).await });
    server.wait_for_requests(1).await;
    let queued = Arc::clone(&model);
    let queued = tokio::spawn(async move { queued.complete(request()).await });
    tokio::task::yield_now().await;
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    assert!(first.await.unwrap().is_ok());
    assert_eq!(model.complete(request()).await.unwrap().content, "{}");
    assert_eq!(server.count(), 2);
}

#[tokio::test]
async fn cli_recovers_with_one_logical_call_and_audits_both_http_attempts() {
    let request: Value =
        serde_json::from_str(include_str!("../examples/claim-only-request.json")).unwrap();
    let argument: Value = live_output::case(include_str!("../examples/claim-only-case.json"));
    let elaboration = pending_support::pending(&serde_json::from_value(argument.clone()).unwrap());
    let content = json!({"argument":argument,"support_elaboration_json":serde_json::to_string(&elaboration).unwrap()}).to_string();
    let mut success = Reply::success();
    let mut envelope: Value = serde_json::from_str(&success.body).unwrap();
    envelope["output"][0]["content"][0]["text"] = json!(content);
    success.body = envelope.to_string();
    let server = Server::new(vec![
        Reply::failure(503, "Retry-After: 0\r\n", &json!({"error":{"type":"service_unavailable_error","code":"server_is_overloaded","message":"sensitive remote echo"}}).to_string()),
        success,
    ]);
    let directory = tempfile::tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let report = directory.path().join("report.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args([
            "analyse",
            "--mode",
            "single",
            "--max-calls",
            "1",
            "--http-retries",
            "1",
            "--retry-max-delay-secs",
            "1",
            "--max-concurrent-requests",
            "1",
            "--request-timeout-secs",
            "5",
            "--timeout-secs",
            "10",
            "--model",
            "test-model",
            "--api-key-env",
            "ENGARG_HTTP_RETRY_TEST_UNUSED_KEY",
            "--endpoint",
        ])
        .arg(&server.endpoint)
        .arg("--claim")
        .arg(request["claim"].as_str().unwrap())
        .args([
            "--format",
            "json",
            "--service-tier",
            "ultrafast",
            "--reasoning-effort",
            "high",
        ])
        .arg("--audit")
        .arg(&audit)
        .arg("--output")
        .arg(&report)
        .env_remove("ENGARG_HTTP_RETRY_TEST_UNUSED_KEY")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    argument_assertions::assert_authored_argument(&saved, &argument);
    assert_eq!(saved["transcript"].as_array().unwrap().len(), 1);
    let audit = std::fs::read_to_string(audit).unwrap();
    assert!(!audit.contains("sensitive remote echo"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("sensitive remote echo"));
    let events = audit
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events[0]["transport_config"],
        json!({"max_http_retries":1,"retry_max_delay_secs":1,"max_concurrent_requests":1})
    );
    assert_eq!(
        events
            .iter()
            .filter(
                |event| event["event"] == "http_attempt" && event["record"]["kind"] == "response"
            )
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "http_attempt" && event["record"]["kind"] == "retry")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "completion")
            .count(),
        1
    );
    assert_eq!(events.last().unwrap()["event"], "success");
    let retry = events
        .iter()
        .find(|event| event["record"]["kind"] == "retry")
        .unwrap();
    assert_eq!(retry["record"]["status"], 503);
    assert_eq!(retry["record"]["provider_code"], "server_is_overloaded");
    let requests = server.state.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[0]["model"], "test-model");
    assert_eq!(requests[0]["service_tier"], "ultrafast");
    assert_eq!(requests[0]["reasoning"], json!({"effort":"high"}));
}

#[tokio::test]
async fn an_unrepresentable_call_deadline_fails_before_transport() {
    let server = Server::new(vec![Reply::success()]);
    let (observer, _) = recorder();
    let model = HttpModel::new_with_transport(
        config(&server.endpoint, Duration::MAX),
        TransportConfig::default(),
        observer,
    )
    .unwrap();
    assert!(model.complete(request()).await.is_err());
    assert_eq!(server.state.requests.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn timeout_diagnostics_retain_the_deadline_and_stage_without_resending() {
    for (body_delay, expected_kind, expected_status) in [
        (false, "request_timeout", 0),
        (true, "response_body_timeout", 200),
    ] {
        let mut reply = Reply::success();
        if body_delay {
            reply.body_delay = Duration::from_millis(350);
        } else {
            reply.delay = Duration::from_millis(350);
        }
        let server = Server::new(vec![reply]);
        let (observer, events) = recorder();
        let model = HttpModel::new_with_transport(
            config(&server.endpoint, Duration::from_millis(150)),
            TransportConfig::default(),
            observer,
        )
        .unwrap();
        let failure = model.complete(request()).await.unwrap_err().to_string();
        assert!(
            failure.contains("logical-call deadline 0.150s"),
            "{failure}"
        );
        assert!(failure.contains("no automatic retry"), "{failure}");
        assert!(failure.contains("--request-timeout-secs"), "{failure}");
        assert!(failure.contains("--timeout-secs"), "{failure}");
        assert!(!failure.contains("sensitive"));
        assert!(!failure.contains("test-private-key"));
        assert_eq!(server.count(), 1);
        let events = events.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.kind, expected_kind);
        assert_eq!(event.attempt, 1);
        assert_eq!(event.status, expected_status);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == expected_kind)
                .count(),
            1
        );
        assert!(!events.iter().any(|event| event.kind == "retry"));
    }
}

#[tokio::test]
async fn cli_timeout_retains_failure_audit_and_checkpoint_without_a_report() {
    let mut reply = Reply::success();
    reply.delay = Duration::from_millis(1500);
    let server = Server::new(vec![reply]);
    let directory = tempfile::tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let checkpoint = directory.path().join("checkpoint.json");
    let report = directory.path().join("report.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--mode", "single", "--max-calls", "1",
            "--request-timeout-secs", "1", "--timeout-secs", "5",
            "--model", "test-model", "--endpoint"])
        .arg(&server.endpoint)
        .args(["--claim", "Parallelising independent CI test groups reduces pipeline duration without changing test outcomes.",
            "--api-key-env", "ENGARG_HTTP_RETRY_TEST_UNUSED_KEY", "--format", "json"])
        .arg("--audit").arg(&audit)
        .arg("--checkpoint").arg(&checkpoint)
        .arg("--output").arg(&report)
        .env_remove("ENGARG_HTTP_RETRY_TEST_UNUSED_KEY")
        .output().unwrap();
    assert!(!output.status.success());
    assert!(!report.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("logical-call deadline 1.000s"), "{stderr}");
    assert!(stderr.contains("no automatic retry"), "{stderr}");
    assert_eq!(server.count(), 1);
    let saved: Value = serde_json::from_slice(&std::fs::read(checkpoint).unwrap()).unwrap();
    assert_eq!(saved["transcript"], json!([]));
    assert!(saved["failure"].as_str().unwrap().contains("timed out"));
    let events = std::fs::read_to_string(audit)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| event["event"] == "http_attempt"
            && event["record"]["kind"] == "request_timeout")
    );
    assert_eq!(events.last().unwrap()["event"], "failure");
    assert!(!events.iter().any(|event| event["event"] == "completion"));
}

#[tokio::test]
async fn cli_incomplete_synthesis_retains_specialists_and_rejection_without_repairing() {
    let mut specialist = Reply::success();
    let mut envelope: Value = serde_json::from_str(&specialist.body).unwrap();
    envelope["output"][0]["content"][0]["text"] = json!(
        json!({"findings":["An explicit scoped evaluation is required."],"objections":[],"evidence_gaps":[]}).to_string()
    );
    specialist.body = envelope.to_string();
    let failed = json!({
        "id":"resp_failed_synthesis", "status":"incomplete",
        "incomplete_details":{"reason":"max_output_tokens"},
        "usage":{"input_tokens":200,"output_tokens":10000,"output_tokens_details":{"reasoning_tokens":9000}},
        "output":[{"type":"message","content":[{"type":"output_text","text":"private partial output"}]}]
    });
    let server = Server::new(vec![
        specialist.clone(),
        specialist.clone(),
        specialist,
        Reply::failure(200, "", &failed.to_string()),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let checkpoint = directory.path().join("checkpoint.json");
    let report = directory.path().join("report.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_engarg"))
        .args(["analyse", "--mode", "hybrid", "--max-calls", "7", "--max-repairs", "1",
            "--request-timeout-secs", "5", "--timeout-secs", "20",
            "--model", "test-model", "--endpoint"])
        .arg(&server.endpoint)
        .args(["--claim", "Parallelising independent CI test groups reduces pipeline duration without changing test outcomes.",
            "--api-key-env", "ENGARG_HTTP_RETRY_TEST_UNUSED_KEY", "--format", "json"])
        .arg("--audit").arg(&audit)
        .arg("--checkpoint").arg(&checkpoint)
        .arg("--output").arg(&report)
        .env_remove("ENGARG_HTTP_RETRY_TEST_UNUSED_KEY")
        .output().unwrap();
    assert!(!output.status.success());
    assert!(!report.exists());
    assert_eq!(
        server.count(),
        4,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("synthesiser: incomplete structured response (max_output_tokens"),
        "{stderr}"
    );
    assert!(!stderr.contains("private partial output"));
    let saved: Value = serde_json::from_slice(&std::fs::read(checkpoint).unwrap()).unwrap();
    assert_eq!(saved["transcript"].as_array().unwrap().len(), 3);
    assert!(
        saved["failure"]
            .as_str()
            .unwrap()
            .contains("max_output_tokens")
    );
    let audit = std::fs::read_to_string(audit).unwrap();
    assert!(!audit.contains("private partial output"));
    let events = audit
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "completion")
            .count(),
        3
    );
    let rejected = events
        .iter()
        .filter(|event| event["record"]["kind"] == "response_rejected")
        .collect::<Vec<_>>();
    assert_eq!(rejected.len(), 1);
    let record = &rejected[0]["record"];
    assert_eq!(record["role"], "synthesiser");
    assert_eq!(record["attempt"], 1);
    assert_eq!(record["status"], 200);
    assert_eq!(record["response_failure"]["reason"], "max_output_tokens");
    assert_eq!(record["response_failure"]["reasoning_tokens"], 9000);
    assert_eq!(
        record["response_failure"]["request_id"],
        "resp_failed_synthesis"
    );
    assert_eq!(events.last().unwrap()["event"], "failure");
}

#[tokio::test]
async fn response_rejection_keeps_the_successful_http_attempt_after_a_retry() {
    let server = Server::new(vec![
        Reply::failure(503, "Retry-After: 0\r\n", "{}"),
        Reply::failure(
            200,
            "",
            &json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}})
                .to_string(),
        ),
    ]);
    let (observer, events) = recorder();
    let failure = model(&server, TransportConfig::default(), observer)
        .complete(request())
        .await
        .unwrap_err();
    assert!(failure.to_string().contains("max_output_tokens"));
    assert_eq!(server.count(), 2);
    let events = events.lock().unwrap();
    let rejected = events.last().unwrap();
    assert_eq!(rejected.kind, "response_rejected");
    assert_eq!(rejected.attempt, 2);
    assert_eq!(rejected.status, 200);
    assert_eq!(
        events.iter().filter(|event| event.kind == "retry").count(),
        1
    );
}
