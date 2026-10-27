//! Admit retained authoring calls under their original or explicit repair budgets.
use super::{Checkpoint, MAX_CALLS};
use crate::{
    Error,
    engine::{CallRecord, RunConfig},
};
use serde::{Deserialize, Serialize};

/// The exact transcript cut and limits granted to one later authoring repair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairAllowance {
    pub recorded_calls: usize,
    pub max_calls: u8,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
}

fn invalid() -> Error {
    Error::InvalidInput("retained authoring calls exceed or contradict their original or explicit repair allowances".into())
}

fn segment(
    calls: &[CallRecord],
    max_calls: usize,
    input: usize,
    response: usize,
) -> Result<(), Error> {
    if input == 0 || response == 0 || calls.len() > max_calls {
        return Err(invalid());
    }
    for call in calls {
        if serde_json::to_vec(&call.prompt)?.len() > input
            || call.response_content_omitted != (call.response_content_bytes > response)
            || (call.response_content_omitted && !call.response.content.is_empty())
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Each slice has one admitted limit. A new stage cannot retroactively broaden
/// earlier calls, including when a previous stage performed no provider calls.
pub(crate) fn validate_call_allowances(
    config: &RunConfig,
    calls: &[CallRecord],
    allowances: &[RepairAllowance],
) -> Result<(), Error> {
    config.validate()?;
    if allowances.len() > MAX_CALLS {
        return Err(invalid());
    }
    let planned = config.required_calls()? as usize;
    let minimum = planned - usize::from(config.max_repairs);
    let first = allowances
        .first()
        .map_or(calls.len(), |entry| entry.recorded_calls);
    if first > calls.len() {
        return Err(invalid());
    }
    segment(
        &calls[..first],
        planned,
        config.max_input_bytes,
        config.max_response_bytes,
    )?;
    let mut previous = first;
    for (index, allowance) in allowances.iter().enumerate() {
        let start = allowance.recorded_calls;
        let end = allowances
            .get(index + 1)
            .map_or(calls.len(), |next| next.recorded_calls);
        if start < minimum
            || start < previous
            || end < start
            || end > calls.len()
            || !(1..=3).contains(&allowance.max_calls)
        {
            return Err(invalid());
        }
        segment(
            &calls[start..end],
            usize::from(allowance.max_calls),
            allowance.max_input_bytes,
            allowance.max_response_bytes,
        )?;
        previous = end;
    }
    Ok(())
}

pub(super) fn validate_source(checkpoint: &Checkpoint) -> Result<(), Error> {
    match (checkpoint.source["kind"].as_str(), checkpoint.repair_allowances.last()) {
        (Some("initial_authoring"), None) => Ok(()),
        (Some("authoring_resume"), Some(last))
            if checkpoint.source["recorded_calls"].as_u64() == u64::try_from(last.recorded_calls).ok()
                && checkpoint.source["additional_repair_limit"].as_u64() == Some(u64::from(last.max_calls))
                && checkpoint.source["stage_byte_limits"]["max_input_bytes"].as_u64() == u64::try_from(last.max_input_bytes).ok()
                && checkpoint.source["stage_byte_limits"]["max_response_bytes"].as_u64() == u64::try_from(last.max_response_bytes).ok() => Ok(()),
        _ => Err(Error::InvalidInput("authoring checkpoint source must retain its explicit repair allowances; regenerate an older resumed checkpoint from its retained calls and budgets".into())),
    }
}
