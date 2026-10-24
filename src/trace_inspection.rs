//! Inspect retained OTLP trace data without establishing provenance or deployment state.
//!
//! A search summary is not a full trace. Selector equality is checked on one
//! actual span, never by combining spans or borrowing resource attributes.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::Error;

const KIND: &str = "dpe.rollout_kind";
const NAME: &str = "dpe.rollout_name";

type Attributes = BTreeMap<String, Value>;

#[derive(Default)]
struct Inspection {
    resource_span_count: usize,
    span_count: usize,
    matches: Vec<Value>,
    diagnostics: Vec<String>,
}

/// Inspect complete-span OTLP JSON returned by a Tempo trace-by-ID read.
///
/// Both protobuf JSON field spellings and Tempo's `batches` wrapper are accepted.
/// Any malformed or ambiguous record suppresses all candidate matches. A valid
/// structure still cannot establish that an HTTP response contains every span,
/// that labels are trustworthy, or that the identified resource currently exists.
pub fn inspect_trace(trace: &Value, kind: &str, name: &str) -> Result<Value, Error> {
    if [kind, name]
        .iter()
        .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
    {
        return Err(Error::InvalidInput(
            "trace inspection requires nonempty literal rollout kind and name".into(),
        ));
    }
    let mut inspection = Inspection::default();
    if let Err(message) = inspection.inspect(trace, kind, name) {
        inspection.diagnostics.push(message);
    }
    let structurally_valid = inspection.diagnostics.is_empty();
    if !structurally_valid {
        inspection.matches.clear();
    }
    Ok(json!({
        "schema": "engarg.trace-inspection/1",
        "selector": {"kind": kind, "name": name, "attribute_scope": "span"},
        "structurally_valid": structurally_valid,
        "resource_span_count": inspection.resource_span_count,
        "span_count": inspection.span_count,
        "candidate_matches": inspection.matches.len(),
        "matching_spans": inspection.matches,
        "diagnostics": inspection.diagnostics,
        "provenance_verified": false,
        "current_state_verified": false,
        "fleet_coverage_verified": false,
        "completeness_verified": false,
        "limitations": [
            "Same-span string equality identifies candidates only; retained labels do not independently establish resource provenance.",
            "Structural validation does not verify server-side trace completeness, resource current state, production classification or fleet coverage.",
            "Independent resource identity, lifecycle, inventory and query-route evidence remain required."
        ]
    }))
}

impl Inspection {
    fn inspect(&mut self, trace: &Value, kind: &str, name: &str) -> Result<(), String> {
        let root = object(trace, "trace payload")?;
        for key in ["partial", "partialSuccess", "partial_success"] {
            if root.contains_key(key) && root[key] != Value::Bool(false) {
                return Err("trace payload declares partial or incomplete data".into());
            }
        }
        let batches = alias(root, &["resourceSpans", "resource_spans", "batches"])?
            .ok_or("trace payload contains no OTLP resource-span collection")?;
        let batches = batches
            .as_array()
            .ok_or("OTLP resource-span collection must be an array")?;
        for (batch_index, batch) in batches.iter().enumerate() {
            if let Err(message) = self.batch(batch, batch_index, kind, name) {
                self.diagnostics.push(message);
            }
        }
        Ok(())
    }

    fn batch(
        &mut self,
        batch: &Value,
        batch_index: usize,
        kind: &str,
        name: &str,
    ) -> Result<(), String> {
        self.resource_span_count += 1;
        let batch = object(batch, "OTLP resource-span record")?;
        let resource = match batch.get("resource") {
            None => Attributes::new(),
            Some(value) => attributes(object(value, "OTLP resource")?)?,
        };
        let scopes = alias(
            batch,
            &[
                "scopeSpans",
                "scope_spans",
                "instrumentationLibrarySpans",
                "instrumentation_library_spans",
            ],
        )?
        .ok_or("OTLP resource-span record has no span collection")?
        .as_array()
        .ok_or("OTLP scope-span collection must be an array")?;
        for (scope_index, scope) in scopes.iter().enumerate() {
            let scope = object(scope, "OTLP scope-span record")?;
            let spans = scope
                .get("spans")
                .and_then(Value::as_array)
                .ok_or("OTLP scope-span record must contain a spans array")?;
            for (span_index, span) in spans.iter().enumerate() {
                self.span_count += 1;
                if let Err(message) = self.span(span, &resource, kind, name) {
                    self.diagnostics.push(format!(
                        "batch {batch_index}, scope {scope_index}, span {span_index}: {message}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn span(
        &mut self,
        span: &Value,
        resource: &Attributes,
        kind: &str,
        name: &str,
    ) -> Result<(), String> {
        let span = object(span, "OTLP span")?;
        let attributes = attributes(span)?;
        let trace_id = identifier(span, &["traceId", "trace_id"], 32)?;
        let span_id = identifier(span, &["spanId", "span_id"], 16)?;
        let start = timestamp(span, &["startTimeUnixNano", "start_time_unix_nano"])?;
        let end = timestamp(span, &["endTimeUnixNano", "end_time_unix_nano"])?;
        if end < start {
            return Err("span end precedes its start".into());
        }
        if attributes.get(KIND).and_then(string_value) != Some(kind)
            || attributes.get(NAME).and_then(string_value) != Some(name)
        {
            return Ok(());
        }
        self.matches.push(json!({
            "trace_id": trace_id,
            "span_id": span_id,
            "span_name": span.get("name"),
            "start_time_unix_nano": start.to_string(),
            "end_time_unix_nano": end.to_string(),
            "span_attributes": attributes,
            "resource_attributes": resource,
            "cluster_attributes": {
                "span": selected(&attributes, |key| key.contains("cluster")),
                "resource": selected(resource, |key| key.contains("cluster"))
            },
            "rollout_identity_attributes": {
                "span": selected(&attributes, rollout_identity),
                "resource": selected(resource, rollout_identity)
            }
        }));
        Ok(())
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))
}

fn alias<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Result<Option<&'a Value>, String> {
    let mut found = None;
    for key in keys {
        if let Some(value) = object.get(*key) {
            if found.is_some() {
                return Err(format!("ambiguous duplicate field aliases for {}", keys[0]));
            }
            found = Some(value);
        }
    }
    Ok(found)
}

fn identifier<'a>(
    span: &'a Map<String, Value>,
    keys: &[&str],
    length: usize,
) -> Result<&'a str, String> {
    let value = alias(span, keys)?
        .and_then(Value::as_str)
        .ok_or_else(|| format!("span requires {}", keys[0]))?;
    if value.len() != length
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().all(|byte| byte == b'0')
    {
        return Err(format!(
            "{} must be a nonzero {length}-digit hex identifier",
            keys[0]
        ));
    }
    Ok(value)
}

fn timestamp(span: &Map<String, Value>, keys: &[&str]) -> Result<u64, String> {
    let value = alias(span, keys)?.ok_or_else(|| format!("span requires {}", keys[0]))?;
    let value = match value {
        Value::String(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
            text.parse::<u64>().ok()
        }
        Value::Number(number) => number.as_u64(),
        _ => None,
    };
    value
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{} must be positive integer Unix nanoseconds", keys[0]))
}

fn attributes(record: &Map<String, Value>) -> Result<Attributes, String> {
    if let Some(dropped) = alias(
        record,
        &["droppedAttributesCount", "dropped_attributes_count"],
    )? {
        let count = match dropped {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.parse::<u64>().ok(),
            _ => None,
        };
        if count != Some(0) {
            return Err("attributes are incomplete or have an invalid dropped count".into());
        }
    }
    let Some(raw) = record.get("attributes") else {
        return Ok(Attributes::new());
    };
    let raw = raw.as_array().ok_or("OTLP attributes must be an array")?;
    let mut values = Attributes::new();
    for attribute in raw {
        let attribute = object(attribute, "OTLP attribute")?;
        let key = attribute
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
            .ok_or("OTLP attribute requires a nonempty string key")?;
        let value = attribute
            .get("value")
            .ok_or("OTLP attribute requires a typed value")?;
        validate_any_value(value, 0)?;
        if values.insert(key.to_owned(), value.clone()).is_some() {
            return Err(format!("duplicate OTLP attribute key {key}"));
        }
    }
    Ok(values)
}

fn validate_any_value(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("OTLP attribute nesting exceeds 16 levels".into());
    }
    let value = object(value, "OTLP attribute value")?;
    if value.len() != 1 {
        return Err("OTLP attribute value must contain exactly one type".into());
    }
    let (key, value) = value.iter().next().expect("one value");
    let valid = match key.as_str() {
        "stringValue" | "string_value" | "bytesValue" | "bytes_value" => value.is_string(),
        "boolValue" | "bool_value" => value.is_boolean(),
        "intValue" | "int_value" => {
            value.as_i64().is_some()
                || value
                    .as_str()
                    .is_some_and(|text| text.parse::<i64>().is_ok())
        }
        "doubleValue" | "double_value" => value.is_number(),
        "arrayValue" | "array_value" => {
            let array = object(value, "OTLP array value")?;
            let values = array
                .get("values")
                .and_then(Value::as_array)
                .ok_or("OTLP array value requires a values array")?;
            for value in values {
                validate_any_value(value, depth + 1)?;
            }
            true
        }
        "kvlistValue" | "kvlist_value" => {
            let list = object(value, "OTLP key-value list")?;
            let values = list
                .get("values")
                .and_then(Value::as_array)
                .ok_or("OTLP key-value list requires a values array")?;
            let mut keys = std::collections::BTreeSet::new();
            for value in values {
                let entry = object(value, "OTLP key-value entry")?;
                let key = entry
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or("OTLP key-value entry requires a string key")?;
                if !keys.insert(key) {
                    return Err("duplicate OTLP nested attribute key".into());
                }
                validate_any_value(
                    entry
                        .get("value")
                        .ok_or("OTLP key-value entry requires a value")?,
                    depth + 1,
                )?;
            }
            true
        }
        _ => false,
    };
    valid
        .then_some(())
        .ok_or_else(|| "invalid OTLP attribute type or value".into())
}

fn string_value(value: &Value) -> Option<&str> {
    let value = value.as_object()?;
    alias(value, &["stringValue", "string_value"])
        .ok()??
        .as_str()
}

fn selected(attributes: &Attributes, predicate: impl Fn(&str) -> bool) -> Attributes {
    attributes
        .iter()
        .filter(|(key, _)| predicate(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn rollout_identity(key: &str) -> bool {
    key.contains("rollout") && (key.contains("uid") || key.contains("generation"))
}
