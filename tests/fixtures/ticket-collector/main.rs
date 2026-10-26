use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    collections::BTreeSet,
    io::{Read, Write},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    evidence_id: String,
    environment: String,
    tool: String,
    tool_version: String,
    input: Box<RawValue>,
    context: Box<RawValue>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    snapshot: String,
    items: Vec<Item>,
    complete: bool,
    formal_input: Box<RawValue>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    id: String,
}
#[derive(Serialize)]
struct Value<'a> {
    collected: bool,
    snapshot: &'a str,
    count: usize,
    distinct: bool,
    complete: bool,
    typed_input: &'a RawValue,
}
#[derive(Serialize)]
struct Echo<'a> {
    tool: &'a str,
    tool_version: &'a str,
    input: &'a RawValue,
    context: &'a RawValue,
}
#[derive(Serialize)]
struct Response<'a> {
    value: Value<'a>,
    observed_at: String,
    context: &'a RawValue,
    request: Echo<'a>,
}

fn now() -> Result<String, Box<dyn std::error::Error>> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let days = i64::try_from(seconds / 86400)? + 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    let remainder = seconds % 86400;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        remainder / 3600,
        remainder % 3600 / 60,
        remainder % 60
    ))
}
fn collect() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::io::stdin().take(1048577).read_to_end(&mut bytes)?;
    if bytes.len() > 1048576 {
        return Err("request limit".into());
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ]
    .iter()
    .any(|v| v.is_empty())
    {
        return Err("identity".into());
    }
    let input: Input = serde_json::from_str(request.input.get())?;
    let formal: serde_json::Value = serde_json::from_str(input.formal_input.get())?;
    let expected: serde_json::Value = serde_json::from_str(
        r#"{"schema":"EAL/typed-input/1","method":"deductive/1","subject":"catalogue_snapshot","quantity":"proposition","unit":"1","scope":"engarg.scope:e4573ab176d50fdfebebee9913448452898c9ad953abd8b22ae3ba2e9efb02a8","valid_from":"2000-01-01T00:00:00Z","valid_until":"2100-01-01T00:00:00Z","payload":{"premises":["count_is_three","identifiers_distinct"],"conclusion":{"and":["count_is_three","identifiers_distinct"]}}}"#,
    )?;
    if formal != expected || !request.context.get().trim_start().starts_with('{') {
        return Err("formal input or context shape".into());
    }
    if input.snapshot.is_empty()
        || input.snapshot.chars().count() > 128
        || input.items.len() > 32
        || input
            .items
            .iter()
            .any(|item| item.id.is_empty() || item.id.chars().count() > 64)
    {
        return Err("input bounds".into());
    }
    let distinct = input
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == input.items.len();
    let response = Response {
        value: Value {
            collected: true,
            snapshot: &input.snapshot,
            count: input.items.len(),
            distinct,
            complete: input.complete,
            typed_input: &input.formal_input,
        },
        observed_at: now()?,
        context: &request.context,
        request: Echo {
            tool: &request.tool,
            tool_version: &request.tool_version,
            input: &request.input,
            context: &request.context,
        },
    };
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
fn main() {
    if collect().is_err() {
        std::process::exit(2);
    }
}
