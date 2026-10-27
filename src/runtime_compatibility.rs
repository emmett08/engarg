//! Admit a selected execution target before authoring against its contracts.
use crate::{
    Error,
    eal_host::{HostConfig, invoke},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const FEATURES: [&str; 5] = [
    "evidence_bindings",
    "pure_derivations",
    "direct_tool_bindings",
    "execute",
    "interface_preparation",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// Portable source is authored against the exact linked EAL contracts.
    EmbeddedContract,
    /// An operator-selected installed host answered bounded discovery.
    SelectedHost,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRuntime {
    pub schema: String,
    pub kind: TargetKind,
    pub language: String,
    pub features: Vec<String>,
    pub method_registry_fingerprint: String,
    pub method_contracts_sha256: String,
    pub implementation_version: Option<String>,
    pub executable_sha256: Option<String>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("EAL_TARGET_INCOMPATIBLE: {}", message.into()))
}

fn digest(value: &Value) -> Result<String, Error> {
    eal_collector_contracts::execution::canonical_digest(value).map_err(Error::InvalidInput)
}

/// A retained target receipt records past admission; it grants no current IO authority.
pub fn validate_receipt(receipt: &TargetRuntime) -> Result<(), Error> {
    let current = embedded()?;
    let features: std::collections::BTreeSet<_> =
        receipt.features.iter().map(String::as_str).collect();
    if receipt.schema != current.schema
        || receipt.language != current.language
        || receipt.method_contracts_sha256 != current.method_contracts_sha256
        || receipt.features.len() != features.len()
        || features != FEATURES.into_iter().collect()
        || !eal_collector_contracts::execution::valid_digest(&receipt.method_registry_fingerprint)
    {
        return Err(invalid(
            "retained target receipt differs from the current language or method contracts",
        ));
    }
    match receipt.kind {
        TargetKind::EmbeddedContract => {
            if receipt.method_registry_fingerprint != current.method_registry_fingerprint
                || receipt.executable_sha256.is_some()
                || receipt.implementation_version.is_some()
            {
                return Err(invalid(
                    "embedded contract receipt cannot assert an observed executable identity",
                ));
            }
        }
        TargetKind::SelectedHost => {
            if !receipt
                .executable_sha256
                .as_deref()
                .is_some_and(eal_collector_contracts::execution::valid_digest)
                || !receipt
                    .implementation_version
                    .as_deref()
                    .is_some_and(|value| {
                        !value.trim().is_empty()
                            && value.len() <= 128
                            && !value.chars().any(char::is_control)
                    })
            {
                return Err(invalid(
                    "selected host receipt requires its observed executable and implementation identities",
                ));
            }
        }
    }
    Ok(())
}

pub fn embedded() -> Result<TargetRuntime, Error> {
    let registry = eal_core::methods::default_registry();
    Ok(TargetRuntime {
        schema: "engarg.runtime-target/1".into(),
        kind: TargetKind::EmbeddedContract,
        language: eal_core::LANGUAGE.into(),
        features: FEATURES.into_iter().map(str::to_owned).collect(),
        method_registry_fingerprint: registry.fingerprint(),
        method_contracts_sha256: digest(&registry.describe())?,
        implementation_version: None,
        executable_sha256: None,
    })
}

/// Compare each required descriptor, allowing additional unrelated installed methods.
/// A transport's advertised version alone cannot establish compatible semantics.
pub fn admit_description(description: &Value) -> Result<TargetRuntime, Error> {
    if description["source_syntax"] != eal_core::LANGUAGE {
        return Err(invalid(format!(
            "target must implement {}; migrate or install the pinned runtime before authoring",
            eal_core::LANGUAGE
        )));
    }
    let features = description["execution_features"]
        .as_array()
        .ok_or_else(|| invalid("target omitted executable language capabilities"))?;
    for required in FEATURES {
        if !features
            .iter()
            .any(|feature| feature.as_str() == Some(required))
        {
            return Err(invalid(format!(
                "target lacks required capability {required}"
            )));
        }
    }
    let actual = description
        .pointer("/typed_bindings/methods")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("target omitted typed method descriptors"))?;
    let expected = eal_core::methods::default_registry().describe();
    for (name, descriptor) in expected.as_object().expect("method descriptors") {
        if actual.get(name) != Some(descriptor) {
            return Err(invalid(format!(
                "target method {name} differs from the authoring contract"
            )));
        }
    }
    let fingerprint = description["method_registry_fingerprint"]
        .as_str()
        .filter(|value| eal_collector_contracts::execution::valid_digest(value))
        .ok_or_else(|| invalid("target omitted its method registry identity"))?;
    let mut result = embedded()?;
    result.kind = TargetKind::SelectedHost;
    result.method_registry_fingerprint = fingerprint.into();
    result.implementation_version = Some(
        description["implementation_version"]
            .as_str()
            .filter(|value| {
                !value.trim().is_empty()
                    && value.len() <= 128
                    && !value.chars().any(char::is_control)
            })
            .ok_or_else(|| invalid("target omitted a bounded implementation version"))?
            .to_owned(),
    );
    Ok(result)
}

/// Pure authoring selects the embedded contract. An explicit target must answer
/// discovery before a model is constructed or a paid authoring call can begin.
pub async fn preflight(executable: Option<&Path>) -> Result<TargetRuntime, Error> {
    let Some(executable) = executable else {
        return embedded();
    };
    let executable = resolve_executable(executable)?;
    let identity = crate::bounded_file::sha256(&executable, 256 * 1_048_576, "selected EAL host")?;
    let workspace = tempfile::Builder::new()
        .prefix("engarg-runtime-preflight-")
        .tempdir()?;
    let config = HostConfig {
        executable: executable.clone(),
        workspace: workspace.path().to_owned(),
        database: None,
        registry: None,
        collector_env: vec![],
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: Duration::from_secs(30),
    };
    let response = invoke(&config, &json!({"operation":"describe"})).await?;
    if response["is_error"] != false {
        return Err(invalid("selected host rejected capability discovery"));
    }
    let mut target = admit_description(&response["result"])?;
    let after = crate::bounded_file::sha256(&executable, 256 * 1_048_576, "selected EAL host")?;
    if after.sha256 != identity.sha256 {
        return Err(invalid("selected host changed during capability discovery"));
    }
    target.executable_sha256 = Some(identity.sha256);
    Ok(target)
}

fn resolve_executable(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() || path.components().count() > 1 {
        return path
            .canonicalize()
            .map_err(|_| invalid("selected eal-host path does not exist"));
    }
    if let Some(search) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&search).filter(|directory| directory.is_absolute())
        {
            let candidate = directory.join(path);
            if candidate.is_file() {
                return candidate
                    .canonicalize()
                    .map_err(|_| invalid("cannot resolve selected eal-host"));
            }
        }
    }
    Err(invalid(
        "selected eal-host was not found in an absolute PATH directory",
    ))
}
