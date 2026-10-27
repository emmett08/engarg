//! Restore raw JSON representation without changing the typed report's meaning.
use crate::{Error, engine::RunReport};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldEncoding {
    Absent,
    Value { value: Value },
}

impl<'de> Deserialize<'de> for FieldEncoding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // An internally tagged derive buffers serde Content, which cannot carry
        // RawValue. Admit the complete tagged object through the exact parser.
        let value = eal_collector_contracts::deserialize_value_exact(deserializer)?;
        let Value::Object(mut fields) = value else {
            return Err(D::Error::custom(
                "retained field encoding requires an object",
            ));
        };
        let kind = fields.remove("kind");
        match kind.as_ref().and_then(Value::as_str) {
            Some("absent") if fields.is_empty() => Ok(Self::Absent),
            Some("value") if fields.len() == 1 && fields.contains_key("value") => Ok(Self::Value {
                value: fields.remove("value").expect("checked encoding value"),
            }),
            _ => Err(D::Error::custom(
                "invalid retained field encoding kind or fields",
            )),
        }
    }
}

pub(super) type Encoding = BTreeMap<String, FieldEncoding>;

fn invalid(message: &str) -> Error {
    Error::InvalidInput(format!("Invalid retained report encoding: {message}"))
}

fn normalise(value: &Value) -> Result<Value, Error> {
    let report: RunReport = serde_json::from_value(value.clone())?;
    Ok(serde_json::to_value(report)?)
}

fn admit_size(value: &impl Serialize) -> Result<(), Error> {
    if serde_json::to_vec(value)?.len() > crate::authoring_checkpoint::MAX_REPORT_BYTES {
        return Err(Error::Budget(
            "retained report encoding exceeds 16 MiB".into(),
        ));
    }
    Ok(())
}

fn component(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn insert(
    encoding: &mut Encoding,
    bytes: &mut usize,
    pointer: String,
    value: FieldEncoding,
) -> Result<(), Error> {
    *bytes = bytes
        .saturating_add(serde_json::to_vec(&pointer)?.len())
        .saturating_add(serde_json::to_vec(&value)?.len())
        .saturating_add(4);
    if *bytes > crate::authoring_checkpoint::MAX_REPORT_BYTES {
        return Err(Error::Budget(
            "retained report encoding exceeds 16 MiB".into(),
        ));
    }
    encoding.insert(pointer, value);
    Ok(())
}

fn differences(
    raw: &Value,
    typed: &Value,
    pointer: &str,
    excluded_roots: &[&str],
    encoding: &mut Encoding,
    bytes: &mut usize,
) -> Result<(), Error> {
    if raw == typed {
        return Ok(());
    }
    match (raw, typed) {
        (Value::Object(raw), Value::Object(typed)) => {
            for key in raw.keys().chain(typed.keys()).collect::<BTreeSet<_>>() {
                if pointer.is_empty() && excluded_roots.contains(&key.as_str()) {
                    continue;
                }
                let path = format!("{pointer}/{}", component(key));
                match (raw.get(key), typed.get(key)) {
                    (Some(raw), Some(typed)) => {
                        differences(raw, typed, &path, excluded_roots, encoding, bytes)?;
                    }
                    (Some(value), None) => insert(
                        encoding,
                        bytes,
                        path,
                        FieldEncoding::Value {
                            value: value.clone(),
                        },
                    )?,
                    (None, Some(_)) => insert(encoding, bytes, path, FieldEncoding::Absent)?,
                    (None, None) => unreachable!("union of object keys"),
                }
            }
        }
        (Value::Array(raw), Value::Array(typed)) if raw.len() == typed.len() => {
            for (index, (raw, typed)) in raw.iter().zip(typed).enumerate() {
                differences(
                    raw,
                    typed,
                    &format!("{pointer}/{index}"),
                    excluded_roots,
                    encoding,
                    bytes,
                )?;
            }
        }
        _ => insert(
            encoding,
            bytes,
            pointer.into(),
            FieldEncoding::Value { value: raw.clone() },
        )?,
    }
    Ok(())
}

pub(super) fn capture(previous: &Value, excluded_roots: &[&str]) -> Result<Encoding, Error> {
    admit_size(previous)?;
    let typed = normalise(previous)?;
    let mut encoding = Encoding::new();
    differences(previous, &typed, "", excluded_roots, &mut encoding, &mut 2)?;
    Ok(encoding)
}

fn parts(pointer: &str) -> Result<Vec<String>, Error> {
    let path = pointer
        .strip_prefix('/')
        .ok_or_else(|| invalid("expected a field pointer"))?;
    path.split('/')
        .map(|part| {
            let mut decoded = String::new();
            let mut chars = part.chars();
            while let Some(next) = chars.next() {
                decoded.push(if next == '~' {
                    match chars.next() {
                        Some('0') => '~',
                        Some('1') => '/',
                        _ => return Err(invalid("invalid pointer escape")),
                    }
                } else {
                    next
                });
            }
            Ok(decoded)
        })
        .collect()
}

fn index(part: &str) -> Result<usize, Error> {
    let value: usize = part.parse().map_err(|_| invalid("invalid array index"))?;
    if value.to_string() != part {
        return Err(invalid("array index must use canonical decimal spelling"));
    }
    Ok(value)
}

fn apply(value: &mut Value, encoding: &Encoding) -> Result<(), Error> {
    for (pointer, field) in encoding {
        for (position, _) in pointer.match_indices('/').skip(1) {
            if encoding.contains_key(&pointer[..position]) {
                return Err(invalid("overlapping field pointers"));
            }
        }
        let parts = parts(pointer)?;
        let (last, parents) = parts.split_last().expect("field pointer has a component");
        let mut selected = &mut *value;
        for part in parents {
            selected = match selected {
                Value::Object(object) => object.get_mut(part),
                Value::Array(array) => array.get_mut(index(part)?),
                _ => None,
            }
            .ok_or_else(|| invalid("field pointer has no parent"))?;
        }
        match (selected, field) {
            (Value::Object(object), FieldEncoding::Absent) => {
                object.remove(last);
            }
            (Value::Object(object), FieldEncoding::Value { value }) => {
                object.insert(last.clone(), value.clone());
            }
            (Value::Array(array), FieldEncoding::Value { value }) => {
                *array
                    .get_mut(index(last)?)
                    .ok_or_else(|| invalid("array index is absent"))? = value.clone();
            }
            _ => {
                return Err(invalid(
                    "encoding requires an object field or existing array value",
                ));
            }
        }
    }
    Ok(())
}

/// Representation patches cannot introduce a semantic argument, support or
/// provenance edit. Commit only after comparing the complete typed reports.
pub(super) fn restore(current: &mut Value, encoding: &Encoding) -> Result<(), Error> {
    if encoding.is_empty() {
        return Ok(());
    }
    admit_size(encoding)?;
    let before = normalise(current)?;
    let mut restored = current.clone();
    apply(&mut restored, encoding)?;
    admit_size(&restored)?;
    if normalise(&restored)? != before {
        return Err(invalid("field encoding changes typed report semantics"));
    }
    *current = restored;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sparse_representation_preserves_absence_null_nested_arrays_and_escaped_keys() {
        let raw =
            json!({"optional":null,"nested":{"a/b~":null},"rows":[{"enabled":false}],"owned":null});
        let typed = json!({"defaulted":false,"nested":{},"rows":[{"enabled":false,"label":null}],"owned":{}});
        let mut encoding = Encoding::new();
        differences(&raw, &typed, "", &["owned"], &mut encoding, &mut 2).unwrap();
        assert_eq!(encoding.len(), 4);
        assert_eq!(encoding["/defaulted"], FieldEncoding::Absent);
        assert_eq!(encoding["/rows/0/label"], FieldEncoding::Absent);
        assert_eq!(
            encoding["/nested/a~1b~0"],
            FieldEncoding::Value { value: Value::Null }
        );
        let mut restored = typed;
        apply(&mut restored, &encoding).unwrap();
        assert_eq!(
            restored,
            json!({"optional":null,"nested":{"a/b~":null},"rows":[{"enabled":false}],"owned":{}})
        );
    }

    #[test]
    fn encoding_wire_roundtrip_preserves_arbitrary_precision_values() {
        let wire = r#"{"/value":{"kind":"value","value":123456789012345678901234567890123456789},"/missing":{"kind":"absent"},"/reserved":{"kind":"value","value":{"$serde_json::private::Number":"literal object key"}}}"#;
        let encoding: Encoding = serde_json::from_str(wire).unwrap();
        let roundtrip: Encoding =
            serde_json::from_value(serde_json::to_value(&encoding).unwrap()).unwrap();
        assert_eq!(roundtrip, encoding);
        assert!(
            serde_json::to_string(&roundtrip)
                .unwrap()
                .contains("123456789012345678901234567890123456789")
        );
        assert_eq!(
            roundtrip["/reserved"],
            FieldEncoding::Value {
                value: json!({"$serde_json::private::Number":"literal object key"})
            }
        );
    }

    #[test]
    fn encoding_wire_rejects_unknown_missing_and_mismatched_fields() {
        for wire in [
            "null",
            "[]",
            "{}",
            r#"{"kind":"absent","value":null}"#,
            r#"{"kind":"value"}"#,
            r#"{"kind":"value","value":1,"extra":false}"#,
            r#"{"kind":false,"value":1}"#,
            r#"{"kind":"unknown"}"#,
        ] {
            assert!(
                serde_json::from_str::<FieldEncoding>(wire).is_err(),
                "{wire}"
            );
            assert!(
                serde_json::from_value::<FieldEncoding>(serde_json::from_str(wire).unwrap())
                    .is_err(),
                "{wire}"
            );
        }
        assert!(
            serde_json::from_str::<FieldEncoding>(r#"{"kind":"absent","kind":"absent"}"#).is_err()
        );
    }

    #[test]
    fn malformed_overlapping_and_aliased_array_pointers_are_rejected() {
        for encoding in [
            BTreeMap::from([("/a~2b".into(), FieldEncoding::Absent)]),
            BTreeMap::from([
                ("/a".into(), FieldEncoding::Value { value: json!({}) }),
                ("/a/b".into(), FieldEncoding::Absent),
            ]),
            BTreeMap::from([("/rows/00".into(), FieldEncoding::Value { value: json!(1) })]),
            BTreeMap::from([("/rows/0".into(), FieldEncoding::Absent)]),
        ] {
            assert!(apply(&mut json!({"a":{"b":1},"rows":[0]}), &encoding).is_err());
        }
    }
}
