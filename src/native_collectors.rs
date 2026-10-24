//! Project native eal-collector contracts into bounded source inputs and command registries.
use crate::{Error, acquisition::AcquisitionPlan, eal_export::quoted};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) struct NativeTool {
    pub id: &'static str,
    pub purpose: &'static str,
    pub parameters: &'static [&'static str],
    source_fields: &'static [&'static str],
    selectors: &'static [&'static str],
    predicate: &'static str,
    environment: &'static [&'static str],
    executable: Option<&'static str>,
}

pub(crate) const TOOLS: &[NativeTool] = &[
    NativeTool {
        id: "kubernetes",
        purpose: "Native eal-collector Kubernetes fleet resource read through kubectl using an operator-approved inventory. Presence differs from controller health and inventory completeness needs review.",
        parameters: &["inventory", "resource", "kind", "namespace", "name"],
        source_fields: &["resource", "namespace", "name"],
        selectors: &["inventory", "resource", "kind", "namespace", "name"],
        predicate: "cluster_count >= 1",
        environment: &["PATH", "HOME", "KUBECONFIG"],
        executable: Some("KUBECTL"),
    },
    NativeTool {
        id: "github",
        purpose: "Native eal-collector exact GitHub commit and tree lookup through gh. Requires an exact 40-character commit SHA. Deployment correspondence needs separate support.",
        parameters: &["repository", "commit"],
        source_fields: &["repository", "commit"],
        selectors: &["repository", "commit"],
        predicate: "found == true",
        environment: &["PATH", "HOME", "GH_TOKEN", "GH_HOST"],
        executable: Some("GH"),
    },
    NativeTool {
        id: "tempo",
        purpose: "Native eal-collector Tempo HTTP search for rollout kind/name in each inventory cluster within Unix-second bounds. Adds a cluster filter. Returns trace summaries. Empty results leave trace absence unresolved. Exact unchanged queries use http_json_get and full span checks need a trace-by-ID read.",
        parameters: &[
            "inventory",
            "url",
            "kind",
            "name",
            "cluster_attribute",
            "start_unix",
            "end_unix",
        ],
        source_fields: &["start_unix", "end_unix"],
        selectors: &["inventory", "url", "kind", "name", "cluster_attribute"],
        predicate: "cluster_count >= 1",
        environment: &["TEMPO_TOKEN", "TEMPO_TENANT"],
        executable: None,
    },
];

pub(crate) fn spec(id: &str) -> Option<&'static NativeTool> {
    TOOLS.iter().find(|tool| tool.id == id)
}

impl NativeTool {
    pub fn input(&self, plan: &AcquisitionPlan) -> Result<Value, Error> {
        let args: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.as_str(), arg.value.as_str()))
            .collect();
        let mut input = json!({});
        for field in self.source_fields {
            input[*field] = match args.get(field) {
                Some(value) if field.ends_with("_unix") => {
                    json!(value.parse::<u64>().map_err(|_| Error::InvalidInput(
                        "native collector Unix times require unsigned integer strings".into()
                    ))?)
                }
                Some(value) => json!(value),
                None => Value::Null,
            };
        }
        let start = input.get("start_unix").and_then(Value::as_u64);
        let end = input.get("end_unix").and_then(Value::as_u64);
        if start == Some(0)
            || end == Some(0)
            || matches!((start,end), (Some(start),Some(end)) if end <= start || end-start > 86400)
        {
            return Err(Error::InvalidInput("native Tempo window requires positive ordered Unix seconds spanning at most 86400 seconds".into()));
        }
        Ok(input)
    }

    pub fn predicate(&self) -> &'static str {
        self.predicate
    }

    pub fn registry(&self, plan: &AcquisitionPlan) -> String {
        let args: BTreeMap<_, _> = plan
            .arguments
            .iter()
            .map(|arg| (arg.name.as_str(), arg.value.as_str()))
            .collect();
        let mut argv = vec!["eal-collector".to_owned(), self.id.into()];
        if let Some(executable) = self.executable {
            argv.extend([
                "--executable".into(),
                format!("/REPLACE_WITH_ABSOLUTE_{executable}"),
                "--supervisor".into(),
                "/REPLACE_WITH_ABSOLUTE_EAL_COMMAND_SUPERVISOR".into(),
            ]);
        }
        for key in self.selectors {
            let value = args.get(key).map(|v| (*v).to_owned()).unwrap_or_else(|| {
                let placeholder = format!("REPLACE_{}", key.to_uppercase());
                if *key == "inventory" {
                    format!("/{placeholder}.json")
                } else {
                    placeholder
                }
            });
            argv.extend([format!("--{}", key.replace('_', "-")), value]);
        }
        if self.executable.is_some() {
            for name in self.environment {
                argv.extend(["--inherit-env".into(), (*name).into()]);
            }
        }
        argv.extend([
            "--timeout-seconds".into(),
            "60".into(),
            "--max-bytes".into(),
            "1048576".into(),
        ]);
        format!(
            "kind = \"command\"\nversion = \"1\"\nargv = {}\ntimeout_seconds = 65\nmax_output_bytes = 4194304\ninherit_env = {}\nparallel_safe = false\n# Replace operator paths and REPLACE selectors before collection.\n# Resolve null EAL input values and keep them equal to the fixed selectors.\n# Tempo authentication is opt-in after approving the exact endpoint.\n# For Tempo append --token-env TEMPO_TOKEN and --tenant-env TEMPO_TENANT as needed.\n# Pin executable and inventory hashes through pinned_files after configuration.\n# Native values retain measured negatives and still require independent review.\n\n",
            quoted_array(&argv),
            quoted_array(self.environment)
        )
    }
}
fn quoted_array(values: &[impl AsRef<str>]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| quoted(value.as_ref()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
