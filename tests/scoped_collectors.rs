//! Offline contracts for explicitly configured single-context source reads.
use engarg::scoped_collectors::observe;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::mpsc,
    time::Duration,
};

const QUERY: &str = "{\n  span.dpe.rollout_kind = \"Application\" &&\n  span.dpe.rollout_name = \"catalog-sync-operator\"\n}";

fn request() -> Value {
    json!({
        "evidence_id":"acquisition_E1","environment":"engarg_case",
        "tool":"acquisition_tool_E1","tool_version":"1",
        "input":{"schema":"engarg.eal-acquisition/1","case_digest":"fixture-case-digest",
            "need":{"id":"E1","comparator":"operator review required"},
            "arguments":{"untrusted_executable":"/must/not/run","untrusted_url":"https://unapproved.example.invalid"}},
        "context":{"engarg_export_digest":"fixture-case-digest","extra":"retain exactly"}
    })
}

fn grafana_inputs(origin: &str, operation: &str) -> Value {
    json!({"schema":"engarg.scoped-inputs/1","case_digest":"fixture-case-digest",
        "needs":{"E1":operation},"kubernetes":null,
        "grafana":{"origin":origin,"datasource_uid":"fixture-tempo","query":QUERY,
            "rollout_kind":"Application","rollout_name":"catalog-sync-operator",
            "start_unix":1000,"end_unix":2000,"trace_ids":["1234567890abcdef1234567890abcdef"],
            "token_env":null,"tenant_env":null}})
}

fn write_inputs(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn serve_once(
    status: &str,
    body: String,
) -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let (sender, receiver) = mpsc::channel();
    let task = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request = read_headers(&mut socket);
        sender.send(request).unwrap();
        write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    (format!("http://{address}"), receiver, task)
}

fn read_headers(socket: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0_u8; 1024];
        let count = socket.read(&mut chunk).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        assert!(bytes.len() < 16384);
    }
    String::from_utf8(bytes).unwrap()
}

fn assert_bound_measurement(observation: &Value, request: &Value) {
    assert_eq!(observation["value"]["collected"], true);
    assert_eq!(observation["value"]["measurement_only"], true);
    assert_eq!(observation["context"], request["context"]);
    assert_eq!(
        observation["request"],
        json!({"tool":request["tool"],"tool_version":request["tool_version"],
        "input":request["input"],"context":request["context"]})
    );
    assert_eq!(
        observation["value"]["details"]["source_inputs_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(
        observation["value"]["details"]["started_at"]["unix_seconds"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        observation["value"]["details"]["finished_at"]["unix_seconds"]
            .as_u64()
            .unwrap()
            > 0
    );
    for key in ["accepted", "proven", "current", "fleet"] {
        assert!(observation["value"].get(key).is_none());
    }
}

#[tokio::test]
async fn search_sends_unchanged_traceql_to_exact_grafana_proxy_and_retains_empty_measurement() {
    let (origin, received, server) = serve_once(
        "200 OK",
        r#"{"traces":[],"metrics":{"inspectedTraces":5}}"#.into(),
    );
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    write_inputs(&path, &grafana_inputs(&origin, "grafana_search"));
    let request = request();
    let measurement = observe(request.clone(), &path).await.unwrap();
    assert_bound_measurement(&measurement, &request);
    let inputs_hash: String = Sha256::digest(std::fs::read(&path).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        measurement["value"]["details"]["source_inputs_sha256"],
        inputs_hash
    );
    assert_eq!(
        measurement["value"]["observation"]["transport"]["method"],
        "GET"
    );
    assert_eq!(
        measurement["value"]["observation"]["transport"]["status"],
        200
    );
    assert_eq!(
        measurement["value"]["observation"]["data"]["traces"],
        json!([])
    );
    let wire = received.recv_timeout(Duration::from_secs(5)).unwrap();
    let request_target = wire
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let url = reqwest::Url::parse(&format!("{origin}{request_target}")).unwrap();
    assert_eq!(
        url.path(),
        "/api/datasources/proxy/uid/fixture-tempo/api/search"
    );
    let parameters: Vec<_> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(
        parameters,
        [
            ("q".into(), QUERY.into()),
            ("start".into(), "1000".into()),
            ("end".into(), "2000".into())
        ]
    );
    assert!(!wire.to_ascii_lowercase().contains("authorization:"));
    server.join().unwrap();
}

#[tokio::test]
async fn full_trace_is_fetched_by_operator_id_through_same_datasource() {
    let data = json!({"batches":[],"fixture":"preserve raw trace"});
    let (origin, received, server) = serve_once("200 OK", data.to_string());
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    write_inputs(&path, &grafana_inputs(&origin, "grafana_trace"));
    let request = request();
    let measurement = observe(request.clone(), &path).await.unwrap();
    assert_bound_measurement(&measurement, &request);
    assert_eq!(
        measurement["value"]["observation"]["traces"][0]["data"],
        data
    );
    assert_eq!(
        measurement["value"]["observation"]["traces"][0]["inspection"]["span_count"],
        0
    );
    assert_eq!(
        measurement["value"]["observation"]["traces"][0]["trace_id_binding"]["all_candidates_match_requested_trace_id"],
        false
    );
    assert!(received.recv_timeout(Duration::from_secs(5)).unwrap().starts_with(
        "GET /api/datasources/proxy/uid/fixture-tempo/api/traces/1234567890abcdef1234567890abcdef HTTP/1.1\r\n"));
    server.join().unwrap();
}

#[tokio::test]
async fn unresolved_selectors_identity_mismatches_and_extra_config_fail_before_network() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    for selector in ["datasource_uid", "start_unix", "end_unix"] {
        let mut inputs = grafana_inputs(&origin, "grafana_search");
        inputs["grafana"][selector] = Value::Null;
        write_inputs(&path, &inputs);
        assert!(observe(request(), &path).await.is_err());
    }
    for modification in ["case", "context", "need", "schema"] {
        write_inputs(&path, &grafana_inputs(&origin, "grafana_search"));
        let mut req = request();
        match modification {
            "case" => req["input"]["case_digest"] = json!("another-case"),
            "context" => req["context"]["engarg_export_digest"] = json!("another-case"),
            "need" => req["input"]["need"]["id"] = json!("E9"),
            _ => req["input"]["schema"] = json!("other-schema"),
        }
        assert!(observe(req, &path).await.is_err());
    }
    let mut inputs = grafana_inputs(&origin, "grafana_search");
    inputs["grafana"]["extra_endpoint"] = json!("/other");
    write_inputs(&path, &inputs);
    assert!(observe(request(), &path).await.is_err());
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[tokio::test]
async fn unapproved_origins_queries_credentials_and_trace_ids_fail_before_network() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    for origin in [
        "http://grafana.example.invalid",
        "https://user@grafana.example.invalid",
        "https://grafana.example.invalid/path",
        "https://grafana.example.invalid?redirect=1",
    ] {
        write_inputs(&path, &grafana_inputs(origin, "grafana_search"));
        assert!(observe(request(), &path).await.is_err());
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for (field, value, operation) in [
        ("query", json!(""), "grafana_search"),
        ("token_env", json!("INVALID-NAME"), "grafana_search"),
        (
            "token_env",
            json!("ENGARG_FIXTURE_INTENTIONALLY_UNSET_TOKEN_9247"),
            "grafana_search",
        ),
        ("trace_ids", json!([]), "grafana_trace"),
        ("trace_ids", json!(["../../secrets"]), "grafana_trace"),
        (
            "trace_ids",
            json!(["00000000000000000000000000000000"]),
            "grafana_trace",
        ),
        (
            "datasource_uid",
            json!("../another-source"),
            "grafana_search",
        ),
    ] {
        let mut inputs = grafana_inputs(&origin, operation);
        inputs["grafana"][field] = value;
        write_inputs(&path, &inputs);
        assert!(observe(request(), &path).await.is_err());
    }
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[tokio::test]
async fn grafana_transport_failures_are_acquisition_errors() {
    for status in [
        "401 Unauthorized",
        "403 Forbidden",
        "500 Internal Server Error",
        "302 Found",
    ] {
        let (origin, received, server) = serve_once(status, "{}".into());
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("inputs.json");
        write_inputs(&path, &grafana_inputs(&origin, "grafana_search"));
        assert!(observe(request(), &path).await.is_err());
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        server.join().unwrap();
    }
}

#[cfg(unix)]
mod kubernetes {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn config(executable: &Path) -> Value {
        json!({"schema":"engarg.scoped-inputs/1","case_digest":"fixture-case-digest",
            "needs":{"E1":"kubernetes"},"grafana":null,
            "kubernetes":{"executable":executable,"context":"dev","resource":"applications.example.test",
                "namespace":"operators","kind":"Application","name":"catalog-sync-operator"}})
    }

    fn mock(path: &Path, output: &str, exit: u8) {
        assert!(!output.contains('\''));
        std::fs::write(path, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.argv\"\nprintf '%s' '{output}'\nexit {exit}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[tokio::test]
    async fn kubernetes_uses_read_only_literal_argv_and_preserves_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("fixture-kubectl");
        mock(
            &executable,
            r#"{"apiVersion":"example.test/v1","kind":"Application","metadata":{"namespace":"operators","name":"catalog-sync-operator","uid":"fixture-uid","resourceVersion":"42"}}"#,
            0,
        );
        let path = temporary.path().join("inputs.json");
        write_inputs(&path, &config(&executable));
        let request = request();
        let measurement = observe(request.clone(), &path).await.unwrap();
        assert_bound_measurement(&measurement, &request);
        assert_eq!(measurement["value"]["observation"]["resource_found"], true);
        assert_eq!(
            measurement["value"]["observation"]["matching_resource_count"],
            1
        );
        assert_eq!(
            std::fs::read_to_string(executable.with_extension("argv")).unwrap(),
            "--context=dev\nget\napplications.example.test\ncatalog-sync-operator\n--namespace=operators\n--output=json\n--request-timeout=55s\n--ignore-not-found=true\n"
        );
    }

    #[tokio::test]
    async fn operator_scope_is_generic_and_response_api_group_must_match() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("fixture-kubectl");
        let path = temporary.path().join("inputs.json");
        let mut inputs = config(&executable);
        inputs["kubernetes"]["context"] = json!("fixture-context");
        inputs["kubernetes"]["resource"] = json!("widgets.fixture.test");
        inputs["kubernetes"]["kind"] = json!("Widget");
        inputs["kubernetes"]["name"] = json!("fixture-widget");
        write_inputs(&path, &inputs);
        for (group, found) in [("fixture.test", true), ("different.test", false)] {
            let data = json!({"apiVersion":format!("{group}/v1"), "kind":"Widget",
                "metadata":{"name":"fixture-widget","namespace":"operators",
                    "uid":"fixture-identity","resourceVersion":"73"}});
            mock(&executable, &data.to_string(), 0);
            let measurement = observe(request(), &path).await.unwrap();
            let observation = &measurement["value"]["observation"];
            assert_eq!(observation["resource_found"], found);
            assert_eq!(
                observation["resource_identities"][0]["uid"],
                "fixture-identity"
            );
            assert_eq!(
                observation["resource_identities"][0]["resource_version"],
                "73"
            );
        }
        assert!(
            std::fs::read_to_string(executable.with_extension("argv"))
                .unwrap()
                .starts_with(
                    "--context=fixture-context\nget\nwidgets.fixture.test\nfixture-widget\n"
                )
        );
    }

    #[tokio::test]
    async fn missing_resource_is_a_negative_measurement_and_command_failure_is_an_error() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("fixture-kubectl");
        let path = temporary.path().join("inputs.json");
        write_inputs(&path, &config(&executable));
        mock(&executable, "", 0);
        let measurement = observe(request(), &path).await.unwrap();
        assert_eq!(measurement["value"]["observation"]["resource_found"], false);
        assert_eq!(
            measurement["value"]["observation"]["matching_resource_count"],
            0
        );
        mock(&executable, "", 1);
        assert!(observe(request(), &path).await.is_err());
    }

    #[tokio::test]
    async fn unresolved_or_wrong_scope_selectors_do_not_start_any_process() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("fixture-kubectl");
        mock(&executable, "{}", 0);
        let path = temporary.path().join("inputs.json");
        for (key, value) in [
            ("resource", Value::Null),
            ("resource", json!("applications")),
            ("namespace", Value::Null),
            ("context", json!("--another-context")),
            ("resource", json!("--raw=/secrets")),
            ("namespace", json!("--all-namespaces")),
            ("kind", json!("--Secret")),
            ("name", json!("--another-object")),
            ("executable", json!("relative/kubectl")),
        ] {
            let mut inputs = config(&executable);
            inputs["kubernetes"][key] = value;
            write_inputs(&path, &inputs);
            assert!(observe(request(), &path).await.is_err());
        }
        assert!(!executable.with_extension("argv").exists());
    }

    #[tokio::test]
    async fn explicit_all_namespaces_uses_read_only_name_field_selector() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("fixture-kubectl");
        mock(&executable, r#"{"kind":"ApplicationList","items":[]}"#, 0);
        let path = temporary.path().join("inputs.json");
        let mut inputs = config(&executable);
        inputs["kubernetes"]["namespace"] = json!("*");
        write_inputs(&path, &inputs);
        let measurement = observe(request(), &path).await.unwrap();
        assert_eq!(measurement["value"]["observation"]["resource_found"], false);
        let argv = std::fs::read_to_string(executable.with_extension("argv")).unwrap();
        assert!(
            argv.contains(
                "--all-namespaces\n--field-selector=metadata.name=catalog-sync-operator\n"
            )
        );
    }
}

#[tokio::test]
async fn configured_query_retains_whitespace_inside_quoted_values() {
    let (origin, received, server) = serve_once("200 OK", r#"{"traces":[]}"#.into());
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    let mut inputs = grafana_inputs(&origin, "grafana_search");
    let query = "{ span.example = \"value with  two spaces\" }\n";
    inputs["grafana"]["query"] = json!(query);
    write_inputs(&path, &inputs);
    let result = observe(request(), &path).await.unwrap();
    assert_eq!(result["value"]["observation"]["query"], query);
    let wire = received.recv_timeout(Duration::from_secs(5)).unwrap();
    let target = wire
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let url = reqwest::Url::parse(&format!("{origin}{target}")).unwrap();
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "q").unwrap().1,
        query
    );
    assert_eq!(
        result["value"]["observation"]["transport"]["url"],
        url.as_str()
    );
    server.join().unwrap();
}

fn trace_data(id: &str) -> Value {
    json!({"resourceSpans":[{"resource":{},"scopeSpans":[{"spans":[{
        "traceId":id,"spanId":"1234567890abcdef","name":"fixture span",
        "startTimeUnixNano":"1000","endTimeUnixNano":"2000",
        "attributes":[
            {"key":"dpe.rollout_kind","value":{"stringValue":"Application"}},
            {"key":"dpe.rollout_name","value":{"stringValue":"catalog-sync-operator"}}
        ]
    }]}]}]})
}

#[tokio::test]
async fn trace_candidates_are_bound_to_requested_trace_identity() {
    for (id, matches) in [
        ("1234567890abcdef1234567890abcdef", 1),
        ("fedcba0987654321fedcba0987654321", 0),
    ] {
        let (origin, received, server) = serve_once("200 OK", trace_data(id).to_string());
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("inputs.json");
        write_inputs(&path, &grafana_inputs(&origin, "grafana_trace"));
        let result = observe(request(), &path).await.unwrap();
        let trace = &result["value"]["observation"]["traces"][0];
        assert_eq!(trace["inspection"]["candidate_matches"], 1);
        assert_eq!(
            trace["trace_id_binding"]["bound_candidate_matches"],
            matches
        );
        assert_eq!(
            trace["trace_id_binding"]["all_candidates_match_requested_trace_id"],
            matches == 1
        );
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        server.join().unwrap();
    }
}

#[tokio::test]
async fn malformed_duplicate_key_and_oversized_grafana_responses_are_rejected() {
    for body in [
        "not JSON".to_owned(),
        r#"{"traces":[],"traces":[{"traceID":"ambiguous"}]}"#.to_owned(),
        format!("{{\"large\":\"{}\"}}", "x".repeat(1_048_576)),
    ] {
        let (origin, received, server) = serve_once("200 OK", body);
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("inputs.json");
        write_inputs(&path, &grafana_inputs(&origin, "grafana_search"));
        assert!(observe(request(), &path).await.is_err());
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        server.join().unwrap();
    }
}

#[tokio::test]
async fn redirect_does_not_contact_its_destination() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let location = format!("http://{}/must-not-read", destination.local_addr().unwrap());
    let redirect = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", redirect.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = redirect.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        read_headers(&mut socket);
        write!(socket,"HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
    });
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("inputs.json");
    write_inputs(&path, &grafana_inputs(&origin, "grafana_search"));
    assert!(observe(request(), &path).await.is_err());
    server.join().unwrap();
    assert!(
        matches!(destination.accept(),Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}
