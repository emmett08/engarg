//! Bind explicit operator qualification to an observed generated implementation.
use super::{
    capability::{
        CapabilityImplementationRequest, ImplementedCapability, revalidate_implemented_capability,
    },
    contracts::digest_json,
    package::load_verified,
};
use crate::Error;
use eal_collector_contracts::capability::CapabilityDescriptor;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn path(path: &Path) -> Result<String, Error> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("generated registration paths require UTF-8"))
}

pub fn operation_contract(
    record: &ImplementedCapability,
) -> Result<CapabilityImplementationRequest, Error> {
    Ok(serde_json::from_str(
        &load_verified(&record.package_manifest)?
            .contract()
            .request_context,
    )?)
}

/// Materialize only the two optional ToolMapping defaults accepted for generated
/// native commands. Preserve every supplied value, unknown field, execution
/// authority and package pin; the authoritative collector validates their types.
pub fn canonical_mapping_defaults(provider: &Value) -> Result<Value, Error> {
    let mut canonical = provider.clone();
    let mapping = canonical
        .get_mut("mapping")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("generated provider requires an explicit mapping object"))?;
    mapping
        .entry("raw_output_schema_sha256")
        .or_insert(Value::Null);
    mapping.entry("scope_pointers").or_insert_with(|| json!({}));
    Ok(canonical)
}

/// The all-zero configuration hash is an internal input to the collector's pure
/// prepare-native-provider command, which owns its calculation and validation.
pub fn native_provider_template(
    record: &ImplementedCapability,
    executable: &Path,
    supervisor: &Path,
    present_environment: &BTreeSet<String>,
) -> Result<Value, Error> {
    let verified = revalidate_implemented_capability(record, executable)?;
    let executable = executable.canonicalize()?;
    let supervisor = supervisor.canonicalize()?;
    let package = load_verified(&verified.package_manifest)?;
    let required: BTreeSet<_> = package.profile().inherit_env.iter().cloned().collect();
    let environment = verified
        .collector
        .command
        .inherit_env
        .iter()
        .filter(|name| present_environment.contains(*name) || required.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    let mut capability = verified.capability;
    capability.provider.configuration_sha256 = "0".repeat(64);
    Ok(json!({
        "capability":capability,
        "transport":{"kind":"command","program":path(&executable)?,"supervisor":path(&supervisor)?,
            "arguments":["eal","tool-run","--package",path(&verified.package_manifest)?],
            "cwd":path(verified.package_manifest.parent().ok_or_else(||invalid("package directory is unavailable"))?)?,
            "inherit_env":environment,"protocol":{"kind":"eal_acquisition","environment":"engarg_generated","tool":verified.collector.id,"tool_version":"1","context":{}}},
        "mapping":{"tool":verified.collector.id,"raw_output_schema_sha256":null,"response_format":{"kind":"structured"},
            "value_pointer":"/value","observation_time":{"kind":"pointer","pointer":"/observed_at"},"scope_pointers":{}},
        "pinned_files":[{"path":path(&verified.package_manifest)?,"sha256":verified.manifest_sha256}],
        "credential_env":required.into_iter().collect::<Vec<_>>()
    }))
}

/// Qualification may describe scientific scope and mapped output semantics, but
/// cannot replace the implementation, stdin protocol or its verified interface.
pub fn validate_qualified_provider(template: &Value, qualified: &Value) -> Result<(), Error> {
    validate_provider_binding(template, qualified)?;
    let capability: CapabilityDescriptor = serde_json::from_value(qualified["capability"].clone())?;
    if capability.semantics.is_none() {
        return Err(invalid(
            "operator qualification must supply reviewed measurement semantics",
        ));
    }
    Ok(())
}

/// Check immutable execution/interface identity when replacing an earlier pending
/// registration. Scientific qualification is deliberately a separate requirement.
pub fn validate_provider_binding(template: &Value, qualified: &Value) -> Result<(), Error> {
    let template = canonical_mapping_defaults(template)?;
    let qualified = canonical_mapping_defaults(qualified)?;
    let baseline: CapabilityDescriptor = serde_json::from_value(template["capability"].clone())?;
    let capability: CapabilityDescriptor = serde_json::from_value(qualified["capability"].clone())?;
    capability.validate().map_err(Error::InvalidInput)?;
    if capability.id != baseline.id
        || capability.version != baseline.version
        || capability.provider.id != baseline.provider.id
        || capability.provider.version != baseline.provider.version
        || capability.provider.transport != baseline.provider.transport
        || capability.provider.connection != baseline.provider.connection
        || capability.provider.tool != baseline.provider.tool
        || digest_json(&capability.input_schema)? != digest_json(&baseline.input_schema)?
        || digest_json(&capability.output_schema)? != digest_json(&baseline.output_schema)?
        || capability.input_schema_sha256 != baseline.input_schema_sha256
        || capability.output_schema_sha256 != baseline.output_schema_sha256
        || capability.limits.timeout_seconds > baseline.limits.timeout_seconds
        || capability.limits.output_bytes > baseline.limits.output_bytes
        || capability.limits.concurrency > baseline.limits.concurrency
    {
        return Err(invalid(
            "qualification changed the generated implementation interface or widened execution limits",
        ));
    }
    for field in ["transport", "pinned_files", "credential_env"] {
        if digest_json(&template[field])? != digest_json(&qualified[field])? {
            return Err(invalid(
                "qualification changed verified source execution or package pins",
            ));
        }
    }
    for field in [
        "tool",
        "raw_output_schema_sha256",
        "response_format",
        "value_pointer",
    ] {
        if template["mapping"][field] != qualified["mapping"][field] {
            return Err(invalid(
                "qualification changed the generated EAL observation protocol",
            ));
        }
    }
    Ok(())
}
