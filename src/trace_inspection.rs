//! Compatibility facade for the optional EAL trace adapter.
//! Remove this module alias in EngArg 0.11 after callers migrate to eal-adapters.
use crate::Error;
use serde_json::Value;
pub fn inspect_trace(trace: &Value, kind: &str, name: &str) -> Result<Value, Error> {
    eal_adapters::trace::inspect_trace(trace, kind, name).map_err(Into::into)
}
