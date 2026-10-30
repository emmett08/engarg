//! Bounded search over declared alternative routes for one unchanged claim.
//!
//! Search changes only `AlternativeRoutes` selections reachable from
//! `current_claim`. It preserves the complete authored graph, including unused
//! routes, conditions, challenges, qualifications and evidence obligations.
//! Length counts unique nodes in the selected dependency closure, not words.
//!
//! The assessment callback owns verification and applicability assessment. Its
//! `supported` result is a hard eligibility gate, not a numerical confidence
//! score. This module neither establishes empirical premise truth nor promotes
//! authored status labels into observations. Adoption of a recommendation is an
//! explicit argument revision requiring a fresh executable assessment.

use crate::domain::{ArgumentCase, SupportRelationship};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod assurance;
pub mod development;
pub mod execution;
pub mod investigation;
mod search;
pub use search::search;

/// Hard bounds constrain both the beam and the number of assessment calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    pub beam_width: usize,
    pub max_candidates: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            beam_width: 8,
            max_candidates: 32,
        }
    }
}

/// One complete selection, including original choices in unexpanded dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub selections: BTreeMap<String, Vec<String>>,
    pub dependency_ids: Vec<String>,
    pub length: usize,
}

impl Candidate {
    /// Apply route choices without deleting or rewriting any authored element.
    /// Joint and cumulative compositions cannot be changed by a recommendation.
    /// Imported recommendations still require normal argument/revision admission.
    pub fn apply(&self, case: &ArgumentCase) -> ArgumentCase {
        let mut candidate = case.clone();
        for composition in &mut candidate.compositions {
            if composition.relationship == SupportRelationship::AlternativeRoutes
                && let Some(selected) = self.selections.get(&composition.conclusion_id)
            {
                composition.selected_warrant_ids = selected.clone();
            }
        }
        candidate
    }
}

/// Assessment of the exact candidate by the caller's verification boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub supported: bool,
    pub status: String,
    pub artifact: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateResult {
    pub candidate: Candidate,
    pub assessment: Assessment,
}

/// Results retain every assessment in call order; the original selection is first.
/// `best` indexes the shortest supported candidate, with lexical selection ties.
/// Completeness concerns only the declared finite alternative-selection family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub best: Option<usize>,
    pub candidates: Vec<CandidateResult>,
    pub search_complete: bool,
    /// Evaluated frontier states discarded by the beam, summed over layers.
    /// Budget-blocked, unevaluated states have unknown cardinality.
    pub pruned_states: usize,
    pub config: SearchConfig,
}
