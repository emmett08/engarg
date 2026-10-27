//! Validate the authored obligations against the operator's direct bindings.
use super::*;
use engarg::{
    domain::ArgumentCase, engine::RouteResolver, support_requirements::SupportElaboration,
};

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("integrate");
    directory(&dir)?;
    let mut analysis: Value = configuration::read(&previous(request, "analysis")?)?;
    let case: ArgumentCase = serde_json::from_value(analysis["argument"].clone())?;
    let elaboration: SupportElaboration =
        serde_json::from_value(analysis["support_elaboration"].clone())?;
    let planner = engarg::direct_planning::DirectPlanner::from_tools(&config.tools)?;
    let plan = planner.resolve(&case, &elaboration).await?;
    let completed = plan.unbound_operations.is_empty();
    let outcome = json!({
        "phase":"integrate", "complete":completed,
        "reason":if completed {"direct_bindings_ready"} else {"required_operation_unbound"},
        "direct_bindings":plan,
    });
    configuration::write_json(&dir.join("integration.json"), &outcome)?;
    let mut artifacts = vec![artifact("integration", &dir.join("integration.json"))?];
    if completed {
        analysis["direct_bindings"] = serde_json::to_value(&plan)?;
        configuration::write_json(&dir.join("analysis.ready.json"), &analysis)?;
        artifacts.push(artifact(
            "analysis_ready",
            &dir.join("analysis.ready.json"),
        )?);
    }
    Ok(ResultData {
        artifacts,
        outcome,
        completed,
    })
}
