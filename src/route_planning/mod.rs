//! Deterministic adequacy-first compilation after tool-independent elaboration.
mod arguments;
mod compiler;
mod configuration;
mod graphs;
pub use graphs::compile;
mod candidates;
mod implementation;
mod preview;
mod selection;
#[cfg(test)]
mod tests;
mod types;
pub use types::*;

use crate::{
    Error, domain::ArgumentCase, engine::RouteResolver, support_requirements::SupportElaboration,
};
use eal_collector_contracts::capability::CapabilityCatalogue;

/// Discovery is performed once by the operator-selected collector. Revisions use
/// the same bounded snapshot; runtime re-admits the live interface before reuse.
pub struct Planner {
    pub catalogue: CapabilityCatalogue,
    pub config: PlanningConfig,
    pub discovery: DiscoverySnapshot,
}
impl RouteResolver for Planner {
    type Plan = RoutePlan;
    fn preview(&self, plan: &Self::Plan) -> Result<serde_json::Value, Error> {
        Ok(serde_json::json!({
            "schema":plan.schema,
            "routes":plan.routes,
            "gaps":plan.gaps.iter().map(|gap|serde_json::json!({"acquisition_id":gap.acquisition_id,"requirement_id":gap.requirement_id,"kind":gap.kind,"description":gap.description})).collect::<Vec<_>>(),
            "qualified_sources":plan.projections.iter().map(|route|serde_json::json!({"blueprint_id":route.blueprint_id,"capability_id":route.capability.id,"version":route.capability.version})).collect::<Vec<_>>(),
            "qualified_alternatives":preview::alternatives(&self.catalogue, plan),
            "selection":plan.selection,
            "discovery_incomplete":plan.discovery.incomplete,
            "acquisition_count":plan.execution.acquisitions.len(),
            "derivation_count":plan.execution.derivations.len(),
            "evaluation_count":plan.execution.evaluations.len(),
        }))
    }
    async fn resolve(
        &self,
        case: &ArgumentCase,
        elaboration: &SupportElaboration,
    ) -> Result<RoutePlan, Error> {
        compile(
            case,
            elaboration,
            &self.catalogue,
            &self.config,
            &self.discovery,
        )
    }
}
