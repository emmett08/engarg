//! Retain removed source designs for repair without admitting them to execution.
use super::{
    AcquisitionDisposition, SupportElaboration, parse_supported_argument, validate_elaboration,
};
use crate::{
    Error,
    engine::{CallRecord, sha256_hex as digest},
    provider::Role,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ACQUISITION_DESIGN_REVIEW_SCHEMA: &str = "engarg.acquisition-design-review/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionDesignStatus {
    /// No model stage proposed acquisitions. This can be valid for conceptual work.
    NoAcquisitionDesign,
    Represented,
    PendingRepair,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionDesignReview {
    pub schema: String,
    pub status: AcquisitionDesignStatus,
    pub active_acquisitions: usize,
    pub active_derivations: usize,
    /// Historical proposals only. Resolvers and exporters must use `elaboration`.
    pub pending_designs: Vec<PendingAcquisitionDesign>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingAcquisitionDesign {
    /// One-based index in the retained transcript, never a new provider call.
    pub source_call: usize,
    pub source_role: Role,
    pub response_content_sha256: String,
    pub elaboration_sha256: String,
    pub missing_acquisition_ids: Vec<String>,
    pub missing_derivation_ids: Vec<String>,
    pub changed_step_ids: Vec<String>,
    pub step_sha256: BTreeMap<String, String>,
    pub disposition: String,
    pub validation_errors: Vec<String>,
    /// Exact typed historical proposal. It may fail validation and is not executable.
    pub elaboration: SupportElaboration,
}

pub fn step_identity(value: &impl Serialize) -> Result<String, Error> {
    eal_collector_contracts::capability::canonical_identity(value).map_err(Error::InvalidInput)
}

/// Compare all retained source proposals with the accepted elaboration. A missing
/// step is preserved for explicit review even when the accepted graph validates.
/// This does not require every engineering argument to contain acquisitions.
pub fn review_acquisition_design(
    current: &SupportElaboration,
    transcript: &[CallRecord],
    maximum_response_bytes: usize,
) -> Result<AcquisitionDesignReview, Error> {
    let acquisitions = current
        .acquisitions
        .iter()
        .map(|step| step.id.as_str())
        .collect::<BTreeSet<_>>();
    let derivations = current
        .derivations
        .iter()
        .map(|step| step.id.as_str())
        .collect::<BTreeSet<_>>();
    let current_steps = current
        .acquisitions
        .iter()
        .map(|step| Ok((step.id.clone(), step_identity(step)?)))
        .chain(
            current
                .derivations
                .iter()
                .map(|step| Ok((step.id.clone(), step_identity(step)?))),
        )
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let current_identity = digest(&serde_json::to_vec(current)?);
    let accepted_call = transcript.iter().rposition(|record| {
        !record.response_content_omitted
            && parse_supported_argument(&record.response.content, maximum_response_bytes)
                .ok()
                .and_then(|candidate| serde_json::to_vec(&candidate.support_elaboration).ok())
                .is_some_and(|bytes| digest(&bytes) == current_identity)
    });
    let mut revisions = BTreeMap::new();
    for revision in &current.acquisition_revisions {
        if revisions
            .insert(
                (
                    revision.previous_step_id.clone(),
                    revision.previous_step_sha256.clone(),
                ),
                revision,
            )
            .is_some()
        {
            return Err(Error::InvalidInput(
                "duplicate acquisition revision anchor".into(),
            ));
        }
    }
    let mut matched_revisions = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending_designs = Vec::new();
    for (index, record) in transcript.iter().enumerate() {
        if record.response_content_omitted
            || !matches!(
                record.role,
                Role::Single | Role::Synthesiser | Role::Adjudicator
            )
        {
            continue;
        }
        if record.response.content.len() != record.response_content_bytes
            || digest(record.response.content.as_bytes()) != record.response_content_sha256
        {
            return Err(Error::InvalidInput(format!(
                "retained call {} response content differs from its recorded byte count or SHA-256 identity",
                index + 1
            )));
        }
        if accepted_call.is_some_and(|accepted| index >= accepted) {
            // A completion cannot authenticate its own claimed prior design identity.
            continue;
        }
        let Ok(prior) = parse_supported_argument(&record.response.content, maximum_response_bytes)
        else {
            // An unparseable completion remains retained in the original transcript.
            continue;
        };
        let prior_steps = prior
            .support_elaboration
            .acquisitions
            .iter()
            .map(|step| Ok((step.id.clone(), step_identity(step)?)))
            .chain(
                prior
                    .support_elaboration
                    .derivations
                    .iter()
                    .map(|step| Ok((step.id.clone(), step_identity(step)?))),
            )
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let mut unresolved = BTreeSet::new();
        for (id, identity) in &prior_steps {
            let anchor = (id.clone(), identity.clone());
            let changed = current_steps.get(id) != Some(identity);
            if let Some(revision) = revisions.get(&anchor) {
                matched_revisions.insert(anchor);
                match revision.disposition {
                    AcquisitionDisposition::Repaired => {
                        if revision.replacement_ids != [id.clone()]
                            || !current_steps.contains_key(id)
                        {
                            return Err(Error::InvalidInput(format!(
                                "acquisition revision {id} repaired disposition must name the same active step"
                            )));
                        }
                    }
                    AcquisitionDisposition::Replaced => {
                        if revision.replacement_ids.is_empty()
                            || revision.replacement_ids.iter().any(|replacement| {
                                replacement == id || !current_steps.contains_key(replacement)
                            })
                        {
                            return Err(Error::InvalidInput(format!(
                                "acquisition revision {id} replaced disposition must name different active replacement steps"
                            )));
                        }
                    }
                    AcquisitionDisposition::Deferred => {
                        unresolved.insert(id.clone());
                    }
                    AcquisitionDisposition::Retired => {
                        if current_steps.contains_key(id) {
                            return Err(Error::InvalidInput(format!(
                                "acquisition revision {id} retired disposition cannot leave that step active"
                            )));
                        }
                    }
                }
            } else if changed {
                unresolved.insert(id.clone());
            }
        }
        if unresolved.is_empty() {
            continue;
        }
        let elaboration_sha256 = digest(&serde_json::to_vec(&prior.support_elaboration)?);
        if !seen.insert(elaboration_sha256.clone()) {
            continue;
        }
        let missing_acquisition_ids = prior
            .support_elaboration
            .acquisitions
            .iter()
            .filter(|step| {
                unresolved.contains(&step.id) && !acquisitions.contains(step.id.as_str())
            })
            .map(|step| step.id.clone())
            .collect();
        let missing_derivation_ids = prior
            .support_elaboration
            .derivations
            .iter()
            .filter(|step| unresolved.contains(&step.id) && !derivations.contains(step.id.as_str()))
            .map(|step| step.id.clone())
            .collect();
        let changed_step_ids = unresolved
            .iter()
            .filter(|id| current_steps.contains_key(id.as_str()))
            .cloned()
            .collect();
        pending_designs.push(PendingAcquisitionDesign {
            source_call: index + 1,
            source_role: record.role,
            response_content_sha256: digest(record.response.content.as_bytes()),
            elaboration_sha256,
            missing_acquisition_ids,
            missing_derivation_ids,
            changed_step_ids,
            step_sha256: prior_steps
                .into_iter()
                .filter(|(id, _)| unresolved.contains(id))
                .collect(),
            disposition: "unresolved_revision".into(),
            validation_errors: validate_elaboration(&prior.argument, &prior.support_elaboration),
            elaboration: prior.support_elaboration,
        });
    }
    if matched_revisions.len() != revisions.len() {
        return Err(Error::InvalidInput(
            "acquisition revision does not name an exact retained prior step identity".into(),
        ));
    }
    let status = if !pending_designs.is_empty() {
        AcquisitionDesignStatus::PendingRepair
    } else if acquisitions.is_empty() && derivations.is_empty() {
        AcquisitionDesignStatus::NoAcquisitionDesign
    } else {
        AcquisitionDesignStatus::Represented
    };
    Ok(AcquisitionDesignReview {
        schema: ACQUISITION_DESIGN_REVIEW_SCHEMA.into(),
        status,
        active_acquisitions: acquisitions.len(),
        active_derivations: derivations.len(),
        pending_designs,
    })
}

impl AcquisitionDesignReview {
    pub fn warning(&self) -> Option<String> {
        if self.status != AcquisitionDesignStatus::PendingRepair {
            return None;
        }
        let acquisitions = self
            .pending_designs
            .iter()
            .flat_map(|design| &design.missing_acquisition_ids)
            .collect::<BTreeSet<_>>()
            .len();
        let derivations = self
            .pending_designs
            .iter()
            .flat_map(|design| &design.missing_derivation_ids)
            .collect::<BTreeSet<_>>()
            .len();
        let changed = self
            .pending_designs
            .iter()
            .flat_map(|design| &design.changed_step_ids)
            .collect::<BTreeSet<_>>()
            .len();
        Some(format!(
            "Acquisition planning remains incomplete: {acquisitions} earlier acquisition designs and {derivations} derivations are absent from the accepted graph. {changed} retained step identities changed without a recorded disposition. Historical designs and diagnostics are retained under evidence_planning.design_review.pending_designs for repair or explicit retirement review. They have not been admitted to execution."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        provider::{Completion, CompletionRequest},
        support_requirements::tests::{case, contract, pending},
    };
    use serde_json::json;

    pub(crate) fn proposed_sources() -> SupportElaboration {
        let argument = case();
        let mut elaboration = pending(&argument);
        let mut source_contract = contract(
            "source_retrieval",
            "existential_witness",
            "fixture.read/1",
            "1",
        );
        source_contract.schema = "engarg.support-contract/1".into();
        source_contract.acceptable_methods.clear();
        elaboration.acquisitions = (1..=5)
            .map(|index| {
                serde_json::from_value(json!({
                    "id":format!("source-{index}"), "contract":source_contract,
                    "arguments":{},"bindings":[],"for_each":null,"operation_spec":null,
                }))
                .unwrap()
            })
            .collect();
        elaboration.derivations = (1..=2)
            .map(|index| {
                serde_json::from_value(json!({
                    "id":format!("projection-{index}"),
                    "inputs":[{"step_id":format!("source-{index}"),"pointer":""}],
                    "operation":{"kind":"project","fields":{"result":"/result"}},
                }))
                .unwrap()
            })
            .collect();
        elaboration
    }

    fn record(elaboration: &SupportElaboration) -> CallRecord {
        let content = json!({"argument":case(),"support_elaboration_json":serde_json::to_string(elaboration).unwrap()}).to_string();
        CallRecord {
            role: Role::Synthesiser,
            prompt: CompletionRequest {
                role: Role::Synthesiser,
                system: String::new(),
                user: String::new(),
                schema: json!({}),
            },
            response: Completion {
                content: content.clone(),
                model: "synthetic-fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            },
            response_content_bytes: content.len(),
            response_content_sha256: digest(content.as_bytes()),
            response_content_omitted: false,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn removed_five_sources_and_two_derivations_remain_non_executable_repair_inputs() {
        let proposed = proposed_sources();
        let accepted = pending(&case());
        let source = record(&proposed);
        let review =
            review_acquisition_design(&accepted, &[source.clone(), source.clone()], 1_048_576)
                .unwrap();
        assert_eq!(review.status, AcquisitionDesignStatus::PendingRepair);
        assert_eq!(
            (review.active_acquisitions, review.active_derivations),
            (0, 0)
        );
        assert_eq!(
            review.pending_designs.len(),
            1,
            "duplicate proposals do not duplicate repair work"
        );
        let retained = &review.pending_designs[0];
        assert_eq!(retained.source_call, 1);
        assert_eq!(
            retained.response_content_sha256,
            source.response_content_sha256
        );
        assert_eq!(retained.missing_acquisition_ids.len(), 5);
        assert_eq!(retained.missing_derivation_ids.len(), 2);
        assert!(!retained.validation_errors.is_empty());
        assert_eq!(
            serde_json::to_value(&retained.elaboration).unwrap(),
            serde_json::to_value(&proposed).unwrap()
        );
        assert!(accepted.acquisitions.is_empty());
        assert!(accepted.derivations.is_empty());
        assert!(
            review
                .warning()
                .unwrap()
                .contains("5 earlier acquisition designs and 2 derivations")
        );
    }

    #[test]
    fn legitimate_source_free_arguments_and_represented_sources_have_distinct_states() {
        let empty = pending(&case());
        let review = review_acquisition_design(&empty, &[record(&empty)], 1_048_576).unwrap();
        assert_eq!(review.status, AcquisitionDesignStatus::NoAcquisitionDesign);
        assert!(review.warning().is_none());
        let proposed = proposed_sources();
        let review = review_acquisition_design(&proposed, &[record(&proposed)], 1_048_576).unwrap();
        assert_eq!(review.status, AcquisitionDesignStatus::Represented);
        assert!(review.pending_designs.is_empty());
    }

    #[test]
    fn omitted_or_unparseable_responses_are_not_reconstructed_as_valid_source_designs() {
        let accepted = pending(&case());
        let mut omitted = record(&proposed_sources());
        omitted.response_content_omitted = true;
        let mut malformed = record(&proposed_sources());
        malformed.response.content = "not a supported argument".into();
        malformed.response_content_sha256 = digest(malformed.response.content.as_bytes());
        malformed.response_content_bytes = malformed.response.content.len();
        let review =
            review_acquisition_design(&accepted, &[omitted, malformed], 1_048_576).unwrap();
        assert!(review.pending_designs.is_empty());
    }
    #[test]
    fn exact_dispositions_resolve_renamed_or_repaired_steps_without_perpetual_pending() {
        let mut proposed = proposed_sources();
        proposed.acquisitions.truncate(1);
        proposed.derivations.clear();
        let source = record(&proposed);
        let mut renamed = proposed.clone();
        renamed.acquisitions[0].id = "replacement-source".into();
        renamed
            .acquisition_revisions
            .push(super::super::AcquisitionRevision {
                previous_step_id: "source-1".into(),
                previous_step_sha256: step_identity(&proposed.acquisitions[0]).unwrap(),
                disposition: AcquisitionDisposition::Replaced,
                replacement_ids: vec!["replacement-source".into()],
                reason:
                    "The revised source definition covers the same retained observation obligation."
                        .into(),
            });
        let review =
            review_acquisition_design(&renamed, std::slice::from_ref(&source), 1_048_576).unwrap();
        assert_eq!(review.status, AcquisitionDesignStatus::Represented);
        assert!(review.pending_designs.is_empty());
        let mut repaired = proposed.clone();
        repaired.acquisitions[0].contract.schema = "eal.support-contract/1".into();
        repaired.acquisitions[0].contract.acceptable_methods = vec!["fixture.read/1".into()];
        let review =
            review_acquisition_design(&repaired, std::slice::from_ref(&source), 1_048_576).unwrap();
        assert_eq!(
            review.status,
            AcquisitionDesignStatus::PendingRepair,
            "same-ID source edits remain attributable"
        );
        assert_eq!(review.pending_designs[0].changed_step_ids, ["source-1"]);
        repaired
            .acquisition_revisions
            .push(super::super::AcquisitionRevision {
                previous_step_id: "source-1".into(),
                previous_step_sha256: step_identity(&proposed.acquisitions[0]).unwrap(),
                disposition: AcquisitionDisposition::Repaired,
                replacement_ids: vec!["source-1".into()],
                reason:
                    "Repair the schema spelling and state the supplied versioned fixture protocol."
                        .into(),
            });
        assert_eq!(
            review_acquisition_design(&repaired, &[source], 1_048_576)
                .unwrap()
                .status,
            AcquisitionDesignStatus::Represented
        );
    }

    #[test]
    fn changed_completion_or_forged_prior_design_anchor_cannot_authenticate_history() {
        let proposed = proposed_sources();
        let accepted = pending(&case());
        let mut tampered = record(&proposed);
        tampered.response.content.push(' ');
        assert!(
            review_acquisition_design(&accepted, &[tampered], 1_048_576)
                .unwrap_err()
                .to_string()
                .contains("SHA-256 identity")
        );
        let mut forged = accepted.clone();
        forged
            .acquisition_revisions
            .push(super::super::AcquisitionRevision {
                previous_step_id: "source-1".into(),
                previous_step_sha256: "0".repeat(64),
                disposition: AcquisitionDisposition::Retired,
                replacement_ids: vec![],
                reason: "This forged identity must be rejected.".into(),
            });
        assert!(
            review_acquisition_design(&forged, &[record(&proposed)], 1_048_576)
                .unwrap_err()
                .to_string()
                .contains("exact retained prior step identity")
        );
    }

    #[test]
    fn explicit_deferral_stays_pending_and_retirement_preserves_its_provenance() {
        let mut proposed = proposed_sources();
        proposed.acquisitions.truncate(1);
        proposed.derivations.clear();
        let source = record(&proposed);
        let mut accepted = pending(&case());
        accepted
            .acquisition_revisions
            .push(super::super::AcquisitionRevision {
                previous_step_id: "source-1".into(),
                previous_step_sha256: step_identity(&proposed.acquisitions[0]).unwrap(),
                disposition: AcquisitionDisposition::Deferred,
                replacement_ids: vec![],
                reason:
                    "The protocol requires clarification before this source design can be repaired."
                        .into(),
            });
        assert_eq!(
            review_acquisition_design(&accepted, std::slice::from_ref(&source), 1_048_576)
                .unwrap()
                .status,
            AcquisitionDesignStatus::PendingRepair
        );
        accepted.acquisition_revisions[0].disposition = AcquisitionDisposition::Retired;
        accepted.acquisition_revisions[0].reason = "The revised conceptual argument retires the empirical observation and retains its reason for retirement.".into();
        let review = review_acquisition_design(&accepted, &[source], 1_048_576).unwrap();
        assert_eq!(review.status, AcquisitionDesignStatus::NoAcquisitionDesign);
        assert_eq!(accepted.acquisition_revisions.len(), 1);
    }
}
