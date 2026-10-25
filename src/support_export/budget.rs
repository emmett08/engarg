//! Conservative collection readiness against the normal EAL host budget.
use super::SupportExport;
use crate::domain::ArgumentIndex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// EAL's advertised default, rather than a language or immutable runtime limit.
pub const DEFAULT_COLLECTION_EVIDENCE_LIMIT: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub struct CollectionBudget {
    pub schema: &'static str,
    pub assumed_default_limit: usize,
    pub limit_source: &'static str,
    /// Normal collect without explicit names selects every emitted declaration.
    pub total_emitted_evidence: usize,
    /// Selected claim collection includes its generated gates and authored reviews.
    pub claim_evidence_counts: BTreeMap<String, usize>,
}

/// Count declarations from the projection metadata, without parsing its own EAL.
pub fn collection_budget(bundle: &SupportExport) -> CollectionBudget {
    let guards = bundle
        .alternative_projection
        .as_ref()
        .map(|projection| projection.guards.as_slice())
        .unwrap_or_default();
    let index = ArgumentIndex::new(&bundle.authored.original_case);
    CollectionBudget {
        schema: "engarg.collection-budget/1",
        assumed_default_limit: DEFAULT_COLLECTION_EVIDENCE_LIMIT,
        limit_source: "EAL default collection_evidence; the selected host may advertise an override",
        total_emitted_evidence: bundle.authored.obligations.len()
            + bundle.requirements.len()
            + guards.len()
            + bundle
                .authored
                .original_case
                .warrants
                .iter()
                .filter(|warrant| warrant.reasoning_method.is_some())
                .count(),
        claim_evidence_counts: bundle
            .claim_names
            .keys()
            .map(|claim| {
                let cone = index.selected_dependency(claim);
                let source_ids: BTreeSet<_> = bundle
                    .authored
                    .obligations
                    .iter()
                    .filter(|obligation| cone.contains(obligation.target_id.as_str()))
                    .flat_map(|obligation| {
                        obligation.source_evidence_ids.iter().map(String::as_str)
                    })
                    .collect();
                let count = bundle
                    .authored
                    .obligations
                    .iter()
                    .filter(|obligation| {
                        cone.contains(obligation.target_id.as_str())
                            || (obligation.kind == "source_review"
                                && source_ids.contains(obligation.target_id.as_str()))
                    })
                    .count()
                    + bundle
                        .requirements
                        .iter()
                        .filter(|requirement| requirement.gated_claim_ids.contains(claim))
                        .count()
                    + bundle
                        .authored
                        .original_case
                        .warrants
                        .iter()
                        .filter(|warrant| {
                            warrant.reasoning_method.is_some() && cone.contains(warrant.id.as_str())
                        })
                        .count()
                    + guards
                        .iter()
                        .filter(|guard| guard.claim_ids.contains(claim))
                        .count();
                (claim.clone(), count)
            })
            .collect(),
    }
}
