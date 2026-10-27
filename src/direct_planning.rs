//! Match authored operation names to an explicit manifest without discovery IO.
use crate::{
    Error, domain::ArgumentCase, engine::RouteResolver, support_requirements::SupportElaboration,
};
use eal_collector_contracts::RegistryDocument;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub struct DirectPlanner {
    versions: BTreeMap<String, String>,
    manifest_sha256: String,
    operations: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectPlan {
    pub schema: String,
    pub manifest_sha256: String,
    pub bindings: BTreeMap<String, String>,
    pub unbound_operations: Vec<String>,
}

impl DirectPlan {
    /// Preserve each unresolved operation explicitly until operator TOML is supplied.
    pub fn unbound(elaboration: &SupportElaboration) -> Self {
        Self {
            schema: "engarg.direct-bindings/1".into(),
            manifest_sha256: crate::authoring_checkpoint::digest(b"[tools]\n"),
            bindings: BTreeMap::new(),
            unbound_operations: elaboration
                .acquisitions
                .iter()
                .map(|a| a.id.clone())
                .collect(),
        }
    }
}

impl Default for DirectPlanner {
    fn default() -> Self {
        Self {
            versions: BTreeMap::new(),
            manifest_sha256: crate::authoring_checkpoint::digest(b"[tools]\n"),
            operations: Vec::new(),
        }
    }
}

impl DirectPlanner {
    pub fn from_tools(path: &Path) -> Result<Self, Error> {
        let bytes = crate::bounded_file::read(path, 1_048_576, "direct tools manifest")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::InvalidInput("tools manifest must be UTF-8".into()))?;
        let document = RegistryDocument::from_toml(text).map_err(Error::InvalidInput)?;
        document.validate().map_err(Error::InvalidInput)?;
        let root = std::path::absolute(path)?
            .parent()
            .ok_or_else(|| Error::InvalidInput("tool manifest has no parent".into()))?
            .canonicalize()?;
        let mut operations = Vec::new();
        let mut schema_bytes = 0usize;
        for (name, binding) in &document.tools {
            let mut operation = json!({"operation":name,"version":binding.version});
            if let Some(settings) = &binding.direct {
                for (field, pin) in [
                    ("input_schema", &settings.input_schema),
                    ("output_schema", &settings.output_schema),
                ] {
                    if let Some(pin) = pin {
                        let schema = pinned_schema(&root, pin, &mut schema_bytes)?;
                        let secrets = binding.env.values().map(String::as_str).collect::<Vec<_>>();
                        if eal_collector_contracts::contains_sensitive_json(
                            &schema,
                            &secrets,
                            Default::default(),
                        )
                        .map_err(|_| {
                            Error::InvalidInput(
                                "operation schema could not be checked for configured secrets"
                                    .into(),
                            )
                        })? {
                            return Err(Error::InvalidInput("operation schema contains a configured secret and cannot enter authoring context".into()));
                        }
                        operation[field] = schema;
                    }
                }
            }
            operations.push(operation);
        }
        Ok(Self {
            operations,
            versions: document
                .tools
                .into_iter()
                .map(|(name, binding)| (name, binding.version))
                .collect(),
            manifest_sha256: crate::authoring_checkpoint::digest(bytes),
        })
    }
}

fn pinned_schema(
    root: &Path,
    pin: &eal_collector_contracts::PinnedFile,
    total: &mut usize,
) -> Result<Value, Error> {
    let path = Path::new(&pin.path);
    let selected = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !path.is_absolute()
        && !selected
            .parent()
            .ok_or_else(|| Error::InvalidInput("schema has no parent".into()))?
            .canonicalize()?
            .starts_with(root)
    {
        return Err(Error::InvalidInput(
            "relative operation schema escapes manifest directory".into(),
        ));
    }
    let bytes = crate::bounded_file::read(&selected, 1_048_576, "operation schema")?;
    *total = total.saturating_add(bytes.len());
    if *total > 1_048_576 {
        return Err(Error::Budget(
            "direct authoring schemas exceed the aggregate 1 MiB limit".into(),
        ));
    }
    if crate::authoring_checkpoint::digest(&bytes) != pin.sha256 {
        return Err(Error::InvalidInput(
            "operation schema differs from its pinned identity".into(),
        ));
    }
    let schema = eal_collector_contracts::strict_json_exact(
        &bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    // Retrieval support is disabled in this dependency; schemas cannot fetch
    // remote references or include local files while constructing a prompt.
    jsonschema::validator_for(&schema).map_err(|_| {
        Error::InvalidInput(
            "operation schema is invalid or has an unavailable external reference".into(),
        )
    })?;
    Ok(schema)
}

impl DirectPlanner {
    /// Resolve the complete authored operation set without discovery or async IO.
    pub fn resolve_operations(
        &self,
        case: &ArgumentCase,
        elaboration: &SupportElaboration,
    ) -> Result<DirectPlan, Error> {
        let errors = crate::support_requirements::validate_elaboration(case, elaboration);
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
        let mut bindings = BTreeMap::new();
        let mut unbound_operations = Vec::new();
        for acquisition in &elaboration.acquisitions {
            let emitted = crate::support_export::operation_tool_name(&acquisition.id);
            if self.versions.contains_key(&acquisition.id) && self.versions.contains_key(&emitted) {
                return Err(Error::InvalidInput(format!(
                    "Duplicate tool binding for {}",
                    acquisition.id
                )));
            }
            let name = if self.versions.contains_key(&acquisition.id) {
                Some(&acquisition.id)
            } else if self.versions.contains_key(&emitted) {
                Some(&emitted)
            } else {
                None
            };
            if let Some(name) = name {
                bindings.insert(acquisition.id.clone(), name.clone());
            } else {
                unbound_operations.push(acquisition.id.clone());
            }
        }
        Ok(DirectPlan {
            schema: "engarg.direct-bindings/1".into(),
            manifest_sha256: self.manifest_sha256.clone(),
            bindings,
            unbound_operations,
        })
    }
}

impl RouteResolver for DirectPlanner {
    type Plan = DirectPlan;
    fn authoring_context(&self) -> Option<Value> {
        Some(json!({"operations":self.operations}))
    }
    async fn resolve(
        &self,
        case: &ArgumentCase,
        elaboration: &SupportElaboration,
    ) -> Result<DirectPlan, Error> {
        self.resolve_operations(case, elaboration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(root: &Path, schema: &[u8]) -> std::path::PathBuf {
        std::fs::write(root.join("input.json"), schema).unwrap();
        let path = root.join("tools.toml");
        std::fs::write(
            &path,
            format!(
                r#"[tools.observe]
kind = "mcp_stdio"
version = "1"
argv = ["PRIVATE_EXECUTABLE"]
env = {{ PRIVATE_TOKEN = "PRIVATE_TOKEN_VALUE" }}
[tools.observe.direct]
tool = "observe"
[tools.observe.direct.input_schema]
path = "input.json"
sha256 = "{}"
"#,
                crate::authoring_checkpoint::digest(schema)
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn authored_interfaces_retain_descriptions_without_transport_configuration() {
        let root = tempfile::tempdir().unwrap();
        let schema = br#"{"type":"object","description":"Observe one declared subject.","properties":{"subject":{"type":"string"}},"required":["subject"],"additionalProperties":false}"#;
        let planner = DirectPlanner::from_tools(&manifest(root.path(), schema)).unwrap();
        let context = planner.authoring_context().unwrap();
        assert_eq!(
            context["operations"][0]["input_schema"]["description"],
            "Observe one declared subject."
        );
        let encoded = context.to_string();
        assert!(!encoded.contains("PRIVATE_"));
        assert!(!encoded.contains("input.json"));
    }

    #[test]
    fn schema_drift_and_secret_bearing_interfaces_fail_before_authoring() {
        let root = tempfile::tempdir().unwrap();
        let path = manifest(root.path(), br#"{"type":"object"}"#);
        std::fs::write(root.path().join("input.json"), br#"{"type":"string"}"#).unwrap();
        assert!(
            DirectPlanner::from_tools(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("pinned identity")
        );
        let path = manifest(
            root.path(),
            br#"{"type":"object","description":"PRIVATE_TOKEN_VALUE"}"#,
        );
        assert!(
            DirectPlanner::from_tools(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("configured secret")
        );
    }
}
