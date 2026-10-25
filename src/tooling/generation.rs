//! Separate measurement-contract design from implementation; never expose the fixture oracle.

use serde::Deserialize;
use serde_json::{Value, json};

use super::contracts::{
    CodeCandidate, ContractDraft, ContractFixture, FixtureKind, NeedContract, SourceFile,
};
use crate::{
    Error,
    acquisition::ToolArgument,
    domain::{EvidenceNeed, Request},
    provider::{CompletionRequest, Role},
};

fn object(properties: Value) -> Value {
    let required = properties
        .as_object()
        .expect("static object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn files_schema() -> Value {
    json!({"type":"array","items":object(json!({
        "path":{"type":"string"},"content":{"type":"string"}
    }))})
}

fn draft_schema() -> Value {
    object(json!({
        "purpose":{"type":"string"},
        "parameters":{"type":"array","items":{"type":"string"}},
        "input_schema_json":{"type":"string"},
        "result_schema_json":{"type":"string"},
        "arguments":{"type":"array","items":object(json!({
            "name":{"type":"string"},"value":{"type":"string"}
        }))},
        "unresolved_arguments":{"type":"array","items":{"type":"string"}},
        "fixtures":{"type":"array","items":object(json!({
            "name":{"type":"string"},
            "kind":{"type":"string","enum":["positive","negative","acquisition_error"]},
            "input_json":{"type":"string"},
            "expected_value_json":{"type":["string","null"]},
            "files":files_schema()
        }))}
    }))
}

pub fn draft_request(
    need: &EvidenceNeed,
    request: &Request,
    language: &str,
    description: &str,
) -> CompletionRequest {
    CompletionRequest {
        role: Role::Constructor,
        system: concat!(
            "Design one bounded evidence collector contract for the exact supplied evidence need. ",
            "A collector records observations; it does not adjudicate the claim or manufacture evidence, credentials or independent review. ",
            "Use only supplied values for arguments; partition every parameter exactly once between arguments and unresolved_arguments. ",
            "Choose a concrete observable operation and local-only JSON Schemas. The input schema must have type:object, additionalProperties:false, and properties exactly matching parameters. ",
            "The result schema describes an object observation value, with properties.collected.type:boolean and required containing collected. It does not describe the outer transport envelope. ",
            "Include distinct positive and negative observation fixtures and an acquisition_error fixture. ",
            "Positive and negative expected values must satisfy the result schema, contain collected:true, and differ from each other. ",
            "A negative measurement is a successful acquisition with a negative result; an acquisition error is an unavailable observation and has null expected_value_json. ",
            "Fixtures are synthetic tests, never evidence supporting the engineering argument. ",
            "Encode input_schema_json, result_schema_json, fixture input_json and non-null expected_value_json as exact JSON text strings. ",
            "Parameter names start with an ASCII letter and contain at most 64 ASCII letters, digits or underscores. Use at most 64 parameters and 3 to 64 fixtures. ",
            "Each fixture has at most 16 files, each at most 128 KiB, totaling at most 512 KiB; all encoded fixture data together is at most 1 MiB. ",
            "Schema and individual JSON values are at most 256 KiB; purpose is at most 8192 characters and 16 KiB and fixture names are unique and at most 128 characters. ",
            "Fixture files use safe relative paths without dot or dot-dot components, absolute paths, backslashes or case-fold aliases and may contain only synthetic local data. Return only the strict structured response."
        ).into(),
        user: json!({
            "need":need,"request":request,"language":language,"toolchain_description":description
        }).to_string(),
        schema: draft_schema(),
    }
}

pub fn code_request(contract: &NeedContract) -> CompletionRequest {
    CompletionRequest {
        role: Role::Single,
        system: concat!(
            "Implement the supplied frozen collector contract in the selected language and operator toolchain. ",
            "Return only source files and the unchanged language identifier. Use 1 to 16 source files, at most 128 KiB each and 512 KiB total. File paths must be safe relative paths without dot or dot-dot components, absolute paths, backslashes or case-fold aliases. ",
            "The program reads exactly one bounded JSON object from stdin and emits exactly one JSON object on stdout on success. ",
            "The request has exactly evidence_id:string, environment:string, tool:string, tool_version:string, input:object, context:object. ",
            "evidence_id is an opaque nonempty caller identity and may be a digit-leading 64-character source digest; do not require an authored EAL identifier or rewrite it. ",
            "Validate input using the frozen schema and reject malformed requests or acquisition failures with a nonzero process exit, with no success JSON on stdout. ",
            "A successful response is {value:OBSERVATION,context:CONTEXT,request:{tool:TOOL,tool_version:VERSION,input:INPUT,context:CONTEXT}}. ",
            "OBSERVATION satisfies result_schema; positive and negative measured results both use collected:true. ",
            "Copy the supplied tool and tool_version from each runtime request, including operator aliases. Do not hardcode an exported tool name or a context object. ",
            "Preserve input and context values exactly, including arbitrarily large JSON integers, precise decimals, escaped strings and Unicode. ",
            "Do not round metadata by parsing it through floating-point numbers; retain original JSON tokens or use a lossless JSON representation. ",
            "Opaque JSON objects may have literal keys named $serde_json::private::Number or $serde_json::private::RawValue; never reinterpret those objects as serialization internals. ",
            "For Rust opaque context or input, use serde_json::value::RawValue with raw_value support and serialize the raw values directly through typed transport structs, without intermediate serde_json::Value or json! buffering. Inspect only typed business fields separately and retain the original raw input/context for the response. ",
            "Keep stdout free of logs; diagnostics belong on stderr. Source files are mounted read-only at /src, fixture files read-only at /fixtures, and /build is writable. ",
            "Read synthetic fixture data through the supplied file layout when implementing the measurement; fixture outcomes are held out by the host. ",
            "Do not invent measurements, credentials, independent acceptance or host capabilities. ",
            "Use only the configured toolchain and the stated runtime network access. The host, not generated code, runs fixtures and decides whether implementation passes. ",
            "Source files must contain the complete implementation; do not return shell commands, deployment actions, a narrative, or test expectations."
        ).into(),
        user: json!({
            "schema":contract.schema,
            "language":contract.language,
            "need":contract.need,
            "request_context":contract.request_context,
            "toolchain_description":contract.toolchain_description,
            "purpose":contract.draft.purpose,
            "parameters":contract.draft.parameters,
            "input_schema":contract.draft.input_schema,
            "result_schema":contract.draft.result_schema,
            "fixtures":contract.draft.fixtures.iter().map(|fixture| json!({
                "name":fixture.name,"input":fixture.input,"files":fixture.files
            })).collect::<Vec<_>>()
        }).to_string(),
        schema: object(json!({"language":{"type":"string"},"files":files_schema()})),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftWire {
    purpose: String,
    parameters: Vec<String>,
    input_schema_json: String,
    result_schema_json: String,
    arguments: Vec<ToolArgument>,
    unresolved_arguments: Vec<String>,
    fixtures: Vec<FixtureWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureWire {
    name: String,
    kind: FixtureKind,
    input_json: String,
    expected_value_json: Option<String>,
    files: Vec<SourceFile>,
}

fn strict_json(bytes: &[u8]) -> Result<Value, Error> {
    eal_collector_contracts::strict_json_exact(
        bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: 8 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(|error| Error::InvalidOutput(format!("invalid tool generation JSON: {error}")))
}

pub fn parse_draft(bytes: &[u8]) -> Result<ContractDraft, Error> {
    let wire: DraftWire = serde_json::from_value(strict_json(bytes)?).map_err(|error| {
        Error::InvalidOutput(format!("invalid tool contract response: {error}"))
    })?;
    let fixtures = wire
        .fixtures
        .into_iter()
        .map(|fixture| {
            Ok(ContractFixture {
                name: fixture.name,
                kind: fixture.kind,
                input: strict_json(fixture.input_json.as_bytes())?,
                expected_value: fixture
                    .expected_value_json
                    .map(|text| strict_json(text.as_bytes()))
                    .transpose()?,
                files: fixture.files,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(ContractDraft {
        purpose: wire.purpose,
        parameters: wire.parameters,
        input_schema: strict_json(wire.input_schema_json.as_bytes())?,
        result_schema: strict_json(wire.result_schema_json.as_bytes())?,
        arguments: wire.arguments,
        unresolved_arguments: wire.unresolved_arguments,
        fixtures,
    })
}

pub fn parse_candidate(bytes: &[u8]) -> Result<CodeCandidate, Error> {
    serde_json::from_value(strict_json(bytes)?).map_err(|error| {
        Error::InvalidOutput(format!("invalid tool implementation response: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_rejects_duplicate_keys_and_unknown_fields() {
        assert!(parse_candidate(br#"{"language":"python","language":"rust","files":[]}"#).is_err());
        assert!(parse_candidate(br#"{"language":"python","files":[],"command":"run"}"#).is_err());
    }

    #[test]
    fn strict_wire_schema_requires_encoded_schema_strings() {
        let schema = draft_schema();
        assert_eq!(schema["properties"]["input_schema_json"]["type"], "string");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["fixtures"]["items"]["additionalProperties"],
            false
        );
    }
}
