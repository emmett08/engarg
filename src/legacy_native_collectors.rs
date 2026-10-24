//! Frozen v1 projection rules used only to reconstruct historical export bundles.
use crate::{Error, acquisition::AcquisitionPlan};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) struct NativeTool {
    pub id: &'static str,
    pub parameters: &'static [&'static str],
    source_fields: &'static [&'static str],
    predicate: &'static str,
}

pub(crate) const TOOLS: &[NativeTool] = &[
    NativeTool {
        id: "kubernetes",
        parameters: &["inventory", "resource", "kind", "namespace", "name"],
        source_fields: &["resource", "namespace", "name"],
        predicate: "cluster_count >= 1",
    },
    NativeTool {
        id: "github",
        parameters: &["repository", "commit"],
        source_fields: &["repository", "commit"],
        predicate: "found == true",
    },
    NativeTool {
        id: "tempo",
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
        predicate: "cluster_count >= 1",
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
}
