//! Prepare current-case adapter bindings from explicit operation and artifact selections.
//!
//! Preparation checks configuration and retained pins. It performs no source reads
//! against Kubernetes or Grafana and creates no independent review decisions.
use crate::{
    Error,
    acquisition::{AcquisitionPlan, InputPreparation, ScopedOperation, ToolArgument},
    domain::ArgumentCase,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

const MAX_ARTIFACT_BYTES: usize = 1_048_576;

pub struct PreparedInputs {
    pub plans: Vec<AcquisitionPlan>,
    scoped: Option<Value>,
    artifacts: Option<Value>,
    report: Value,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn path_text(path: &Path) -> Result<String, Error> {
    path.to_str()
        .filter(|text| !text.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| invalid("collector preparation paths require UTF-8 without controls"))
}

impl PreparedInputs {
    /// Bind files only after the final plan has determined the export revision.
    pub fn finish(mut self, case_digest: &str) -> Result<BTreeMap<String, Vec<u8>>, Error> {
        if !digest(case_digest) {
            return Err(invalid(
                "collector preparation requires the final export digest",
            ));
        }
        let mut files = BTreeMap::new();
        for (name, value) in [
            ("scoped-inputs.json", self.scoped.as_mut()),
            ("artifact-inputs.json", self.artifacts.as_mut()),
            ("collector-preparation.json", Some(&mut self.report)),
        ] {
            if let Some(value) = value {
                value["case_digest"] = json!(case_digest);
                let mut bytes = serde_json::to_vec_pretty(value)?;
                bytes.push(b'\n');
                files.insert(name.into(), bytes);
            }
        }
        Ok(files)
    }
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| {
        !text.trim().is_empty() && !text.chars().any(char::is_control) && !text.contains("REPLACE")
    })
}

fn literal(value: &Value) -> bool {
    text(value).is_some_and(|text| {
        text.as_bytes()[0].is_ascii_alphanumeric()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    })
}

fn selected_object(input: &Value, name: &str, fields: &[&str]) -> Value {
    let source = input.get(name).and_then(Value::as_object);
    Value::Object(
        fields
            .iter()
            .map(|field| {
                (
                    (*field).into(),
                    source
                        .and_then(|source| source.get(*field))
                        .cloned()
                        .unwrap_or(Value::Null),
                )
            })
            .collect(),
    )
}

fn scope_configuration(inputs: &Value) -> (Value, Value) {
    let mut kube = selected_object(
        inputs,
        "kubernetes",
        &[
            "executable",
            "context",
            "resource",
            "namespace",
            "kind",
            "name",
        ],
    );
    if kube["executable"].is_null() {
        kube["executable"] = json!("kubectl");
    }
    let mut grafana = selected_object(
        inputs,
        "grafana",
        &[
            "origin",
            "datasource_uid",
            "query",
            "rollout_kind",
            "rollout_name",
            "start_unix",
            "end_unix",
            "trace_ids",
            "token_env",
            "tenant_env",
        ],
    );
    if grafana["trace_ids"].is_null() {
        grafana["trace_ids"] = json!([]);
    }
    for (target, source) in [("rollout_kind", "kind"), ("rollout_name", "name")] {
        if grafana[target].is_null() {
            grafana[target] = kube[source]
                .as_str()
                .map_or(Value::Null, |value| json!(value));
        }
    }
    (kube, grafana)
}

fn kube_gaps(kube: &Value) -> Vec<String> {
    let mut gaps = Vec::new();
    let executable = text(&kube["executable"]);
    if !executable
        .is_some_and(|value| Path::new(value).is_absolute() || literal(&kube["executable"]))
    {
        gaps.push(
            "kubernetes.executable requires an explicit executable path or literal command".into(),
        );
    }
    for field in ["context", "kind", "name", "resource"] {
        if !literal(&kube[field]) {
            gaps.push(format!("kubernetes.{field} requires a literal selector"));
        }
    }
    if !literal(&kube["namespace"]) && kube["namespace"] != "*" {
        gaps.push("kubernetes.namespace requires a literal namespace or explicit *".into());
    }
    if !kube["resource"].as_str().is_some_and(|resource| {
        resource
            .split_once('.')
            .is_some_and(|(plural, group)| literal(&json!(plural)) && literal(&json!(group)))
    }) {
        gaps.push("kubernetes.resource requires an explicit resource.API-group selector".into());
    }
    gaps
}

fn grafana_gaps(grafana: &Value, kube: &Value, operation: &ScopedOperation) -> Vec<String> {
    let mut gaps = Vec::new();
    let origin_valid = text(&grafana["origin"])
        .and_then(|origin| reqwest::Url::parse(origin).ok())
        .is_some_and(|url| {
            let loopback = url.scheme() == "http"
                && url
                    .host_str()
                    .and_then(|host| {
                        host.trim_matches(['[', ']'])
                            .parse::<std::net::IpAddr>()
                            .ok()
                    })
                    .is_some_and(|ip| ip.is_loopback());
            (url.scheme() == "https" || loopback)
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
        });
    if !origin_valid {
        gaps.push(
            "grafana.origin requires an approved HTTPS origin without path or credentials".into(),
        );
    }
    if !literal(&grafana["datasource_uid"]) {
        gaps.push("grafana.datasource_uid requires a literal datasource UID".into());
    }
    if !grafana["query"].as_str().is_some_and(|query| {
        !query.trim().is_empty()
            && query.len() <= 65_536
            && !query
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        gaps.push("grafana.query requires the exact nonempty bounded TraceQL expression".into());
    }
    for (field, permitted) in [("token_env", "TEMPO_TOKEN"), ("tenant_env", "TEMPO_TENANT")] {
        if !grafana[field].is_null() && grafana[field] != permitted {
            gaps.push(format!(
                "grafana.{field} is outside the selected adapter's forwarded environment"
            ));
        }
    }
    match operation {
        ScopedOperation::GrafanaSearch => {
            if !grafana["start_unix"]
                .as_u64()
                .zip(grafana["end_unix"].as_u64())
                .is_some_and(|(start, end)| start > 0 && end > start)
            {
                gaps.push(
                    "grafana search requires positive ordered start_unix and end_unix".into(),
                );
            }
        }
        ScopedOperation::GrafanaTrace => {
            for (field, source) in [("rollout_kind", "kind"), ("rollout_name", "name")] {
                if !text(&grafana[field]).is_some_and(|value| !value.starts_with('-')) {
                    gaps.push(format!(
                        "grafana.{field} requires the explicit resource identity"
                    ));
                } else if !kube[source].is_null() && grafana[field] != kube[source] {
                    gaps.push(format!(
                        "grafana.{field} differs from the supplied Kubernetes identity"
                    ));
                }
            }
            if !grafana["trace_ids"].as_array().is_some_and(|ids| {
                !ids.is_empty()
                    && ids.len() <= 32
                    && ids.iter().all(|id| {
                        id.as_str().is_some_and(|id| {
                            matches!(id.len(), 16 | 32)
                                && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                                && !id.bytes().all(|byte| byte == b'0')
                        })
                    })
            }) {
                gaps.push(
                    "grafana.trace_ids requires 1 to 32 observed nonzero hexadecimal trace IDs"
                        .into(),
                );
            }
        }
        ScopedOperation::Kubernetes => unreachable!("Kubernetes uses its own configuration"),
    }
    gaps
}

fn bind_inputs(plan: &mut AcquisitionPlan, path: Option<&str>) {
    plan.arguments.retain(|argument| argument.name != "inputs");
    plan.unresolved_arguments
        .retain(|argument| argument != "inputs");
    if let Some(path) = path {
        plan.arguments.push(ToolArgument {
            name: "inputs".into(),
            value: path.into(),
        });
    } else {
        plan.unresolved_arguments.push("inputs".into());
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactPin {
    path: PathBuf,
    sha256: String,
    observed_at: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactGroup {
    artifacts: Vec<ArtifactPin>,
}

fn prepared_artifacts(inputs: &Value, reference: &str, directory: &Path) -> Result<Value, Error> {
    if !crate::acquisition::valid_artifact_reference(reference) {
        return Err(invalid(
            "artifact reference requires an explicit literal alias",
        ));
    }
    let selected = inputs
        .get("artifacts")
        .and_then(Value::as_object)
        .and_then(|entries| entries.get(reference))
        .ok_or_else(|| {
            invalid("the selected artifact producer reference has no supplied retained records")
        })?;
    let mut group: ArtifactGroup = serde_json::from_value(selected.clone())?;
    let artifacts: Vec<eal_adapters::retained::Artifact> =
        serde_json::from_value(serde_json::to_value(&group.artifacts)?)?;
    eal_adapters::retained::import_artifacts(&artifacts, directory)?;
    for pin in &mut group.artifacts {
        // Admission reads no special files and resolves old relative paths before
        // moving their bindings into the new run directory.
        if pin
            .path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(invalid(
                "retained artifact paths must not traverse parent directories",
            ));
        }
        let selected = if pin.path.is_absolute() {
            pin.path.clone()
        } else {
            directory.join(&pin.path)
        };
        let bytes = crate::bounded_file::read(&selected, MAX_ARTIFACT_BYTES, "retained artifact")?;
        let observed_hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if observed_hash != pin.sha256 {
            return Err(invalid("retained artifact changed during preparation"));
        }
        let canonical = selected.canonicalize()?;
        if pin.path.is_relative() && !canonical.starts_with(directory) {
            return Err(invalid(
                "relative retained artifact escapes the original inputs directory",
            ));
        }
        pin.path = canonical;
    }
    Ok(json!({"artifacts": group.artifacts}))
}

/// Materialise bindings from typed operation choices and operator-owned scope.
/// Old case digests and need mappings supply no authority for the new argument.
pub fn prepare_plans(
    case: &ArgumentCase,
    plans: &[AcquisitionPlan],
    operator_inputs: &Value,
    operator_directory: &Path,
    output_directory: &Path,
) -> Result<PreparedInputs, Error> {
    if !operator_inputs.is_object()
        || !operator_directory.is_absolute()
        || !output_directory.is_absolute()
    {
        return Err(invalid(
            "collector preparation requires an operator object and absolute directories",
        ));
    }
    let directory = operator_directory.canonicalize()?;
    if !directory.is_dir() {
        return Err(invalid(
            "operator inputs require an existing parent directory",
        ));
    }
    let scoped_path = path_text(&output_directory.join("scoped-inputs.json"))?;
    let artifact_path = path_text(&output_directory.join("artifact-inputs.json"))?;
    let (kube, grafana) = scope_configuration(operator_inputs);
    // Both optional configurations are deserialised before selecting an operation.
    // Invalid unused objects must not poison an otherwise valid scoped read.
    let kube_shape_valid =
        serde_json::from_value::<eal_adapters::scoped::Kubernetes>(kube.clone()).is_ok();
    let grafana_shape_valid =
        serde_json::from_value::<eal_adapters::scoped::Grafana>(grafana.clone()).is_ok();
    let mut retained = BTreeMap::new();
    for plan in plans {
        if !case
            .evidence_plan
            .iter()
            .any(|need| need.id == plan.need_id)
            || retained.insert(plan.need_id.as_str(), plan).is_some()
        {
            return Err(invalid(
                "collector preparation repeats a need or names an unknown need",
            ));
        }
    }
    let mut scoped_needs = Map::new();
    let mut artifact_needs = Map::new();
    let mut needs_scoped = false;
    let mut needs_artifacts = false;
    let mut prepared = Vec::new();
    let mut entries = Vec::new();
    for need in &case.evidence_plan {
        let mut plan = retained
            .get(need.id.as_str())
            .map(|plan| (**plan).clone())
            .unwrap_or_else(|| AcquisitionPlan {
                need_id: need.id.clone(),
                tool_id: "custom".into(),
                arguments: Vec::new(),
                unresolved_arguments: vec!["adapter".into()],
                input_preparation: None,
            });
        let mut gaps = Vec::new();
        let mut evidence = "not_collected";
        let mut operation = Value::Null;
        let mut reference = Value::Null;
        match plan.tool_id.as_str() {
            "scoped_json" => {
                needs_scoped = true;
                match &plan.input_preparation {
                    Some(InputPreparation::Scoped {
                        operation: selected,
                    }) => {
                        operation = serde_json::to_value(selected)?;
                        scoped_needs.insert(need.id.clone(), operation.clone());
                        gaps = match selected {
                            ScopedOperation::Kubernetes => kube_gaps(&kube),
                            _ => grafana_gaps(&grafana, &kube, selected),
                        };
                        if matches!(selected, ScopedOperation::Kubernetes) && !kube_shape_valid {
                            gaps.push("Kubernetes configuration differs from the selected adapter's input types".into());
                        } else if !matches!(selected, ScopedOperation::Kubernetes)
                            && !grafana_shape_valid
                        {
                            gaps.push("Grafana configuration differs from the selected adapter's input types".into());
                        }
                    }
                    None => gaps.push(
                        "scoped operation is unselected for this current evidence need".into(),
                    ),
                    _ => return Err(invalid("scoped_json requires scoped input preparation")),
                }
                bind_inputs(&mut plan, gaps.is_empty().then_some(scoped_path.as_str()));
            }
            "artifact_json" => {
                needs_artifacts = true;
                let pins = match &plan.input_preparation {
                    Some(InputPreparation::Artifact {
                        reference: selected,
                    }) => {
                        reference = json!(selected);
                        match prepared_artifacts(operator_inputs, selected, &directory) {
                            Ok(pins) => {
                                evidence = "retained_artifacts_verified";
                                pins
                            }
                            Err(error) => {
                                gaps.push(error.to_string());
                                json!({"artifacts": []})
                            }
                        }
                    }
                    None => {
                        gaps.push("artifact producer unidentified: select a supplied retained-artifact reference or an acquisition producer".into());
                        json!({"artifacts": []})
                    }
                    _ => return Err(invalid("artifact_json requires artifact input preparation")),
                };
                artifact_needs.insert(need.id.clone(), pins);
                bind_inputs(&mut plan, gaps.is_empty().then_some(artifact_path.as_str()));
                if !gaps.is_empty() {
                    evidence = "unavailable";
                }
            }
            "custom" => {
                gaps.push("acquisition implementation is missing".into());
                evidence = "unavailable";
            }
            _ => {}
        }
        for argument in &plan.unresolved_arguments {
            if argument != "inputs" || gaps.is_empty() {
                gaps.push(format!("unresolved collector argument: {argument}"));
            }
        }
        let implementation = if plan.tool_id == "custom" {
            "missing"
        } else if crate::acquisition::required_arguments(&plan.tool_id).is_some() {
            "available"
        } else {
            "catalogue_selected"
        };
        entries.push(json!({
            "need_id":need.id,"tool_id":plan.tool_id,
            "implementation":implementation,
            "configuration":if gaps.is_empty() {"bound"} else {"unresolved"},
            "configuration_scope":"static_argument_bindings_only",
            "evidence":evidence,"operation":operation,"artifact_reference":reference,"gaps":gaps,
        }));
        prepared.push(plan);
    }
    let bound = entries
        .iter()
        .filter(|entry| entry["configuration"] == "bound")
        .count();
    let retained_count = entries
        .iter()
        .filter(|entry| entry["evidence"] == "retained_artifacts_verified")
        .count();
    let ignored_legacy_binding_fields: Vec<_> = ["case_digest", "needs"]
        .into_iter()
        .filter(|field| operator_inputs.get(*field).is_some())
        .collect();
    let report = json!({
        "schema":"engarg.collector-preparation/1","case_digest":Value::Null,
        "inputs_prepared":true,"evidence_collected":false,
        "readiness_scope":"static_configuration_and_retained_pins",
        "runtime_access_and_credentials_checked":false,
        "independent_reviews_created":false,
        "ignored_legacy_binding_fields":ignored_legacy_binding_fields,
        "summary":{"bound":bound,"unresolved":entries.len()-bound,"retained_artifacts_verified":retained_count},
        "entries":entries,
    });
    Ok(PreparedInputs {
        plans: prepared,
        scoped: needs_scoped.then(|| json!({"schema":"engarg.scoped-inputs/1","case_digest":Value::Null,"needs":scoped_needs,"kubernetes":if kube_shape_valid {kube} else {Value::Null},"grafana":if grafana_shape_valid {grafana} else {Value::Null}})),
        artifacts: needs_artifacts.then(|| json!({"schema":"engarg.artifact-inputs/1","case_digest":Value::Null,"needs":artifact_needs})),
        report,
    })
}
