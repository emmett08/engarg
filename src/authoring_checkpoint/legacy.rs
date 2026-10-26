//! Strict import of completed exchanges from historical authoring run directories.
use super::*;

pub fn import_legacy(directory: &Path) -> Result<Checkpoint, Error> {
    let settings = exact_json(&crate::bounded_file::read(
        &directory.join("run-settings.json"),
        1_048_576,
        "run settings",
    )?)?;
    let bytes = crate::bounded_file::read(
        &directory.join("analysis.audit.jsonl"),
        MAX_BYTES,
        "authoring audit",
    )?;
    let mut start = None;
    let mut calls = vec![];
    let mut failure = None;
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let event = exact_json(line)?;
        match event["event"].as_str() {
            Some("start") if start.is_none() && calls.is_empty() => start = Some(event),
            Some("start") => {
                return Err(Error::InvalidInput(
                    "legacy audit has duplicate or misplaced start".into(),
                ));
            }
            Some("completion") if start.is_some() => {
                if calls.len() >= MAX_CALLS {
                    return Err(Error::Budget("legacy audit call history exhausted".into()));
                }
                calls.push(serde_json::from_value(event["record"].clone())?);
            }
            Some("completion") => {
                return Err(Error::InvalidInput(
                    "legacy completion precedes audit start".into(),
                ));
            }
            Some("failure") => failure = event["error"].as_str().map(str::to_owned),
            Some("http_attempt" | "warning" | "success") => {}
            _ => {
                return Err(Error::InvalidInput(
                    "legacy audit event is unsupported".into(),
                ));
            }
        }
    }
    let start = start.ok_or_else(|| Error::InvalidInput("legacy audit has no start".into()))?;
    for (setting, field) in [
        ("model", "default_model"),
        ("max_output_tokens", "max_output_tokens"),
        ("request_timeout_seconds", "request_timeout_secs"),
    ] {
        let left = settings[setting]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| settings[setting].to_string());
        let right = start[field]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| start[field].to_string());
        if left != right {
            return Err(Error::InvalidInput(format!(
                "legacy run setting {setting} differs from audit"
            )));
        }
    }
    for (setting, field) in [
        ("max_calls", "max_calls"),
        ("max_repairs", "max_repairs"),
        ("max_input_bytes", "max_input_bytes"),
        ("max_response_bytes", "max_response_bytes"),
    ] {
        let setting_value = settings[setting]
            .as_str()
            .and_then(|text| text.parse::<u64>().ok())
            .or_else(|| settings[setting].as_u64());
        if setting_value.is_none() || setting_value != start["config"][field].as_u64() {
            return Err(Error::InvalidInput(format!(
                "legacy run setting {setting} differs from audit config"
            )));
        }
    }
    if settings["argument_depth"] != start["config"]["max_argument_depth"] {
        return Err(Error::InvalidInput(
            "legacy argument depth differs from audit config".into(),
        ));
    }
    let request: Request = serde_json::from_value(start["request"].clone())?;
    let request_snapshot = exact_json(&crate::bounded_file::read(
        &directory.join("request.json"),
        1_048_576,
        "request snapshot",
    )?)?;
    if request_snapshot != serde_json::to_value(&request)? {
        return Err(Error::InvalidInput(
            "legacy request snapshot differs from audit".into(),
        ));
    }
    let evidence = crate::bounded_file::read(
        &directory.join("evidence-config.json"),
        1_048_576,
        "evidence configuration",
    )?;
    let evidence_digest = digest(&evidence);
    if settings["evidence_config_sha256"].as_str() != Some(evidence_digest.as_str()) {
        return Err(Error::InvalidInput(
            "legacy evidence configuration digest mismatch".into(),
        ));
    }
    let mut checkpoint = Checkpoint::new(
        request,
        serde_json::from_value(start["config"].clone())?,
        start.clone(),
        evidence_digest,
    );
    checkpoint.producer_version = start["version"]
        .as_str()
        .ok_or_else(|| Error::InvalidInput("legacy audit has no producer version".into()))?
        .into();
    checkpoint.protocol_identity = "legacy-unrecorded".into();
    checkpoint.source = json!({"kind":"legacy_audit","audit_sha256":digest(&bytes),"git_commit":settings["git_commit"],
        "engarg_binary_sha256":settings["engarg_binary_sha256"]});
    checkpoint.transcript = calls;
    checkpoint.failure = failure;
    checkpoint.validate()?;
    Ok(checkpoint)
}
