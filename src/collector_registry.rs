//! Bind EngArg facade commands using EAL-owned compatibility adapter contracts.
use crate::{Error, acquisition::AcquisitionPlan, collector_catalogue::RegistryCommand};

pub(crate) fn binding(plan: &AcquisitionPlan, need_id: &str) -> Result<RegistryCommand, Error> {
    if plan.tool_id == "custom" {
        return Ok(RegistryCommand {
            kind: eal_collector_contracts::ToolKind::Command,
            version: "1".into(),
            argv: vec![
                "/REPLACE_WITH_CUSTOM_COLLECTOR".into(),
                "--need-id".into(),
                need_id.into(),
            ],
            inherit_env: Vec::new(),
            timeout_seconds: 65,
            max_output_bytes: 4_194_304,
            parallel_safe: false,
        });
    }
    let profile = eal_adapters::profiles::require(&plan.tool_id)?;
    Ok(RegistryCommand {
        kind: eal_collector_contracts::ToolKind::Command,
        version: profile.version.into(),
        argv: vec![
            "engarg".into(),
            "eal".into(),
            "observe".into(),
            "--tool".into(),
            plan.tool_id.clone(),
        ],
        inherit_env: profile
            .inherit_env
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        timeout_seconds: profile.timeout_seconds,
        max_output_bytes: profile.max_output_bytes,
        parallel_safe: profile.parallel_safe,
    })
}
