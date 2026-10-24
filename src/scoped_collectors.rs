//! Operator-configured reads for one explicitly selected Kubernetes context and Grafana source.
//!
//! Inputs are private operator data, never model-supplied execution instructions. These
//! adapters report measurements; independent EAL review decides their adequacy.

use crate::Error;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    net::IpAddr,
    path::Path,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncReadExt, process::Command};

const MAX_BYTES: usize = 1_048_576;
const TIMEOUT: Duration = Duration::from_secs(60);

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    schema: String,
    case_digest: String,
    needs: BTreeMap<String, Operation>,
    kubernetes: Option<Kubernetes>,
    grafana: Option<Grafana>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Kubernetes,
    GrafanaSearch,
    GrafanaTrace,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Kubernetes {
    executable: String,
    context: String,
    resource: Option<String>,
    namespace: Option<String>,
    kind: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grafana {
    origin: String,
    datasource_uid: Option<String>,
    query: String,
    rollout_kind: Option<String>,
    rollout_name: Option<String>,
    start_unix: Option<u64>,
    end_unix: Option<u64>,
    #[serde(default)]
    trace_ids: Vec<String>,
    token_env: Option<String>,
    tenant_env: Option<String>,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn selector<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, Error> {
    value
        .as_deref()
        .filter(|value| nonempty(value) && !value.starts_with('-') && !value.contains("REPLACE"))
        .ok_or_else(|| {
            Error::InvalidInput(format!("scoped collector requires an explicit {field}"))
        })
}

fn literal_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn timestamp() -> Result<Value, Error> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("collector clock is before the Unix epoch"))?;
    Ok(json!({"unix_seconds":time.as_secs(),"subsec_nanos":time.subsec_nanos()}))
}

fn read_inputs(path: &Path) -> Result<(Inputs, String), Error> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::Budget("scoped inputs exceed 1 MiB".into()));
    }
    let inputs: Inputs = serde_json::from_value(crate::retained_collectors::parse_json(&bytes)?)
        .map_err(|_| invalid("invalid engarg.scoped-inputs/1 input file"))?;
    if inputs.schema != "engarg.scoped-inputs/1" || !nonempty(&inputs.case_digest) {
        return Err(invalid(
            "scoped inputs require their schema and exact case digest",
        ));
    }
    Ok((inputs, sha256(&bytes)))
}

fn validate_binding<'a>(request: &'a CollectionRequest, inputs: &Inputs) -> Result<&'a str, Error> {
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ]
    .iter()
    .any(|value| !nonempty(value))
        || request.input.get("schema").and_then(Value::as_str) != Some("engarg.eal-acquisition/1")
        || request.input.get("case_digest").and_then(Value::as_str)
            != Some(inputs.case_digest.as_str())
        || request
            .context
            .get("engarg_export_digest")
            .and_then(Value::as_str)
            != Some(inputs.case_digest.as_str())
    {
        return Err(invalid(
            "scoped collection requires an exact EAL acquisition and case digest binding",
        ));
    }
    request
        .input
        .get("need")
        .and_then(|need| need.get("id"))
        .and_then(Value::as_str)
        .filter(|id| nonempty(id) && inputs.needs.contains_key(*id))
        .ok_or_else(|| invalid("scoped inputs have no operation for this exact evidence need"))
}

fn kubernetes_argv(config: &Kubernetes) -> Result<Vec<String>, Error> {
    let executable = &config.executable;
    if !nonempty(executable)
        || (!Path::new(executable).is_absolute() && !literal_identifier(executable))
        || executable.contains("REPLACE")
        || !literal_identifier(&config.context)
        || !literal_identifier(&config.kind)
        || !literal_identifier(&config.name)
    {
        return Err(invalid(
            "scoped Kubernetes requires an operator executable and literal context, kind and name",
        ));
    }
    let resource = selector(&config.resource, "Kubernetes resource")?;
    let namespace = selector(&config.namespace, "Kubernetes namespace")?;
    if !literal_identifier(resource) || (namespace != "*" && !literal_identifier(namespace)) {
        return Err(invalid(
            "Kubernetes resource and namespace must be literal selectors",
        ));
    }
    if !resource
        .split_once('.')
        .is_some_and(|(plural, group)| literal_identifier(plural) && literal_identifier(group))
    {
        return Err(invalid(
            "Kubernetes custom resource requires an explicit resource.API-group selector",
        ));
    }
    let mut argv = vec![
        format!("--context={}", config.context),
        "get".into(),
        resource.into(),
    ];
    if namespace == "*" {
        argv.extend([
            "--all-namespaces".into(),
            format!("--field-selector=metadata.name={}", config.name),
        ]);
    } else {
        argv.extend([config.name.clone(), format!("--namespace={namespace}")]);
    }
    argv.extend([
        "--output=json".into(),
        "--request-timeout=55s".into(),
        "--ignore-not-found=true".into(),
    ]);
    Ok(argv)
}

async fn kubernetes(config: &Kubernetes) -> Result<Value, Error> {
    let argv = kubernetes_argv(config)?;
    let mut child = Command::new(&config.executable)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Provider("scoped Kubernetes executable could not be started".into()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("collector stdout unavailable"))?;
    let result = tokio::time::timeout(TIMEOUT, async {
        let mut bytes = Vec::new();
        (&mut stdout)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Budget("Kubernetes response exceeds 1 MiB".into()));
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(Error::Provider("scoped Kubernetes read failed".into()));
        }
        let data: Value = if bytes.iter().all(u8::is_ascii_whitespace) {
            Value::Null
        } else {
            crate::retained_collectors::parse_json(&bytes)
                .map_err(|_| Error::Provider("Kubernetes returned invalid or duplicate-key JSON".into()))?
        };
        let resources: Vec<&Value> = match data.get("items").and_then(Value::as_array) {
            Some(items) => items.iter().collect(),
            None if data.is_null() => Vec::new(),
            None => vec![&data],
        };
        let expected_group = config.resource.as_deref().and_then(|resource| resource.split_once('.')).expect("validated resource group").1;
        let identity_matches = |item: &&&Value| {
            let api_group = item.get("apiVersion").and_then(Value::as_str)
                .and_then(|version| version.split_once('/')).map(|(group,_)| group);
            api_group == Some(expected_group)
                && item.get("kind").and_then(Value::as_str) == Some(config.kind.as_str())
                && item.pointer("/metadata/name").and_then(Value::as_str)
                    == Some(config.name.as_str())
                && (config.namespace.as_deref() == Some("*")
                    || item.pointer("/metadata/namespace").and_then(Value::as_str)
                        == config.namespace.as_deref())
        };
        let matching_resource_count = resources.iter().filter(identity_matches).count();
        let identities: Vec<_> = resources.iter().map(|resource| json!({
            "api_version":resource.get("apiVersion"),"kind":resource.get("kind"),
            "name":resource.pointer("/metadata/name"),"namespace":resource.pointer("/metadata/namespace"),
            "uid":resource.pointer("/metadata/uid"),"resource_version":resource.pointer("/metadata/resourceVersion")
        })).collect();
        Ok(json!({
            "operation":"kubernetes", "context":config.context,
            "resource":config.resource,"namespace":config.namespace,
            "kind":config.kind,"name":config.name,"expected_api_group":expected_group,
            "command":{"executable":config.executable,"argv":argv,"exit_code":status.code()},
            "response_sha256":sha256(&bytes),"resource_identities":identities,
            "resource_found":matching_resource_count > 0,
            "returned_resource_count":resources.len(),
            "matching_resource_count":matching_resource_count,
            "data":data
        }))
    })
    .await;
    match result {
        Ok(Ok(value)) => Ok(value),
        outcome => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            match outcome {
                Ok(Err(error)) => Err(error),
                Err(_) => Err(Error::Budget(
                    "scoped Kubernetes exceeded 60 seconds".into(),
                )),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}

fn environment_value(name: &Option<String>) -> Result<Option<String>, Error> {
    let Some(name) = name else {
        return Ok(None);
    };
    if name.is_empty()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        || name.as_bytes()[0].is_ascii_digit()
    {
        return Err(invalid(
            "Grafana credential selectors must be environment variable names",
        ));
    }
    let value = std::env::var(name).map_err(|_| {
        invalid("configured Grafana credential environment variable is unavailable")
    })?;
    if !nonempty(&value) {
        return Err(invalid(
            "configured Grafana credential environment variable is empty or invalid",
        ));
    }
    Ok(Some(value))
}

struct GrafanaRequest {
    origin: reqwest::Url,
    uid: String,
    token: Option<String>,
    tenant: Option<String>,
}

fn grafana_request(config: &Grafana, operation: Operation) -> Result<GrafanaRequest, Error> {
    let uid = selector(&config.datasource_uid, "Grafana datasource UID")?;
    if !literal_identifier(uid) {
        return Err(invalid(
            "Grafana datasource UID must be a literal identifier",
        ));
    }
    if config.query.trim().is_empty()
        || config.query.len() > 65_536
        || config
            .query
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(invalid(
            "Grafana requires a nonempty bounded literal TraceQL query",
        ));
    }
    let origin = reqwest::Url::parse(&config.origin)
        .map_err(|_| invalid("Grafana requires an explicit approved origin"))?;
    let loopback_fixture = origin.scheme() == "http"
        && origin
            .host_str()
            .and_then(|host| {
                host.trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<IpAddr>()
                    .ok()
            })
            .is_some_and(|address| address.is_loopback());
    if (origin.scheme() != "https" && !loopback_fixture)
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || config.origin.contains("REPLACE")
    {
        return Err(invalid(
            "Grafana requires an HTTPS origin without a path, credentials, query or fragment; HTTP is reserved for loopback fixtures",
        ));
    }
    match operation {
        Operation::GrafanaSearch => match (config.start_unix, config.end_unix) {
            (Some(start), Some(end)) if start > 0 && end > start => {}
            _ => {
                return Err(invalid(
                    "Grafana search requires explicit positive ordered Unix-second bounds",
                ));
            }
        },
        Operation::GrafanaTrace => {
            selector(&config.rollout_kind, "trace rollout kind")?;
            selector(&config.rollout_name, "trace rollout name")?;
            if config.trace_ids.is_empty()
                || config.trace_ids.len() > 32
                || config.trace_ids.iter().any(|id| {
                    !matches!(id.len(), 16 | 32)
                        || !id.bytes().all(|b| b.is_ascii_hexdigit())
                        || id.bytes().all(|b| b == b'0')
                })
            {
                return Err(invalid(
                    "Grafana trace read requires 1 to 32 explicit nonzero hexadecimal trace IDs",
                ));
            }
        }
        Operation::Kubernetes => return Err(invalid("invalid Grafana operation")),
    }
    let token = environment_value(&config.token_env)?;
    let tenant = environment_value(&config.tenant_env)?;
    if tenant
        .as_ref()
        .is_some_and(|value| reqwest::header::HeaderValue::from_str(value).is_err())
        || token
            .as_ref()
            .is_some_and(|value| reqwest::header::HeaderValue::from_str(value).is_err())
    {
        return Err(invalid(
            "Grafana credential environment value is not a valid HTTP header",
        ));
    }
    Ok(GrafanaRequest {
        origin,
        uid: uid.into(),
        token,
        tenant,
    })
}

async fn get_json(
    client: &reqwest::Client,
    url: reqwest::Url,
    source: &GrafanaRequest,
    remaining: &mut usize,
) -> Result<(Value, Value), Error> {
    let started_at = timestamp()?;
    let actual_url = url.as_str().to_owned();
    let mut request = client.get(url);
    if let Some(token) = &source.token {
        request = request.bearer_auth(token);
    }
    if let Some(tenant) = &source.tenant {
        request = request.header("X-Scope-OrgID", tenant);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| Error::Provider("Grafana read failed".into()))?;
    if !response.status().is_success() {
        return Err(Error::Provider(format!(
            "Grafana read returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::Provider("Grafana response read failed".into()))?
    {
        if chunk.len() > *remaining {
            return Err(Error::Budget("Grafana responses exceed 1 MiB".into()));
        }
        *remaining -= chunk.len();
        bytes.extend_from_slice(&chunk);
    }
    let data = crate::retained_collectors::parse_json(&bytes)
        .map_err(|_| Error::Provider("Grafana returned invalid or duplicate-key JSON".into()))?;
    Ok((
        data,
        json!({"url":actual_url,"method":"GET","status":status,
        "started_at":started_at,"finished_at":timestamp()?,"response_sha256":sha256(&bytes)}),
    ))
}

async fn grafana(config: &Grafana, operation: Operation) -> Result<Value, Error> {
    let source = grafana_request(config, operation)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| invalid("Grafana HTTP collector could not be configured"))?;
    tokio::time::timeout(TIMEOUT, async {
        let prefix = format!("/api/datasources/proxy/uid/{}/api", source.uid);
        let mut remaining = MAX_BYTES;
        match operation {
            Operation::GrafanaSearch => {
                let mut url = source.origin.clone();
                url.set_path(&format!("{prefix}/search"));
                url.query_pairs_mut()
                    .append_pair("q", &config.query)
                    .append_pair(
                        "start",
                        &config.start_unix.expect("validated start").to_string(),
                    )
                    .append_pair("end", &config.end_unix.expect("validated end").to_string());
                let (data, transport) = get_json(&client, url, &source, &mut remaining).await?;
                Ok(json!({"operation":"grafana_search","origin":config.origin,
                    "datasource_uid":source.uid,"query":config.query,"start_unix":config.start_unix,
                    "end_unix":config.end_unix,"data":data,"transport":transport}))
            }
            Operation::GrafanaTrace => {
                let mut traces = Vec::new();
                for id in &config.trace_ids {
                    let mut url = source.origin.clone();
                    url.set_path(&format!("{prefix}/traces/{id}"));
                    let (data, transport) = get_json(&client, url, &source, &mut remaining).await?;
                    let inspection = crate::trace_inspection::inspect_trace(&data,
                        config.rollout_kind.as_deref().expect("validated rollout kind"),
                        config.rollout_name.as_deref().expect("validated rollout name"))?;
                    let candidate_ids: Vec<_> = inspection.get("matching_spans").and_then(Value::as_array)
                        .into_iter().flatten().filter_map(|span| span.get("trace_id").and_then(Value::as_str)).collect();
                    let bound_candidate_matches = candidate_ids.iter().filter(|candidate| candidate.eq_ignore_ascii_case(id)).count();
                    traces.push(json!({"trace_id":id,"data":data,"transport":transport,"inspection":inspection,
                        "trace_id_binding":{"requested_trace_id":id,"candidate_trace_ids":candidate_ids,
                            "bound_candidate_matches":bound_candidate_matches,
                            "all_candidates_match_requested_trace_id":!candidate_ids.is_empty() && bound_candidate_matches == candidate_ids.len()}}));
                }
                Ok(json!({"operation":"grafana_trace","origin":config.origin,
                    "datasource_uid":source.uid,"query":config.query,
                    "rollout_kind":config.rollout_kind,"rollout_name":config.rollout_name,"traces":traces}))
            }
            Operation::Kubernetes => unreachable!(),
        }
    })
    .await
    .map_err(|_| Error::Budget("Grafana operation exceeded 60 seconds".into()))?
}

/// Read configured sources and bind the measurement to the unchanged EAL request.
/// Unknown operations, unresolved selectors and identity mismatches fail before I/O.
pub async fn observe(request: Value, inputs_path: &Path) -> Result<Value, Error> {
    let request: CollectionRequest =
        serde_json::from_value(request).map_err(|_| invalid("invalid EAL collection request"))?;
    let (inputs, inputs_sha256) = read_inputs(inputs_path)?;
    let need = validate_binding(&request, &inputs)?;
    let started_at = timestamp()?;
    let observation = match inputs.needs[need] {
        Operation::Kubernetes => {
            kubernetes(
                inputs
                    .kubernetes
                    .as_ref()
                    .ok_or_else(|| invalid("scoped inputs require Kubernetes configuration"))?,
            )
            .await?
        }
        operation @ (Operation::GrafanaSearch | Operation::GrafanaTrace) => {
            grafana(
                inputs
                    .grafana
                    .as_ref()
                    .ok_or_else(|| invalid("scoped inputs require Grafana configuration"))?,
                operation,
            )
            .await?
        }
    };
    Ok(json!({
        "value":{"collected":true,"measurement_only":true,"observation":observation,
            "details":{"source_inputs_sha256":inputs_sha256,"started_at":started_at,"finished_at":timestamp()?}},
        "context":request.context,
        "request":{"tool":request.tool,"tool_version":request.tool_version,"input":request.input,"context":request.context}
    }))
}
