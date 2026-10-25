use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::io::{Read, Write};

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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: Mode,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Positive,
    Negative,
    Error,
}

#[derive(Serialize)]
struct Measurement {
    collected: bool,
    present: bool,
    mode: Mode,
}

#[derive(Serialize)]
struct Echo<'a> {
    tool: &'a str,
    tool_version: &'a str,
    input: &'a RawValue,
    context: &'a RawValue,
}

#[derive(Serialize)]
struct Envelope<'a> {
    value: Measurement,
    observed_at: String,
    context: &'a RawValue,
    request: Echo<'a>,
}

// Format the invocation's measurement clock in UTC with the Rust standard
// library only; the pinned SDK needs no date executable or extra time crate.
pub fn observation_time(
    clock: std::time::SystemTime,
) -> Result<String, Box<dyn std::error::Error>> {
    let seconds = clock.duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let days = i64::try_from(seconds / 86_400)? + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    if year > 9999 {
        return Err("clock outside RFC3339 four-digit years".into());
    }
    let remainder = seconds % 86_400;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        remainder / 3600,
        (remainder % 3600) / 60,
        remainder % 60
    ))
}

fn collect() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = Vec::new();
    std::io::stdin()
        .lock()
        .take(1_048_577)
        .read_to_end(&mut raw)?;
    if raw.len() > 1_048_576 {
        return Err("request limit".into());
    }
    let request: Request = serde_json::from_slice(&raw)?;
    if [
        &request.evidence_id,
        &request.environment,
        &request.tool,
        &request.tool_version,
    ]
    .iter()
    .any(|identity| identity.is_empty())
    {
        return Err("request identity".into());
    }
    if !request.context.get().trim_start().starts_with('{') {
        return Err("context object".into());
    }
    let input: Input = serde_json::from_str(request.input.get())?;
    let present = match input.mode {
        Mode::Positive => true,
        Mode::Negative => false,
        Mode::Error => return Err("synthetic acquisition error".into()),
    };
    // Stream raw metadata through typed structs. A Value/json! intermediate can
    // confuse literal reserved-key objects with serde's private number markers.
    let envelope = Envelope {
        value: Measurement {
            collected: true,
            present,
            mode: input.mode,
        },
        observed_at: observation_time(std::time::SystemTime::now())?,
        context: &request.context,
        request: Echo {
            tool: &request.tool,
            tool_version: &request.tool_version,
            input: &request.input,
            context: &request.context,
        },
    };
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &envelope)?;
    output.write_all(b"\n")?;
    Ok(())
}

fn main() {
    if collect().is_err() {
        std::process::exit(2);
    }
}
