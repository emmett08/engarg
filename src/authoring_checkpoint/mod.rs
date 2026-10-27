//! Bounded, durable authoring checkpoints for the current protocol.
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
        include_str!("../engine/repair_scope.rs"),
        include_str!("../engine/retained.rs"),
        include_str!("../engine/support.rs"),
        include_str!("../domain/records.rs"),
        include_str!("../support_requirements/mod.rs"),
        include_str!("../engine/support/authoring.rs"),
        include_str!("../domain/reasoning.rs"),
        include_str!("../direct_planning.rs"),
        include_str!("../support_requirements/validation.rs"),
        include_str!("mod.rs"),
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
        self.validate_transcript()
    }
    /// Validate the authoring protocol independently of checkpoint storage limits.
    /// Current provider reports can contain a larger admitted debate than the
    /// separately bounded resumable-checkpoint history.
    pub(crate) fn validate_transcript(&self) -> Result<(), Error> {
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
        self.config.required_calls()?;
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
    /// Specialists complete concurrently within each round. Original synthesis
    /// consumes participant order in every configured round, regardless of audit
    /// arrival order. Later dependent stages keep their sequential positions.
    pub fn ordered_transcript(&self) -> Result<Vec<CallRecord>, Error> {
        Ok(self
            .transcript_order()?
            .into_iter()
            .map(|index| self.transcript[index].clone())
            .collect())
    }
    fn transcript_order(&self) -> Result<Vec<usize>, Error> {
        self.config.required_calls()?;
        let calls = &self.transcript;
        let mut order: Vec<_> = (0..calls.len()).collect();
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
                let (participants, specialist_calls) = if let Some(debate) = &self.config.debate {
                    (
                        usize::from(debate.participants),
                        debate.specialist_calls()? as usize,
                    )
                } else {
                    (3, 3)
                };
                for start in (0..calls.len().min(specialist_calls)).step_by(participants) {
                    let end = (start + participants).min(calls.len());
                    let round = start / participants;
                    let mut seen = std::collections::BTreeSet::new();
                    let mut members = Vec::new();
                    for (index, call) in calls.iter().enumerate().take(end).skip(start) {
                        let prompt = exact_json(call.prompt.user.as_bytes())?;
                        let participant = if self.config.debate.is_some() {
                            let participant = prompt["debate"]["participant"]
                                .as_u64()
                                .and_then(|value| usize::try_from(value).ok())
                                .filter(|participant| *participant < participants)
                                .ok_or_else(|| {
                                    Error::InvalidInput(
                                        "checkpoint debate participant is invalid".into(),
                                    )
                                })?;
                            if prompt["debate"]["round"].as_u64() != Some(round as u64)
                                || call.role != specialists[participant % specialists.len()]
                            {
                                return Err(Error::InvalidInput(
                                    "checkpoint debate round or participant role is invalid".into(),
                                ));
                            }
                            participant
                        } else {
                            if prompt.get("debate").is_some() {
                                return Err(Error::InvalidInput(
                                    "checkpoint debate metadata requires its configured topology"
                                        .into(),
                                ));
                            }
                            specialists
                                .iter()
                                .position(|role| *role == call.role)
                                .ok_or_else(|| {
                                    Error::InvalidInput(
                                        "checkpoint authoring stage order is invalid".into(),
                                    )
                                })?
                        };
                        if !seen.insert(participant) {
                            return Err(Error::InvalidInput(
                                "checkpoint has duplicate specialist participants in a round"
                                    .into(),
                            ));
                        }
                        members.push((participant, index));
                    }
                    members.sort_by_key(|(participant, _)| *participant);
                    for (destination, (_, source)) in order[start..end].iter_mut().zip(members) {
                        *destination = source;
                    }
                }
                for (index, call) in calls.iter().enumerate().skip(specialist_calls) {
                    let expected = match index - specialist_calls {
                        0 => Synthesiser,
                        1 => FinalCritic,
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
        Ok(order)
    }
    /// Keep the exact original arrival order through indices into the report's
    /// ordered prefix, rather than duplicating every recorded exchange.
    pub fn compact_parent(&self) -> Result<(Self, Vec<usize>), Error> {
        let sorted = self.transcript_order()?;
        let mut order = vec![0; sorted.len()];
        for (position, original) in sorted.into_iter().enumerate() {
            order[original] = position;
        }
        let mut metadata = self.clone();
        metadata.transcript.clear();
        Ok((metadata, order))
    }
    pub fn inspection(&self) -> Value {
        let minimum = self
            .config
            .required_calls()
            .ok()
            .map(|calls| (calls - u32::from(self.config.max_repairs)) as usize);
        let complete = minimum.is_some_and(|minimum| self.transcript.len() >= minimum)
            && self.transcript_order().is_ok()
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

mod storage;
#[cfg(test)]
mod tests;
pub use storage::{Store, output_identity, private_new};

/// Check resumed call accounting and immutable source history at report admission.
/// This verifies consistency of retained local records rather than their authenticity.
pub fn validate_recovery_report(raw: &Value) -> Result<(), Error> {
    if raw
        .get("argument_revisions")
        .and_then(Value::as_array)
        .is_some_and(|revisions| !revisions.is_empty())
    {
        let report: crate::engine::RunReport = serde_json::from_value(raw.clone())?;
        let original = crate::engine::support::revision::origin(&report)?;
        return validate_recovery_report(&serde_json::to_value(original)?);
    }
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
        &combined.config,
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
            != *raw
                .get("support_elaboration")
                .unwrap_or(&raw["evidence_planning"]["elaboration"])
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
