//! Resolved, identity-bound freshness policy for current export projections.
use crate::{Error, domain::ArgumentCase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const POLICY_SCHEMA: &str = "engarg.export-policy/1";
pub const DEFAULT_MAX_AGE_SECONDS: u64 = 86_400;
const MAX_AGE_SECONDS: u64 = 315_360_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPolicy {
    pub schema: String,
    pub review_max_age_seconds: u64,
    pub acquisition_max_age_seconds: u64,
    /// Resolved entries cover every authored evidence need.
    #[serde(default)]
    pub acquisition_per_need: BTreeMap<String, u64>,
}
impl Default for ExportPolicy {
    fn default() -> Self {
        Self {
            schema: POLICY_SCHEMA.into(),
            review_max_age_seconds: DEFAULT_MAX_AGE_SECONDS,
            acquisition_max_age_seconds: DEFAULT_MAX_AGE_SECONDS,
            acquisition_per_need: BTreeMap::new(),
        }
    }
}
impl ExportPolicy {
    pub fn resolve(&self, case: &ArgumentCase) -> Result<Self, Error> {
        if self.schema != POLICY_SCHEMA
            || [
                self.review_max_age_seconds,
                self.acquisition_max_age_seconds,
            ]
            .into_iter()
            .chain(self.acquisition_per_need.values().copied())
            .any(|value| !(1..=MAX_AGE_SECONDS).contains(&value))
        {
            return Err(Error::InvalidInput("EXPORT_POLICY_INVALID: ages must be positive seconds through 315360000 and use the supported schema".into()));
        }
        if self
            .acquisition_per_need
            .keys()
            .any(|id| !case.evidence_plan.iter().any(|need| &need.id == id))
        {
            return Err(Error::InvalidInput(
                "EXPORT_POLICY_NEED: policy names an unknown evidence need".into(),
            ));
        }
        let mut resolved = self.clone();
        for need in &case.evidence_plan {
            resolved
                .acquisition_per_need
                .entry(need.id.clone())
                .or_insert(self.acquisition_max_age_seconds);
        }
        Ok(resolved)
    }
}
