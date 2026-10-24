//! Bounded, immutable contracts for a generated collector and its held-out fixtures.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Error, acquisition::ToolArgument, domain::EvidenceNeed};

pub const CONTRACT_SCHEMA: &str = "engarg.tool-contract/1";
pub const MAX_FILES: usize = 16;
pub const MAX_FILE_BYTES: usize = 131_072;
pub const MAX_SOURCE_BYTES: usize = 524_288;
pub const MAX_FIXTURES: usize = 64;
pub const MAX_JSON_BYTES: usize = 262_144;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureKind {
    Positive,
    Negative,
    AcquisitionError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractFixture {
    pub name: String,
    pub kind: FixtureKind,
    pub input: Value,
    pub expected_value: Option<Value>,
    pub files: Vec<SourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDraft {
    pub purpose: String,
    pub parameters: Vec<String>,
    pub input_schema: Value,
    pub result_schema: Value,
    pub arguments: Vec<ToolArgument>,
    pub unresolved_arguments: Vec<String>,
    pub fixtures: Vec<ContractFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedContract {
    pub schema: String,
    pub language: String,
    pub need: EvidenceNeed,
    pub request_context: String,
    pub toolchain_description: String,
    pub draft: ContractDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeCandidate {
    pub language: String,
    pub files: Vec<SourceFile>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

pub fn validate_language(language: &str) -> Result<(), Error> {
    if language.is_empty()
        || language.len() > 64
        || !language.as_bytes()[0].is_ascii_alphanumeric()
        || !language
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_+.-".contains(&b))
    {
        return Err(invalid("tool language must be a bounded ASCII identifier"));
    }
    Ok(())
}

/// Use portable, relative file paths; never let a model select a host path.
fn validate_path(path: &str) -> Result<(), Error> {
    if path.is_empty()
        || path.len() > 200
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid(format!("unsafe generated file path {path:?}")));
    }
    Ok(())
}

fn validate_files(files: &[SourceFile], required: bool) -> Result<(), Error> {
    if (required && files.is_empty()) || files.len() > MAX_FILES {
        return Err(invalid("generated files must contain 1 to 16 entries"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    for file in files {
        validate_path(&file.path)?;
        if !names.insert(file.path.to_ascii_lowercase()) {
            return Err(invalid("duplicate generated file path"));
        }
        if file.content.len() > MAX_FILE_BYTES || file.content.contains('\0') {
            return Err(invalid("generated file is too large or contains NUL"));
        }
        total = total
            .checked_add(file.content.len())
            .ok_or_else(|| invalid("generated source size overflow"))?;
    }
    if total > MAX_SOURCE_BYTES {
        return Err(invalid("generated source exceeds 512 KiB"));
    }
    Ok(())
}

fn bounded_json(value: &Value) -> Result<(), Error> {
    if serde_json::to_vec(value)?.len() > MAX_JSON_BYTES {
        return Err(invalid("contract JSON value exceeds 256 KiB"));
    }
    Ok(())
}

fn validate_schema(schema: &Value) -> Result<(), Error> {
    bounded_json(schema)?;
    fn local(value: &Value, depth: usize) -> Result<(), Error> {
        if depth > 64 {
            return Err(invalid("contract schema is too deeply nested"));
        }
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if matches!(
                        key.as_str(),
                        "$ref" | "$dynamicRef" | "$recursiveRef" | "$id"
                    ) && !child
                        .as_str()
                        .is_some_and(|reference| reference.starts_with('#'))
                    {
                        return Err(invalid("contract schemas may use local references only"));
                    }
                    local(child, depth + 1)?;
                }
            }
            Value::Array(array) => {
                for child in array {
                    local(child, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    local(schema, 0)?;
    jsonschema::validator_for(schema)
        .map_err(|e| invalid(format!("invalid contract schema: {e}")))?;
    Ok(())
}

fn parameter(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && name.as_bytes()[0].is_ascii_alphabetic()
}

pub fn validate_contract(contract: &NeedContract) -> Result<(), Error> {
    if contract.schema != CONTRACT_SCHEMA {
        return Err(invalid("unsupported tool contract schema"));
    }
    validate_language(&contract.language)?;
    if contract.need.id.trim().is_empty()
        || contract.draft.purpose.trim().is_empty()
        || contract.draft.purpose.len() > 16_384
        || contract.request_context.len() > 1_048_576
        || contract.toolchain_description.trim().is_empty()
        || contract.toolchain_description.len() > 16_384
    {
        return Err(invalid(
            "tool contract requires bounded need, purpose, context and toolchain",
        ));
    }
    let draft = &contract.draft;
    if draft.parameters.len() > 64 {
        return Err(invalid("tool contract has too many parameters"));
    }
    let mut parameters = BTreeSet::new();
    for name in &draft.parameters {
        if !parameter(name) || !parameters.insert(name.as_str()) {
            return Err(invalid("invalid or duplicate tool parameter"));
        }
    }
    validate_schema(&draft.input_schema)?;
    validate_schema(&draft.result_schema)?;
    if draft.input_schema["type"] != "object" || draft.input_schema["additionalProperties"] != false
    {
        return Err(invalid(
            "collector input schema must describe a closed object",
        ));
    }
    if draft.result_schema["type"] != "object"
        || draft.result_schema["properties"]["collected"]["type"] != "boolean"
        || !draft.result_schema["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|name| name == "collected"))
    {
        return Err(invalid(
            "result schema requires a boolean collected property",
        ));
    }
    let properties = draft.input_schema["properties"]
        .as_object()
        .ok_or_else(|| invalid("collector input schema requires properties"))?;
    if properties
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != parameters
    {
        return Err(invalid(
            "input schema properties must exactly match parameters",
        ));
    }
    let mut supplied = BTreeSet::new();
    for argument in &draft.arguments {
        if !parameters.contains(argument.name.as_str())
            || !supplied.insert(argument.name.as_str())
            || argument.value.trim().is_empty()
            || argument.value.len() > MAX_JSON_BYTES
        {
            return Err(invalid("unknown, duplicate or empty supplied parameter"));
        }
    }
    for name in &draft.unresolved_arguments {
        if !parameters.contains(name.as_str()) || !supplied.insert(name.as_str()) {
            return Err(invalid(
                "unknown, duplicate or supplied-and-unresolved parameter",
            ));
        }
    }
    if supplied != parameters {
        return Err(invalid(
            "every parameter must be supplied or explicitly unresolved",
        ));
    }
    // Admit the same consumer contract that will later be exported, before any
    // implementation call or execution can spend its budget.
    let descriptor = eal_collector_contracts::CollectorDescriptor {
        id: "generated_contract_validation".into(),
        version: "1".into(),
        purpose: draft.purpose.clone(),
        parameters: draft.parameters.clone(),
        input_schema: draft.input_schema.clone(),
        result_schema: draft.result_schema.clone(),
        command: eal_collector_contracts::CommandTemplate {
            argv: vec!["/engarg-contract-validation".into()],
            inherit_env: vec![],
            timeout_seconds: 30,
            max_output_bytes: 1_048_576,
            parallel_safe: false,
        },
        acquisition_predicate: "collected == true".into(),
        constraints: vec![],
    };
    descriptor
        .validate_for(eal_collector_contracts::AuthoringProfile::RuntimeExportable)
        .map_err(invalid)?;
    crate::collector_catalogue::CollectorDescriptor(&descriptor).input(
        &crate::acquisition::AcquisitionPlan {
            need_id: contract.need.id.clone(),
            tool_id: descriptor.id.clone(),
            arguments: draft.arguments.clone(),
            unresolved_arguments: draft.unresolved_arguments.clone(),
        },
    )?;
    if draft.fixtures.len() < 3 || draft.fixtures.len() > MAX_FIXTURES {
        return Err(invalid("tool contract requires 3 to 64 fixtures"));
    }
    let input = jsonschema::validator_for(&draft.input_schema)
        .map_err(|e| invalid(format!("invalid input schema: {e}")))?;
    let result = jsonschema::validator_for(&draft.result_schema)
        .map_err(|e| invalid(format!("invalid result schema: {e}")))?;
    let mut names = BTreeSet::new();
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    let mut error = false;
    let mut fixture_bytes = 0usize;
    for fixture in &draft.fixtures {
        if fixture.name.trim().is_empty()
            || fixture.name.len() > 128
            || !names.insert(&fixture.name)
        {
            return Err(invalid("fixture names must be unique and bounded"));
        }
        validate_files(&fixture.files, false)?;
        bounded_json(&fixture.input)?;
        if !input.is_valid(&fixture.input) {
            return Err(invalid(format!(
                "fixture {} violates input schema",
                fixture.name
            )));
        }
        fixture_bytes = fixture_bytes
            .checked_add(serde_json::to_vec(fixture)?.len())
            .ok_or_else(|| invalid("fixture size overflow"))?;
        match fixture.kind {
            FixtureKind::Positive | FixtureKind::Negative => {
                let expected = fixture
                    .expected_value
                    .as_ref()
                    .ok_or_else(|| invalid("collected fixtures require expected_value"))?;
                bounded_json(expected)?;
                if expected["collected"] != true || !result.is_valid(expected) {
                    return Err(invalid(
                        "positive and negative fixtures require valid collected=true results",
                    ));
                }
                match fixture.kind {
                    FixtureKind::Positive => positive.push(expected),
                    FixtureKind::Negative => negative.push(expected),
                    FixtureKind::AcquisitionError => unreachable!(),
                }
            }
            FixtureKind::AcquisitionError => {
                error = true;
                if fixture.expected_value.is_some() {
                    return Err(invalid(
                        "acquisition-error fixtures must not have an expected collected value",
                    ));
                }
            }
        }
    }
    if fixture_bytes > 1_048_576 {
        return Err(invalid("contract fixtures exceed 1 MiB"));
    }
    if positive.is_empty()
        || negative.is_empty()
        || !error
        || positive.iter().any(|p| negative.contains(p))
    {
        return Err(invalid(
            "contract requires distinct positive, negative and acquisition-error fixtures",
        ));
    }
    Ok(())
}

pub fn validate_candidate(contract: &NeedContract, candidate: &CodeCandidate) -> Result<(), Error> {
    validate_contract(contract)?;
    validate_language(&candidate.language)?;
    if candidate.language != contract.language {
        return Err(invalid("candidate language differs from frozen contract"));
    }
    validate_files(&candidate.files, true)?;
    for fixture in &contract.draft.fixtures {
        if fixture.files.iter().any(|fixture_file| {
            candidate
                .files
                .iter()
                .any(|source| source.path == fixture_file.path)
        }) {
            return Err(invalid("fixture files must not overwrite generated source"));
        }
    }
    Ok(())
}

/// Canonical object-key ordering gives stable digests independent of map insertion order.
pub fn digest_json<T: Serialize>(value: &T) -> Result<String, Error> {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted: std::collections::BTreeMap<_, _> = map
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            other => other,
        }
    }
    let bytes = serde_json::to_vec(&canonical(serde_json::to_value(value)?))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn contract_digest(contract: &NeedContract) -> Result<String, Error> {
    validate_contract(contract)?;
    digest_json(contract)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn portable_paths_reject_escape_and_aliases() {
        for path in ["../x", "/x", "a//b", "a/./b", "C:/x", "a\\b", "a/../b"] {
            assert!(validate_path(path).is_err(), "{path}");
        }
        assert!(validate_path("src/main.rs").is_ok());
    }

    #[test]
    fn schemas_cannot_retrieve_remote_references() {
        assert!(validate_schema(&json!({"$ref":"https://example.com/schema"})).is_err());
        assert!(validate_schema(&json!({"type":"object","properties":{}})).is_ok());
    }

    fn contract() -> NeedContract {
        let case: crate::domain::ArgumentCase =
            serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        NeedContract {
            schema: CONTRACT_SCHEMA.into(),
            language: "python".into(),
            need: case.evidence_plan[0].clone(),
            request_context: "{}".into(),
            toolchain_description: "operator-selected interpreter".into(),
            draft: ContractDraft {
                purpose: "Measure a synthetic local instrument".into(),
                parameters: vec!["mode".into()],
                input_schema: json!({"type":"object","additionalProperties":false,
                    "properties":{"mode":{"type":"string"}},"required":["mode"]}),
                result_schema: json!({"type":"object","properties":{"collected":{"type":"boolean"},"present":{"type":"boolean"}},
                    "required":["collected","present"]}),
                arguments: vec![ToolArgument {
                    name: "mode".into(),
                    value: "positive".into(),
                }],
                unresolved_arguments: vec![],
                fixtures: vec![
                    ContractFixture {
                        name: "positive".into(),
                        kind: FixtureKind::Positive,
                        input: json!({"mode":"positive"}),
                        expected_value: Some(json!({"collected":true,"present":true})),
                        files: vec![],
                    },
                    ContractFixture {
                        name: "negative".into(),
                        kind: FixtureKind::Negative,
                        input: json!({"mode":"negative"}),
                        expected_value: Some(json!({"collected":true,"present":false})),
                        files: vec![],
                    },
                    ContractFixture {
                        name: "error".into(),
                        kind: FixtureKind::AcquisitionError,
                        input: json!({"mode":"error"}),
                        expected_value: None,
                        files: vec![],
                    },
                ],
            },
        }
    }

    #[test]
    fn contract_requires_closed_input_and_required_boolean_collection() {
        let valid = contract();
        validate_contract(&valid).unwrap();
        let mut open = valid.clone();
        open.draft.input_schema["additionalProperties"] = json!(true);
        assert!(validate_contract(&open).is_err());
        let mut optional = valid.clone();
        optional.draft.result_schema["required"] = json!(["present"]);
        assert!(validate_contract(&optional).is_err());
        let mut wrong_type = valid;
        wrong_type.draft.result_schema["properties"]["collected"]["type"] = json!("string");
        assert!(validate_contract(&wrong_type).is_err());
    }

    #[test]
    fn eal_export_admission_rejects_oversized_purpose_before_coding() {
        let mut value = contract();
        value.draft.purpose = "x".repeat(8193);
        assert!(validate_contract(&value).is_err());
        let mut value = contract();
        value.draft.input_schema["properties"]["mode"] = json!({"type":"integer"});
        assert!(validate_contract(&value).is_err());
    }

    #[test]
    fn rejects_case_aliases_and_supplied_unresolved_overlap() {
        let files = vec![
            SourceFile {
                path: "Data/record.json".into(),
                content: "{}".into(),
            },
            SourceFile {
                path: "data/record.json".into(),
                content: "{}".into(),
            },
        ];
        assert!(validate_files(&files, true).is_err());
        let mut value = contract();
        value.draft.unresolved_arguments.push("mode".into());
        assert!(validate_contract(&value).is_err());
    }

    #[test]
    fn digests_canonicalise_object_order() {
        let left: Value = serde_json::from_str(r#"{"b":2,"a":{"y":1,"x":0}}"#).unwrap();
        let right: Value = serde_json::from_str(r#"{"a":{"x":0,"y":1},"b":2}"#).unwrap();
        assert_eq!(digest_json(&left).unwrap(), digest_json(&right).unwrap());
    }
}
