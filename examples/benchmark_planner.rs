//! Complete deterministic route compilation from a frozen synthetic snapshot.
//! Run separate processes to obtain independent timing observations.
#[path = "../tests/common/support.rs"]
mod support_fixture;

use clap::Parser;
use eal_collector_contracts::capability::{
    CAPABILITY_CATALOGUE_SCHEMA, CapabilityCatalogue, CostEstimate, EstimateBasis,
    canonical_identity,
};
use engarg::route_planning::{DiscoverySnapshot, PlanningConfig, compile};
use serde_json::json;
use std::{error::Error, hint::black_box, time::Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 32, value_parser = clap::value_parser!(u32).range(1..=256))]
    acquisitions: u32,
    #[arg(long, default_value_t = 16, value_parser = clap::value_parser!(u32).range(1..=64))]
    candidates: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let (_, case, mut elaboration, _, routes, _) = support_fixture::fixture();
    let blueprint = elaboration.acquisitions[0].clone();
    let evaluation = elaboration
        .evaluations
        .iter()
        .find(|evaluation| evaluation.requirement_id == "shared_requirement_0")
        .unwrap()
        .clone();
    let requirement = elaboration
        .requirements
        .iter()
        .find(|requirement| requirement.id == evaluation.requirement_id)
        .unwrap()
        .clone();
    for index in 1..args.acquisitions {
        let mut acquisition = blueprint.clone();
        acquisition.id = format!("source_{index}");
        acquisition.arguments["request_index"] = json!(index);
        let mut requirement = requirement.clone();
        requirement.id = format!("profile_requirement_{index}");
        let mut evaluation = evaluation.clone();
        evaluation.id = format!("profile_evaluation_{index}");
        evaluation.requirement_id = requirement.id.clone();
        evaluation.inputs[0].step_id = acquisition.id.clone();
        elaboration.acquisitions.push(acquisition);
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
    }
    let capabilities = (0..args.candidates)
        .map(|index| {
            let mut capability = routes[0].capability.clone();
            capability.id = format!("source_{index:03}");
            capability.costs.execution_requests = CostEstimate {
                value: Some(u64::from(index) + 1),
                basis: EstimateBasis::Declared,
                reference: "Synthetic workload input, not measured provider cost".into(),
            };
            capability
        })
        .collect::<Vec<_>>();
    let discovery = DiscoverySnapshot {
        admitted_capabilities: capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect(),
        ..DiscoverySnapshot::default()
    };
    let catalogue = CapabilityCatalogue {
        schema: CAPABILITY_CATALOGUE_SCHEMA.into(),
        capabilities,
    };
    let config: PlanningConfig = serde_json::from_value(json!({
        "schema":"engarg.capability-planning-config/1", "collector":"/benchmark/collector",
        "connections":"/benchmark/connections.json", "store":"/benchmark/store",
        "ranking":{"dimensions":["execution_requests"]},
        "scheduling":{"maximum_concurrent_sources":4,"per_provider":{}}, "bindings":[]
    }))?;
    let input_sha256 = canonical_identity(&(&case, &elaboration, &catalogue, &config, &discovery))?;
    let start = Instant::now();
    let plan = black_box(compile(
        &case,
        &elaboration,
        &catalogue,
        &config,
        &discovery,
    )?);
    let elapsed = start.elapsed();
    assert_eq!(plan.routes.len(), args.acquisitions as usize);
    assert!(
        plan.routes
            .iter()
            .all(|route| route.selected.as_deref() == Some("source_000"))
    );
    assert_eq!(
        plan.execution.evaluations.len(),
        elaboration.evaluations.len()
    );
    let selection = plan.selection.as_ref().unwrap();
    assert_eq!(
        selection.total_costs,
        vec![(0, u128::from(args.acquisitions))]
    );
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema":"engarg.complete-planner-profile/1", "acquisitions":args.acquisitions,
            "candidates_per_acquisition":args.candidates, "input_sha256":input_sha256,
            "output_sha256":canonical_identity(&plan)?, "elapsed_ns":elapsed.as_nanos(),
            "selection":selection, "evaluations":plan.execution.evaluations.len(),
            "scope":"compile: requirement and catalogue validation, schema matching, argument materialisation, selection, graph projection and execution validation. Excludes fixture construction, JSON decoding, discovery, providers and models."
        }))?
    );
    Ok(())
}
