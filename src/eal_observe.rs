//! Explicit, bounded adapters for known read-only JSON collectors.
//!
//! These adapters run only through the operator-invoked EAL collection command.
//! Tool selection never supplies an executable or a shell command.

use crate::Error;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

const MAX_BYTES: usize = 1_048_576;
const TIMEOUT: Duration = Duration::from_secs(60);

/// Parse a collector request without silently collapsing duplicate JSON keys.
pub fn parse_request(bytes: &[u8]) -> Result<Value, Error> {
    crate::retained_collectors::parse_json(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionRequest {
    evidence_id: String,
    environment: String,
    tool: String,
    tool_version: String,
    input: Value,
    context: Value,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn arguments(input: &Value) -> Result<BTreeMap<String, String>, Error> {
    serde_json::from_value(input.get("arguments").cloned().unwrap_or(Value::Null))
        .map_err(|_| invalid("collector input.arguments must contain named string values"))
}

fn required<'a>(args: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, Error> {
    args.get(key)
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .map(String::as_str)
        .ok_or_else(|| invalid("collector argument is missing or invalid"))
}

fn exact_keys(args: &BTreeMap<String, String>, expected: &[&str]) -> Result<(), Error> {
    if args.len() != expected.len() || expected.iter().any(|key| !args.contains_key(*key)) {
        return Err(invalid(
            "collector arguments differ from the selected tool contract",
        ));
    }
    Ok(())
}

fn command_args(
    tool: &str,
    args: &BTreeMap<String, String>,
) -> Result<(&'static str, Vec<String>), Error> {
    match tool {
        "kubectl_get" => {
            exact_keys(args, &["context", "namespace", "resource", "name"])?;
            let context = required(args, "context")?;
            let namespace = required(args, "namespace")?;
            let resource = required(args, "resource")?;
            let name = required(args, "name")?;
            if [context, namespace, resource, name]
                .iter()
                .any(|v| v.starts_with('-'))
                || !resource
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || (name != "*"
                    && !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
            {
                return Err(invalid(
                    "kubectl resource and scope arguments must be literal identifiers",
                ));
            }
            let mut argv = vec![
                format!("--context={context}"),
                "get".into(),
                resource.into(),
            ];
            if namespace == "*" {
                argv.push("--all-namespaces".into());
                if name != "*" {
                    // kubectl rejects a positional name across all namespaces.
                    // A field selector retains the requested name in the list query.
                    argv.push(format!("--field-selector=metadata.name={name}"));
                }
            } else {
                if name != "*" {
                    argv.push(name.into());
                }
                argv.push(format!("--namespace={namespace}"));
            }
            argv.extend(["--output=json".into(), "--request-timeout=55s".into()]);
            Ok(("kubectl", argv))
        }
        "gh_api_get" => {
            exact_keys(args, &["endpoint"])?;
            let endpoint = required(args, "endpoint")?;
            if endpoint.starts_with('-')
                || endpoint.starts_with("//")
                || endpoint.contains([':', '\\', '#'])
            {
                return Err(invalid("GitHub API endpoint must be a relative API path"));
            }
            Ok((
                "gh",
                vec![
                    "api".into(),
                    "--method".into(),
                    "GET".into(),
                    endpoint.into(),
                ],
            ))
        }
        _ => Err(invalid("unknown command collector")),
    }
}

async fn command_json(tool: &str, args: &BTreeMap<String, String>) -> Result<Value, Error> {
    let (program, argv) = command_args(tool, args)?;
    let mut child = Command::new(program)
        .args(argv)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| invalid("collector executable could not be started"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| invalid("collector stdout unavailable"))?;
    let result = tokio::time::timeout(TIMEOUT, async {
        let mut bytes = Vec::new();
        (&mut output)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Budget("collector output exceeds 1 MiB".into()));
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(Error::Provider("collector command failed".into()));
        }
        crate::retained_collectors::parse_json(&bytes)
            .map_err(|_| invalid("collector returned invalid JSON"))
    })
    .await;
    match result {
        Ok(Ok(value)) => Ok(value),
        other => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            match other {
                Ok(Err(error)) => Err(error),
                Err(_) => Err(Error::Budget("collector exceeded 60 seconds".into())),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}

async fn http_json(args: &BTreeMap<String, String>) -> Result<Value, Error> {
    exact_keys(args, &["url"])?;
    let url = reqwest::Url::parse(required(args, "url")?)
        .map_err(|_| invalid("collector URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "collector URL requires HTTP(S) without embedded credentials or a fragment",
        ));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| invalid("HTTP collector could not be configured"))?;
    let mut request = client.get(url.clone());
    if let Ok(token) = std::env::var("ENGARG_COLLECTOR_TOKEN") {
        let origin = std::env::var("ENGARG_COLLECTOR_ORIGIN")
            .map_err(|_| invalid("authenticated collection requires ENGARG_COLLECTOR_ORIGIN"))?;
        validate_token_origin(&url, &origin)?;
        if token.trim().is_empty() {
            return Err(invalid("collector token must be nonempty"));
        }
        request = request.bearer_auth(token);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| Error::Provider("HTTP collector request failed".into()))?;
    if !response.status().is_success() {
        return Err(Error::Provider(
            "HTTP collector returned an unsuccessful status".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::Provider("HTTP collector body failed".into()))?
    {
        if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(Error::Budget("collector output exceeds 1 MiB".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    crate::retained_collectors::parse_json(&bytes)
        .map_err(|_| invalid("collector returned invalid JSON"))
}

fn validate_token_origin(url: &reqwest::Url, allowed: &str) -> Result<(), Error> {
    let origin = reqwest::Url::parse(allowed)
        .map_err(|_| invalid("collector credential origin is invalid"))?;
    if url.scheme() != "https"
        || origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.origin() != url.origin()
    {
        return Err(invalid(
            "collector URL differs from its authorised HTTPS credential origin",
        ));
    }
    Ok(())
}

/// Acquire JSON and bind its observation to the exact EAL acquisition identity.
/// `collected` records successful acquisition, while acceptance needs its review gate.
pub async fn observe(tool: &str, value: Value) -> Result<Value, Error> {
    if matches!(tool, "artifact_json" | "scoped_json") {
        let args = arguments(&value["input"])?;
        exact_keys(&args, &["inputs"])?;
        let path = std::path::Path::new(required(&args, "inputs")?);
        return if tool == "artifact_json" {
            crate::retained_collectors::observe(tool, value, path)
        } else {
            crate::scoped_collectors::observe(value, path).await
        };
    }
    let request: CollectionRequest =
        serde_json::from_value(value).map_err(|_| invalid("invalid EAL collection request"))?;
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ]
    .iter()
    .any(|v| v.trim().is_empty())
        || !request.context.is_object()
    {
        return Err(invalid("EAL collection identity and context are required"));
    }
    let args = arguments(&request.input)?;
    let observation = match tool {
        "kubectl_get" | "gh_api_get" => command_json(tool, &args).await?,
        "http_json_get" => http_json(&args).await?,
        _ => {
            return Err(invalid(
                "collector is unavailable and needs a custom adapter",
            ));
        }
    };
    Ok(json!({
        "value": {"collected":true,"observation":observation},
        "context": request.context,
        "request": {"tool":request.tool,"tool_version":request.tool_version,"input":request.input,"context":request.context}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kubectl_uses_literal_read_only_argv() {
        let args = BTreeMap::from([
            ("context".into(), "prod cluster".into()),
            ("namespace".into(), "*".into()),
            ("resource".into(), "applications.argoproj.io".into()),
            ("name".into(), "pilot-sync-operator".into()),
        ]);
        let (program, argv) = command_args("kubectl_get", &args).unwrap();
        assert_eq!(program, "kubectl");
        assert_eq!(
            argv,
            [
                "--context=prod cluster",
                "get",
                "applications.argoproj.io",
                "--all-namespaces",
                "--field-selector=metadata.name=pilot-sync-operator",
                "--output=json",
                "--request-timeout=55s"
            ]
        );
        let mut bad = args;
        bad.insert("name".into(), "--raw=/secrets".into());
        assert!(command_args("kubectl_get", &bad).is_err());
    }

    #[test]
    fn kubectl_preserves_named_and_wildcard_scope_without_selector_injection() {
        for (namespace, name, expected) in [
            ("pilot", "operator", vec!["operator", "--namespace=pilot"]),
            ("pilot", "*", vec!["--namespace=pilot"]),
            ("*", "*", vec!["--all-namespaces"]),
            (
                "*",
                "operator",
                vec![
                    "--all-namespaces",
                    "--field-selector=metadata.name=operator",
                ],
            ),
        ] {
            let args = BTreeMap::from([
                ("context".into(), "pilot".into()),
                ("namespace".into(), namespace.into()),
                ("resource".into(), "pods".into()),
                ("name".into(), name.into()),
            ]);
            let (_, argv) = command_args("kubectl_get", &args).unwrap();
            assert_eq!(&argv[3..argv.len() - 2], expected);
        }
        for name in [
            "operator,status.phase=Running",
            "operator=other",
            "operator other",
        ] {
            let args = BTreeMap::from([
                ("context".into(), "pilot".into()),
                ("namespace".into(), "*".into()),
                ("resource".into(), "pods".into()),
                ("name".into(), name.into()),
            ]);
            assert!(command_args("kubectl_get", &args).is_err());
        }
    }

    #[test]
    fn gh_has_fixed_get_method_and_rejects_extra_flags() {
        let mut args =
            BTreeMap::from([("endpoint".into(), "repos/example/repo/actions/runs".into())]);
        let (_, argv) = command_args("gh_api_get", &args).unwrap();
        assert_eq!(argv[2], "GET");
        args.insert("method".into(), "POST".into());
        assert!(command_args("gh_api_get", &args).is_err());
        for endpoint in [
            "https://elsewhere.example/repo",
            "//elsewhere.example/repo",
            "--input=x",
        ] {
            assert!(
                command_args(
                    "gh_api_get",
                    &BTreeMap::from([("endpoint".into(), endpoint.into())])
                )
                .is_err()
            );
        }
    }

    #[test]
    fn http_credentials_require_an_exact_operator_supplied_https_origin() {
        let url = reqwest::Url::parse("https://tempo.example:8443/api/search?q=test").unwrap();
        assert!(validate_token_origin(&url, "https://tempo.example:8443").is_ok());
        for origin in [
            "https://tempo.example",
            "https://other.example:8443",
            "http://tempo.example:8443",
            "https://tempo.example:8443/path",
            "https://user@tempo.example:8443",
        ] {
            assert!(validate_token_origin(&url, origin).is_err());
        }
    }

    #[tokio::test]
    async fn malformed_or_custom_acquisitions_cannot_report_collection() {
        assert!(observe("custom", json!({})).await.is_err());
        assert!(observe("custom", json!({"evidence_id":"e","environment":"env","tool":"custom","tool_version":"1","input":{"arguments":{}},"context":{}})).await.is_err());
    }
}
