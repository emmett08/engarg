//! Compatibility routing for EngArg-specific bindings and optional EAL adapters.
//! Retain these facades until native catalogues provide equivalent literal-query
//! collectors and saved binding migration has been verified.
use crate::Error;
use serde_json::Value;
use std::collections::BTreeMap;

pub fn parse_request(bytes: &[u8]) -> Result<Value, Error> {
    eal_adapters::parse_json(bytes).map_err(Into::into)
}

pub async fn observe(tool: &str, value: Value) -> Result<Value, Error> {
    if matches!(tool, "artifact_json" | "scoped_json") {
        let args: BTreeMap<String, String> =
            serde_json::from_value(value["input"]["arguments"].clone())?;
        eal_adapters::profiles::require(tool)?.validate_arguments(&args)?;
        let path = std::path::Path::new(&args["inputs"]);
        return if tool == "artifact_json" {
            crate::retained_collectors::observe(tool, value.clone(), path)
        } else {
            crate::scoped_collectors::observe(value.clone(), path).await
        };
    }
    eal_adapters::generic::observe(tool, value)
        .await
        .map_err(Into::into)
}
