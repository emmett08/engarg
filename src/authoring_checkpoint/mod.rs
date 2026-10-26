//! Bounded, durable authoring checkpoints and admission of legacy run audits.
use crate::{
    Error,
    domain::Request,
    engine::{CallRecord, Mode, RunConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const SCHEMA: &str = "engarg.authoring-checkpoint/1";
pub const MAX_BYTES: usize = 128 * 1_048_576;
pub const MAX_CALLS: usize = 32;
pub const MAX_REPORT_BYTES: usize = 16 * 1_048_576;

pub fn digest(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes.as_ref())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn protocol_identity() -> String {
    digest(concat!(
        include_str!("../prompts.rs"),
        include_str!("../output_validation.rs"),
        include_str!("../output_validation/target_context.rs"),
        include_str!("../output_validation/revisions.rs"),
        include_str!("../domain/dissent.rs"),
        include_str!("../engine/retained.rs"),
        include_str!("../engine/support.rs"),
        include_str!("../domain/records.rs"),
        include_str!("../support_requirements/mod.rs"),
        include_str!("../engine/support/authoring.rs"),
        include_str!("../domain/reasoning.rs"),
        include_str!("../support_requirements/validation.rs"),
        include_str!("mod.rs"),
        include_str!("legacy.rs"),
        include_str!("storage.rs"),
        include_str!("../authoring_commands.rs")
    ))
}

pub fn exact_json(bytes: &[u8]) -> Result<Value, Error> {
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = eal_collector_contracts::deserialize_value_exact(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema: String,
    pub producer_version: String,
    pub protocol_identity: String,
    pub request: Request,
    pub config: RunConfig,
    pub provider_config: Value,
    pub evidence_config_sha256: String,
    pub source: Value,
    pub transcript: Vec<CallRecord>,
    pub failure: Option<String>,
}

impl Checkpoint {
    pub fn new(
        request: Request,
        config: RunConfig,
        provider_config: Value,
        evidence_config_sha256: String,
    ) -> Self {
        Self {
            schema: SCHEMA.into(),
            producer_version: env!("CARGO_PKG_VERSION").into(),
            protocol_identity: protocol_identity(),
            request,
            config,
            provider_config,
            evidence_config_sha256,
            source: json!({"kind":"initial_authoring"}),
            transcript: vec![],
            failure: None,
        }
    }
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = crate::bounded_file::read(path, MAX_BYTES, "authoring checkpoint")?;
        let checkpoint: Self = serde_json::from_value(exact_json(&bytes)?)?;
        checkpoint.validate()?;
        Ok(checkpoint)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA
            || self.transcript.len() > MAX_CALLS
            || self.config.collector_catalogue.is_some()
        {
            return Err(Error::InvalidInput(
                "unsupported or oversized graph authoring checkpoint".into(),
            ));
        }
        if self.config.max_input_bytes == 0
            || self.config.max_response_bytes == 0
            || self.config.max_repairs > 3
            || self
                .config
                .max_argument_depth
                .is_some_and(|depth| !(1..=16).contains(&depth))
        {
            return Err(Error::InvalidInput(
                "checkpoint has invalid resource or argument-depth bounds".into(),
            ));
        }
        crate::domain::validate_request(&self.request)
            .map_err(|e| Error::InvalidInput(e.join("; ")))?;
        let expected_request = serde_json::to_value(&self.request)?;
        for call in &self.transcript {
            if call.role != call.prompt.role
                || (!call.response_content_omitted
                    && (call.response_content_bytes != call.response.content.len()
                        || call.response_content_sha256
                            != digest(call.response.content.as_bytes())))
            {
                return Err(Error::InvalidInput(
                    "checkpoint completion identity or digest mismatch".into(),
                ));
            }
            let prompt = exact_json(call.prompt.user.as_bytes())?;
            if prompt.get("request") != Some(&expected_request) {
                return Err(Error::InvalidInput(
                    "checkpoint prompt request differs from its declared request".into(),
                ));
            }
        }
        self.ordered_transcript()?;
        Ok(())
    }
    /// Independent specialists complete concurrently. Original synthesis always
    /// consumes them in this fixed order, regardless of audit arrival order.
    pub fn ordered_transcript(&self) -> Result<Vec<CallRecord>, Error> {
        let mut calls = self.transcript.clone();
        match self.config.mode {
            Mode::Single => {
                if calls
                    .iter()
                    .any(|call| call.role != crate::provider::Role::Single)
                {
                    return Err(Error::InvalidInput(
                        "single checkpoint contains a different role".into(),
                    ));
                }
            }
            Mode::Hybrid => {
                use crate::provider::Role::*;
                let specialists = [Constructor, Sceptic, EvidenceAuditor];
                let first_count = calls.len().min(3);
                for (position, call) in calls.iter().take(first_count).enumerate() {
                    if !specialists.contains(&call.role)
                        || calls[..position]
                            .iter()
                            .any(|previous| previous.role == call.role)
                    {
                        return Err(Error::InvalidInput(
                            "checkpoint has missing, duplicate or misordered specialist roles"
                                .into(),
                        ));
                    }
                }
                if first_count == 3 {
                    calls[..3].sort_by_key(|call| {
                        specialists
                            .iter()
                            .position(|role| *role == call.role)
                            .unwrap()
                    });
                }
                for (index, call) in calls.iter().enumerate().skip(3) {
                    let expected = match index {
                        3 => Synthesiser,
                        4 => FinalCritic,
                        _ => Adjudicator,
                    };
                    if call.role != expected {
                        return Err(Error::InvalidInput(
                            "checkpoint authoring stage order is invalid".into(),
                        ));
                    }
                }
            }
        }
        Ok(calls)
    }
    /// Keep the exact original arrival order through indices into the report's
    /// ordered prefix, rather than duplicating every recorded exchange.
    pub fn compact_parent(&self) -> Result<(Self, Vec<usize>), Error> {
        let ordered = self.ordered_transcript()?;
        let mut order: Vec<usize> = (0..self.transcript.len()).collect();
        if self.config.mode == Mode::Hybrid {
            for (index, call) in self.transcript.iter().take(3).enumerate() {
                order[index] = ordered
                    .iter()
                    .position(|item| item.role == call.role)
                    .ok_or_else(|| {
                        Error::InvalidInput("checkpoint specialist order is inconsistent".into())
                    })?;
            }
        }
        let mut metadata = self.clone();
        metadata.transcript.clear();
        Ok((metadata, order))
    }
    pub fn inspection(&self) -> Value {
        let minimum = if self.config.mode == Mode::Hybrid {
            6
        } else {
            1
        };
        let complete = self.transcript.len() >= minimum
            && self
                .transcript
                .iter()
                .all(|call| !call.response_content_omitted);
        json!({"schema":"engarg.authoring-inspection/1","recorded_calls":self.transcript.len(),
            "resumable":complete,"stage":if complete {"final_validation"} else {"incomplete_authoring"},
            "producer_version":self.producer_version,"protocol_migration_required":self.protocol_identity != protocol_identity(),
            "failure":self.failure,"source":self.source,
            "limitations":["Digests check local consistency and do not authenticate historical provenance.",
                "Only completed authoring stages can resume. Interrupted requests cannot be reconstructed."]})
    }
}

mod legacy;
mod storage;
#[cfg(test)]
mod tests;
pub use legacy::import_legacy;
pub use storage::{Store, output_identity, private_new};

/// Check resumed call accounting and immutable source history at report admission.
/// This verifies consistency of retained local records rather than their authenticity.
pub fn validate_recovery_report(raw: &Value) -> Result<(), Error> {
    // A later support-only recovery appends calls while retaining this extension.
    // Validate authoring at its own report cut, then let the support validator
    // check subsequent changes against its separately retained source report.
    let mut raw = raw;
    for depth in 0..=3 {
        let Some(recovery) = raw.get("support_recovery").filter(|value| !value.is_null()) else {
            break;
        };
        if depth == 3 {
            return Err(Error::InvalidInput(
                "support recovery history exceeds three revisions".into(),
            ));
        }
        let previous = recovery
            .get("previous_report")
            .ok_or_else(|| Error::InvalidInput("support recovery has no source report".into()))?;
        if raw.get("authoring_recovery") != previous.get("authoring_recovery") {
            return Err(Error::InvalidInput(
                "support recovery changed authoring recovery history".into(),
            ));
        }
        raw = previous;
    }
    let Some(record) = raw
        .get("authoring_recovery")
        .filter(|value| !value.is_null())
    else {
        return Ok(());
    };
    let invalid = || {
        Error::InvalidInput(
            "authoring recovery provenance or call accounting differs from retained checkpoint"
                .into(),
        )
    };
    if record["schema"] != "engarg.authoring-recovery/1" {
        return Err(invalid());
    }
    let count = usize::try_from(record["recorded_calls"].as_u64().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let new_calls = usize::try_from(record["new_model_calls"].as_u64().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let allowance = record["additional_repair_limit"]
        .as_u64()
        .ok_or_else(invalid)?;
    if !(1..=3).contains(&allowance) || new_calls > allowance as usize || count > MAX_CALLS {
        return Err(invalid());
    }
    let transcript: Vec<CallRecord> = serde_json::from_value(raw["transcript"].clone())?;
    if transcript.len() != count.saturating_add(new_calls) || transcript.len() > MAX_CALLS {
        return Err(invalid());
    }
    let mut previous: Checkpoint = serde_json::from_value(record["previous_checkpoint"].clone())?;
    if !previous.transcript.is_empty() {
        return Err(invalid());
    }
    let order: Vec<usize> = serde_json::from_value(record["previous_call_order"].clone())?;
    let unique: std::collections::BTreeSet<_> = order.iter().copied().collect();
    if order.len() != count || unique.len() != count || order.iter().any(|index| *index >= count) {
        return Err(invalid());
    }
    previous.transcript = order
        .iter()
        .map(|index| transcript[*index].clone())
        .collect();
    previous.validate()?;
    if record["parent_checkpoint_sha256"] != digest(serde_json::to_vec(&previous)?)
        || record["previous_protocol"] != previous.protocol_identity
        || raw["request"] != serde_json::to_value(&previous.request)?
        || raw["run_config"] != serde_json::to_value(&previous.config)?
    {
        return Err(invalid());
    }
    let max_input = record["stage_byte_limits"]["max_input_bytes"]
        .as_u64()
        .filter(|limit| *limit > 0)
        .ok_or_else(invalid)?;
    let max_response = record["stage_byte_limits"]["max_response_bytes"]
        .as_u64()
        .filter(|limit| *limit > 0)
        .ok_or_else(invalid)?;
    if transcript.iter().any(|call| call.response_content_omitted) {
        return Err(invalid());
    }
    for call in &transcript[count..] {
        if serde_json::to_vec(&call.prompt)?.len() as u64 > max_input
            || call.response_content_bytes as u64 > max_response
        {
            return Err(invalid());
        }
    }
    let prefix = previous.ordered_transcript()?;
    if serde_json::to_value(&transcript[..count])? != serde_json::to_value(prefix)? {
        return Err(invalid());
    }
    if record["protocol_migration"]
        != json!(record["previous_protocol"] != record["current_protocol"])
        || record["current_protocol"]
            .as_str()
            .is_none_or(str::is_empty)
    {
        return Err(invalid());
    }
    let mut combined = previous;
    combined.transcript = transcript;
    combined.validate()?;
    let argument: crate::domain::ArgumentCase = serde_json::from_value(raw["argument"].clone())?;
    let history = crate::engine::support::authoring::replay_history(
        &combined.ordered_transcript()?,
        combined.config.mode,
    )?;
    let errors = crate::output_validation::errors(
        &combined.request,
        &argument,
        &history.retained,
        combined.config.max_argument_depth,
    );
    if !errors.is_empty() {
        return Err(Error::InvalidInput(errors.join("; ")));
    }
    let last = combined.transcript.last().ok_or_else(invalid)?;
    let mut final_candidate =
        crate::support_requirements::parse_supported_argument(&last.response.content, MAX_BYTES)?;
    crate::output_validation::restore_host_fields(&mut final_candidate.argument, &argument)?;
    if serde_json::to_value(final_candidate.argument)? != raw["argument"]
        || serde_json::to_value(final_candidate.support_elaboration)?
            != raw["evidence_planning"]["elaboration"]
    {
        return Err(invalid());
    }
    Ok(())
}

/// Resumed reports must fit the existing EAL consumer admission cap. Larger
/// histories remain available through their separate durable checkpoint.
pub fn admitted_report_bytes(value: &Value) -> Result<Vec<u8>, Error> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len().saturating_add(1) > MAX_REPORT_BYTES {
        return Err(Error::Budget("resumed report exceeds the existing 16 MiB consumer limit; completed exchanges remain in the audit and authoring checkpoint".into()));
    }
    bytes.push(b'\n');
    Ok(bytes)
}
