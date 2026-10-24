//! Render operator-owned command bindings for native, generic and custom collectors.
use crate::{acquisition::AcquisitionPlan, eal_export::quoted};

pub(crate) fn binding(plan: &AcquisitionPlan, need_id: &str) -> String {
    if let Some(tool) = crate::native_collectors::spec(&plan.tool_id) {
        tool.registry(plan)
    } else if plan.tool_id == "custom" {
        custom(need_id)
    } else {
        generic(plan)
    }
}
fn custom(need_id: &str) -> String {
    format!(
        "kind = \"command\"\nversion = \"1\"\nargv = [\"/REPLACE_WITH_CUSTOM_COLLECTOR\", \"--need-id\", {}]\ntimeout_seconds = 65\nmax_output_bytes = 4194304\n# Implementation stub. Select a native collector where its measurement fits.\n# Implement the EAL stdin request and output envelope for this exact evidence need.\n# Full method and acceptance criterion appear in the matching EAL source record.\n\n",
        quoted(need_id)
    )
}
fn generic(plan: &AcquisitionPlan) -> String {
    let inherited = match plan.tool_id.as_str() {
        "kubectl_get" => "[\"PATH\", \"HOME\", \"KUBECONFIG\"]",
        "gh_api_get" => "[\"PATH\", \"HOME\", \"GH_TOKEN\", \"GH_HOST\"]",
        "http_json_get" => "[\"PATH\", \"ENGARG_COLLECTOR_TOKEN\", \"ENGARG_COLLECTOR_ORIGIN\"]",
        _ => unreachable!("validated tool catalogue"),
    };
    format!(
        "kind = \"command\"\nversion = \"1\"\nargv = [\"engarg\", \"eal\", \"observe\", \"--tool\", {}]\ntimeout_seconds = 65\nmax_output_bytes = 4194304\ninherit_env = {inherited}\nparallel_safe = false\n# Named arguments are in the matching EAL evidence input.arguments object.\n# Resolve every required argument before collection.\n# The adapter invokes the selected read operation without a shell.\n\n",
        quoted(&plan.tool_id)
    )
}
