//! Mandatory protocol examples and rejection expectations for generated collectors.
use crate::Error;
use serde_json::{Value, json};

/// Version 3 keeps host-authored metadata inside its verification namespace.
/// Changing mandatory protocol behaviour requires a new receipt version.
pub const TEST_SCHEMA: &str = "engarg.tool-tests/3";

/// Host diagnostics are opaque metadata, never a caller's operation selectors.
pub fn protocol_fixture_context(name: &str) -> Result<Value, Error> {
    Ok(json!({"engarg_verification":{
        "fixture":name,"precision":protocol_precision_context()?
    }}))
}

pub fn protocol_precision_context() -> Result<Value, Error> {
    eal_collector_contracts::strict_json_exact(br#"{
        "integer":90071992547409931234567890,"decimal":1.234567890123456789,
        "escaped":"quote \" slash \\ CRLF\r\n\u2028",
        "opaque_number":{"$serde_json::private::Number":"literal object"},
        "opaque_numeric_number":{"$serde_json::private::Number":"123456789012345678901234567890"},
        "opaque_raw_value":{"$serde_json::private::RawValue":"{\"literal\":true}"},
        "nested_opaque":[{"$serde_json::private::Number":"opaque"},{"$serde_json::private::RawValue":{"literal":"object"}}]
    }"#,Default::default()).map_err(Error::InvalidInput)
}

#[derive(Clone, Copy)]
pub(crate) enum RejectionCase {
    MalformedJson,
    MissingIdentity,
    InvalidContext,
}
pub(crate) const REJECTION_CASES: &[RejectionCase] = &[
    RejectionCase::MalformedJson,
    RejectionCase::MissingIdentity,
    RejectionCase::InvalidContext,
];
impl RejectionCase {
    pub fn name(self) -> &'static str {
        match self {
            Self::MalformedJson => "malformed_json",
            Self::MissingIdentity => "missing_identity",
            Self::InvalidContext => "invalid_context",
        }
    }
    pub fn request(self, valid: &Value) -> Result<Vec<u8>, Error> {
        let mut request = valid.clone();
        match self {
            Self::MalformedJson => return Ok(b"{".to_vec()),
            Self::MissingIdentity => {
                request
                    .as_object_mut()
                    .ok_or_else(|| {
                        Error::InvalidInput("protocol prototype requires an object".into())
                    })?
                    .remove("evidence_id");
            }
            Self::InvalidContext => request["context"] = json!("invalid"),
        }
        Ok(serde_json::to_vec(&request)?)
    }
}
pub(crate) fn rejected(returncode: i32, stdout_empty: bool) -> bool {
    returncode != 0 && stdout_empty
}
