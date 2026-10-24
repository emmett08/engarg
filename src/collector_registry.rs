//! Bind EngArg's generic acquisition adapters to structured operator commands.
use crate::{Error, acquisition::AcquisitionPlan, collector_catalogue::RegistryCommand};

pub(crate) fn binding(plan: &AcquisitionPlan, need_id: &str) -> Result<RegistryCommand, Error> {
    let (argv, inherit_env) = if plan.tool_id == "custom" {
        (
            vec![
                "/REPLACE_WITH_CUSTOM_COLLECTOR".into(),
                "--need-id".into(),
                need_id.into(),
            ],
            vec![],
        )
    } else {
        let inherited: &[&str] = match plan.tool_id.as_str() {
            "artifact_json" => &[],
            "http_json_get" => &["PATH", "ENGARG_COLLECTOR_TOKEN", "ENGARG_COLLECTOR_ORIGIN"],
            // Explicit bindings in historical reports remain usable during migration.
            "scoped_json" => &["PATH", "HOME", "KUBECONFIG", "TEMPO_TOKEN", "TEMPO_TENANT"],
            "kubectl_get" => &["PATH", "HOME", "KUBECONFIG"],
            "gh_api_get" => &["PATH", "HOME", "GH_TOKEN", "GH_HOST"],
            _ => {
                return Err(Error::InvalidInput(
                    "collector needs an operator-selected descriptor".into(),
                ));
            }
        };
        (
            vec![
                "engarg".into(),
                "eal".into(),
                "observe".into(),
                "--tool".into(),
                plan.tool_id.clone(),
            ],
            inherited.iter().map(|name| (*name).into()).collect(),
        )
    };
    Ok(RegistryCommand {
        kind: "command".into(),
        version: "1".into(),
        argv,
        inherit_env,
        timeout_seconds: 65,
        max_output_bytes: 4_194_304,
        parallel_safe: false,
    })
}
