//! Persisted origins and authored applications of objections to revised targets.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DefeaterBasis, DefeaterKind};

pub const DISSENT_REVISIONS_SCHEMA: &str = "engarg.dissent-revisions/1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DissentRevisions {
    pub schema: String,
    pub applications: Vec<DissentApplication>,
    /// Installed by the host from retained first attachments, never generated.
    #[serde(default)]
    #[schemars(skip)]
    pub origins: Vec<DissentOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DissentOrigin {
    pub objection_id: String,
    pub challenge: String,
    pub target_id: String,
    pub kind: DefeaterKind,
    pub basis: DefeaterBasis,
    pub source_role: String,
    pub must_remain_material: bool,
    pub context_sha256: String,
    pub target_context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DissentApplication {
    pub objection_id: String,
    /// Copy the host-supplied original context identity. Never guess a digest.
    pub origin_context_sha256: String,
    pub effect: DissentRevisionEffect,
    pub rationale: String,
    /// Host-computed from this complete candidate, rather than model-authored.
    #[serde(default)]
    #[schemars(skip)]
    pub current_context_sha256: String,
    #[serde(default)]
    #[schemars(skip)]
    pub changes: Vec<DissentFieldChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DissentRevisionEffect {
    StillApplies,
    RemovedByRevision,
    RequiresReview,
}

/// JSON pointers distinguish a missing property from a present null value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DissentFieldChange {
    pub path: String,
    pub before_present: bool,
    pub before: Value,
    pub after_present: bool,
    pub after: Value,
}
