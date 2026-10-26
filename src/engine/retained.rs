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
                let prefix = match call.role {
                    Role::Constructor => "constructor",
                    Role::Sceptic => "sceptic",
                    Role::EvidenceAuditor => "auditor",
                    _ => "critic",
                };
                context::assessment(call.role, prefix, parse_assessment(call)?, &mut retained);
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
