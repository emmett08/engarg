//! Import operator-selected JSON artifacts and independently retained reviews.
//!
//! Artifact inputs use `engarg.artifact-inputs/1` with `case_digest` and a `needs`
//! map. Each need has a nonempty `artifacts` array of `{path, sha256,
//! observed_at, source}` records. Relative paths stay within the inputs directory;
//! absolute paths are explicit operator grants. JSON bytes must match their pin.
//!
//! Review inputs use `engarg.review-inputs/1` with a `reviews` array. Each record
//! has `{request, observed_at, value}`. The request is the exact four-field EAL
//! correspondence object and value must contain an authored boolean `accepted`.
//! Nothing in either importer decides whether acquired evidence is sufficient.
//! Times are retained UTC RFC3339 instants (`YYYY-MM-DDTHH:MM:SS[.fraction]Z`).
//! EAL applies its declared max_age to those original observation times.

use crate::Error;
use serde::{Deserialize, de};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_FILE_BYTES: usize = 1_048_576;
const MAX_ARTIFACT_BYTES: usize = 4_194_304;
const MAX_ARTIFACTS: usize = 16;
const MAX_REVIEWS: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    evidence_id: String,
    environment: String,
    tool: String,
    tool_version: String,
    input: Value,
    context: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactInputs {
    schema: String,
    case_digest: String,
    needs: BTreeMap<String, ArtifactNeed>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactNeed {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: PathBuf,
    sha256: String,
    observed_at: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewInputs {
    schema: String,
    reviews: Vec<Review>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    request: Correspondence,
    observed_at: String,
    value: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Correspondence {
    tool: String,
    tool_version: String,
    input: Value,
    context: Value,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl Request {
    fn validate(&self) -> Result<&str, Error> {
        if [&self.evidence_id, &self.environment, &self.tool]
            .iter()
            .any(|s| !text(s, 1024))
            || self.tool_version != "1"
            || !self.input.is_object()
            || !self.context.is_object()
        {
            return Err(invalid(
                "invalid retained collector request identity or objects",
            ));
        }
        let case_digest = self.input["case_digest"]
            .as_str()
            .filter(|value| digest(value))
            .ok_or_else(|| invalid("retained collector requires the exact export case_digest"))?;
        if self.context["engarg_export_digest"].as_str() != Some(case_digest) {
            return Err(invalid(
                "retained collector case digest differs from its scoped context",
            ));
        }
        Ok(case_digest)
    }

    fn correspondence(&self) -> Value {
        json!({"tool":self.tool,"tool_version":self.tool_version,"input":self.input,"context":self.context})
    }
}

fn bounded_file(path: &Path) -> Result<Vec<u8>, Error> {
    let selected = std::fs::symlink_metadata(path)?;
    if !selected.is_file() || selected.len() > MAX_FILE_BYTES as u64 {
        return Err(invalid(
            "retained input requires a regular file of at most 1 MiB",
        ));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(invalid(
            "retained input requires a regular file of at most 1 MiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Budget("retained input exceeds 1 MiB".into()));
    }
    Ok(bytes)
}

fn artifact_path(base: &Path, path: &Path) -> Result<PathBuf, Error> {
    if path.to_str().is_none_or(|value| !text(value, 4096)) {
        return Err(invalid("retained artifact path is missing or invalid"));
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid(
            "retained artifact paths must not traverse parent directories",
        ));
    }
    let selected = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    if std::fs::symlink_metadata(&selected)?
        .file_type()
        .is_symlink()
    {
        return Err(invalid("retained artifact must not be a symbolic link"));
    }
    let canonical = selected.canonicalize()?;
    if !path.is_absolute() && !canonical.starts_with(base) {
        return Err(invalid(
            "relative retained artifact escapes the inputs directory",
        ));
    }
    Ok(canonical)
}

// Validate nested duplicate keys before parsing Value, preserving its exact number
// representation rather than converting through floating point in the guard.
struct JsonGuard(usize);
impl<'de> de::DeserializeSeed<'de> for JsonGuard {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if self.0 > 64 {
            return Err(de::Error::custom("JSON exceeds depth 64"));
        }
        deserializer.deserialize_any(self)
    }
}
impl<'de> de::Visitor<'de> for JsonGuard {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("strict bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: de::SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while sequence.next_element_seed(JsonGuard(self.0 + 1))?.is_some() {}
        Ok(())
    }
    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            map.next_value_seed(JsonGuard(self.0 + 1))?;
        }
        Ok(())
    }
}

pub(crate) fn parse_json(bytes: &[u8]) -> Result<Value, Error> {
    use de::DeserializeSeed;
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    JsonGuard(0).deserialize(&mut parser)?;
    parser.end()?;
    Ok(serde_json::from_slice(bytes)?)
}

/// A sortable retained UTC timestamp, with nanoseconds padded for comparison.
fn timestamp(value: &str) -> Result<(String, String), Error> {
    let bytes = value.as_bytes();
    let expected = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if !(20..=30).contains(&bytes.len())
        || expected.iter().any(|(i, b)| bytes.get(*i) != Some(b))
        || bytes.last() != Some(&b'Z')
        || bytes[..19].iter().enumerate().any(|(i, b)| {
            !expected.iter().any(|(separator, _)| *separator == i) && !b.is_ascii_digit()
        })
    {
        return Err(invalid(
            "observed_at requires a retained UTC RFC3339 timestamp",
        ));
    }
    let number = |start: usize, end: usize| -> u32 {
        bytes[start..end]
            .iter()
            .fold(0, |n, b| n * 10 + u32::from(b - b'0'))
    };
    let (year, month, day) = (number(0, 4), number(5, 7), number(8, 10));
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0
        || day == 0
        || day > days
        || number(11, 13) > 23
        || number(14, 16) > 59
        || number(17, 19) > 59
    {
        return Err(invalid("observed_at contains an invalid calendar instant"));
    }
    let fraction = if bytes.len() == 20 {
        ""
    } else {
        if bytes[19] != b'.'
            || bytes.len() < 22
            || !bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit)
        {
            return Err(invalid("observed_at contains an invalid fractional second"));
        }
        &value[20..value.len() - 1]
    };
    Ok((value[..19].into(), format!("{fraction:0<9}")))
}

fn artifacts(request: &Request, bytes: &[u8], base: &Path) -> Result<(Value, String), Error> {
    let inputs: ArtifactInputs = serde_json::from_value(parse_json(bytes)?)?;
    if inputs.schema != "engarg.artifact-inputs/1" || inputs.case_digest != request.validate()? {
        return Err(invalid("retained artifact inputs do not match this export"));
    }
    if request.input["schema"] != "engarg.eal-acquisition/1" {
        return Err(invalid(
            "retained artifacts require an EngArg acquisition input",
        ));
    }
    let need = request.input["need"]["id"]
        .as_str()
        .filter(|value| text(value, 1024))
        .ok_or_else(|| invalid("retained artifacts require an exact evidence need ID"))?;
    let selected = inputs
        .needs
        .get(need)
        .ok_or_else(|| invalid("retained artifact inputs have no matching evidence need"))?;
    if selected.artifacts.is_empty() || selected.artifacts.len() > MAX_ARTIFACTS {
        return Err(invalid(
            "each evidence need requires 1 to 16 retained artifacts",
        ));
    }
    let mut values = Vec::new();
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    let mut oldest = None;
    for artifact in &selected.artifacts {
        if !digest(&artifact.sha256) || !text(&artifact.source, 4096) {
            return Err(invalid(
                "retained artifacts require SHA-256 pins and source provenance",
            ));
        }
        let instant = timestamp(&artifact.observed_at)?;
        let path = artifact_path(base, &artifact.path)?;
        if !paths.insert(path.clone()) {
            return Err(invalid("a retained artifact path is listed more than once"));
        }
        let bytes = bounded_file(&path)?;
        total = total.saturating_add(bytes.len());
        if total > MAX_ARTIFACT_BYTES {
            return Err(Error::Budget(
                "retained artifacts exceed 4 MiB in aggregate".into(),
            ));
        }
        if hash(&bytes) != artifact.sha256 {
            return Err(invalid(
                "retained artifact bytes differ from their SHA-256 pin",
            ));
        }
        let observation = parse_json(&bytes)?;
        if !observation.is_object() {
            return Err(invalid(
                "retained artifact observation must be a JSON object",
            ));
        }
        if oldest.as_ref().is_none_or(|(old, _)| instant < *old) {
            oldest = Some((instant, artifact.observed_at.clone()));
        }
        values.push(json!({"path":artifact.path,"sha256":artifact.sha256,
            "observed_at":artifact.observed_at,"source":artifact.source,"observation":observation}));
    }
    Ok((
        json!({"collected":true,"artifacts":values,
        "interpretation":"retained_artifacts_require_independent_review"}),
        oldest.expect("nonempty artifacts validated").1,
    ))
}

fn review(request: &Request, bytes: &[u8]) -> Result<(Value, String), Error> {
    let inputs: ReviewInputs = serde_json::from_value(parse_json(bytes)?)?;
    if inputs.schema != "engarg.review-inputs/1" || inputs.reviews.len() > MAX_REVIEWS {
        return Err(invalid("invalid retained review schema or review count"));
    }
    if request.tool != "engarg_review_contract" || request.input.get("obligation").is_none() {
        return Err(invalid(
            "retained review requires an exact independent review request",
        ));
    }
    let mut matched = None;
    for retained in inputs.reviews {
        let correspondence = json!({"tool":retained.request.tool,"tool_version":retained.request.tool_version,
            "input":retained.request.input,"context":retained.request.context});
        if correspondence == request.correspondence() {
            if matched.is_some() {
                return Err(invalid(
                    "retained review repeats the exact acquisition request",
                ));
            }
            timestamp(&retained.observed_at)?;
            if !retained.value.is_object() || retained.value["accepted"].as_bool().is_none() {
                return Err(invalid(
                    "retained review value requires an authored boolean accepted decision",
                ));
            }
            matched = Some((retained.value, retained.observed_at));
        }
    }
    matched.ok_or_else(|| invalid("no retained review matches the exact request and context"))
}

/// Import pinned retained artifacts or an independently authored exact review.
/// No network calls, command execution or automatic acceptance occur here.
pub fn observe(tool: &str, request: Value, inputs_path: &Path) -> Result<Value, Error> {
    if serde_json::to_vec(&request)?.len() > MAX_FILE_BYTES {
        return Err(Error::Budget(
            "retained collector request exceeds 1 MiB".into(),
        ));
    }
    let request: Request = serde_json::from_value(request)?;
    request.validate()?;
    if !matches!(tool, "artifact_json" | "review_json") {
        return Err(invalid("unknown retained collector"));
    }
    let bytes = bounded_file(inputs_path)?;
    let (value, observed_at) = match tool {
        "artifact_json" => {
            let base = inputs_path
                .canonicalize()?
                .parent()
                .ok_or_else(|| invalid("retained inputs require a parent directory"))?
                .to_owned();
            artifacts(&request, &bytes, &base)?
        }
        "review_json" => review(&request, &bytes)?,
        _ => unreachable!("tool checked above"),
    };
    let envelope = json!({"value":value,"observed_at":observed_at,"context":request.context,
        "request":request.correspondence(),"details":{"collector":"engarg-retained/1",
        "inputs_sha256":hash(&bytes),"tool":tool}});
    if serde_json::to_vec(&envelope)?.len() > MAX_ARTIFACT_BYTES {
        return Err(Error::Budget(
            "retained observation envelope exceeds 4 MiB".into(),
        ));
    }
    Ok(envelope)
}
