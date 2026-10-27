//! Verify saved canonical history against the exact retained provider exchanges.

use super::{CallRecord, context, parse_assessment, sha256_hex};
use crate::{
    Error,
    domain::{ArgumentCase, Request},
    output_validation::{self, RetainedObjection},
    provider::Role,
};

/// Replaying the original workflow order reconstructs first attachments rather
/// than trusting an origin merely because its local digest is self-consistent.
pub(crate) fn reconstruct_retained(
    transcript: &[CallRecord],
) -> Result<Vec<RetainedObjection>, Error> {
    let mut retained = Vec::new();
    for call in transcript {
        if call.response_content_omitted
            || call.response_content_bytes != call.response.content.len()
            || call.response_content_sha256 != sha256_hex(call.response.content.as_bytes())
            || call.prompt.role != call.role
        {
            return Err(Error::InvalidInput(
                "retained history contains an invalid completion identity".into(),
            ));
        }
        match call.role {
            Role::Constructor | Role::Sceptic | Role::EvidenceAuditor | Role::FinalCritic => {
                let fallback = match call.role {
                    Role::Constructor => "constructor",
                    Role::Sceptic => "sceptic",
                    Role::EvidenceAuditor => "auditor",
                    _ => "critic",
                };
                let prompt = crate::authoring_checkpoint::exact_json(call.prompt.user.as_bytes())?;
                let prefix = if let Some(debate) = prompt.get("debate") {
                    let invalid = || {
                        Error::InvalidInput(
                            "retained debate assessment has invalid participant or round identity"
                                .into(),
                        )
                    };
                    let participant = debate["participant"].as_u64().ok_or_else(invalid)?;
                    let round = debate["round"].as_u64().ok_or_else(invalid)?;
                    let roles = [Role::Constructor, Role::Sceptic, Role::EvidenceAuditor];
                    if call.role != roles[(participant % roles.len() as u64) as usize] {
                        return Err(invalid());
                    }
                    format!("agent-{participant}-round-{round}")
                } else {
                    fallback.to_owned()
                };
                context::assessment(call.role, &prefix, parse_assessment(call)?, &mut retained);
            }
            Role::Single | Role::Synthesiser | Role::Adjudicator => {
                let parsed = eal_collector_contracts::strict_json_exact(
                    call.response.content.as_bytes(),
                    eal_collector_contracts::JsonLimits {
                        max_bytes: call.response_content_bytes.max(1),
                        ..Default::default()
                    },
                )
                .ok()
                .and_then(|value| {
                    serde_json::from_value::<ArgumentCase>(
                        value.get("argument").cloned().unwrap_or(value),
                    )
                    .ok()
                });
                if let Some(case) = parsed {
                    output_validation::retain_objections(&case, call.role, &mut retained);
                } else if call.role == Role::Synthesiser {
                    return Err(Error::InvalidInput(
                        "retained synthesis cannot reconstruct objection origins".into(),
                    ));
                }
            }
        }
    }
    Ok(retained)
}

/// Check a saved source argument and restore only host-owned derived fields.
/// The transcript supplies origin provenance. Local hashes are not signatures.
pub fn reconcile_retained_argument(
    request: &Request,
    candidate: &mut ArgumentCase,
    canonical: &ArgumentCase,
    transcript: &[CallRecord],
) -> Result<(), Error> {
    if canonical.dissent_revisions.is_some() {
        if transcript.is_empty() {
            return Err(Error::InvalidInput(
                "canonical dissent origins require retained provider exchanges".into(),
            ));
        }
        for call in transcript {
            let prompt = eal_collector_contracts::strict_json_exact(
                call.prompt.user.as_bytes(),
                eal_collector_contracts::JsonLimits {
                    max_bytes: call.prompt.user.len().max(1),
                    ..Default::default()
                },
            )
            .map_err(Error::InvalidInput)?;
            let supplied: Request =
                serde_json::from_value(prompt["request"].clone()).map_err(|_| {
                    Error::InvalidInput("retained completion lacks its exact source request".into())
                })?;
            if serde_json::to_value(supplied)? != serde_json::to_value(request)? {
                return Err(Error::InvalidInput(
                    "retained completion request differs from the canonical request".into(),
                ));
            }
        }
        let retained = reconstruct_retained(transcript)?;
        let errors = output_validation::errors(request, canonical, &retained, None);
        if !errors.is_empty() {
            return Err(Error::InvalidInput(errors.join("; ")));
        }
    }
    output_validation::restore_host_fields(candidate, canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Completion, CompletionRequest};
    use serde_json::json;

    #[test]
    fn routed_dissent_replays_exact_participant_and_round_ids() {
        let roles = [Role::Constructor, Role::Sceptic, Role::EvidenceAuditor];
        let mut transcript = Vec::new();
        for round in 0..2 {
            for participant in 0..4 {
                let role = roles[participant % roles.len()];
                let content = json!({"findings":[],"objections":[format!("Concern from participant {participant} in round {round}.")],"evidence_gaps":[]}).to_string();
                transcript.push(CallRecord {
                    role,
                    prompt: CompletionRequest {
                        role,
                        system: String::new(),
                        user: json!({"debate":{"participant":participant,"round":round}})
                            .to_string(),
                        schema: json!({}),
                    },
                    response: Completion {
                        content: content.clone(),
                        model: "fixture".into(),
                        input_tokens: None,
                        output_tokens: None,
                        request_id: None,
                        service_tier: None,
                        reasoning_effort: None,
                    },
                    response_content_bytes: content.len(),
                    response_content_sha256: sha256_hex(content.as_bytes()),
                    response_content_omitted: false,
                    elapsed_ms: 0,
                });
            }
        }
        let retained = reconstruct_retained(&transcript).unwrap();
        assert_eq!(retained.len(), 8);
        for (index, objection) in retained.iter().enumerate() {
            assert_eq!(
                objection.id,
                format!("agent-{}-round-{}-1", index % 4, index / 4)
            );
        }
        transcript[0].role = Role::Sceptic;
        transcript[0].prompt.role = Role::Sceptic;
        assert!(reconstruct_retained(&transcript).is_err());
    }
}
