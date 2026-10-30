//! Record the checked transition from authored proposal to native execution.
use super::*;
use crate::domain::ArgumentCase;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationLifecycle {
    pub phase: String,
    pub authored_status: String,
    pub runtime_status: Option<String>,
    pub bindings: Option<ExecutionBindings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBindings {
    pub raw_candidate_sha256: String,
    pub admitted_candidate_sha256: String,
    pub admitted_report_sha256: String,
    pub source_digest: String,
    pub executable_digest: String,
    pub collection_id: String,
    pub assessment_id: String,
    pub normalization_checked: bool,
    pub replay_checked: bool,
}

impl InvestigationLifecycle {
    pub(super) fn pending(argument: &ArgumentCase) -> Self {
        Self {
            phase: "pre_execution".into(),
            authored_status: serde_json::to_value(argument.status)
                .ok()
                .and_then(|s| s.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".into()),
            runtime_status: None,
            bindings: None,
        }
    }
}

impl AssuranceReceipt {
    /// Called only after explicit revision admission and independent native replay.
    pub fn record_execution(
        &mut self,
        proposal: &SupportedArgument,
        report: &RunReport,
        envelope: &Value,
        native_status: &str,
    ) -> Result<(), Error> {
        let mut normalized = proposal.argument.clone();
        // Admission installs retained dissent history. All other fields and the
        // entire support contract must remain byte-semantically identical.
        normalized.dissent_revisions = report.argument.dissent_revisions.clone();
        let normalization_checked = digest(&normalized)? == digest(&report.argument)?
            && digest(&Some(&proposal.support_elaboration))?
                == digest(&report.support_elaboration)?;
        let run = &envelope["result"];
        let collection = &run["collection"];
        let assessment = &run["assessment"];
        let identities_present = [
            &run["source_digest"],
            &run["executable_digest"],
            &collection["collection_id"],
            &assessment["assessment_id"],
        ]
        .iter()
        .all(|value| value.as_str().is_some_and(|value| !value.is_empty()));
        let replay_checked = envelope["is_error"] == false
            && run["schema"] == "EAL/execution/1"
            && identities_present
            && collection["source_digest"] == run["source_digest"]
            && assessment["source_digest"] == run["source_digest"]
            && assessment["collection_id"] == collection["collection_id"]
            && collection["records"].is_object()
            && collection["context"].is_object()
            && matches!(native_status, "supported" | "unsupported")
            && run["assessment"]["valid"] == true
            && run["assessment"]["complete"] == true;
        self.lifecycle.phase = if replay_checked {
            "post_execution"
        } else {
            "execution_incomplete"
        }
        .into();
        self.lifecycle.runtime_status = Some(native_status.into());
        self.lifecycle.bindings = Some(ExecutionBindings {
            raw_candidate_sha256: digest(proposal)?,
            admitted_candidate_sha256: crate::optimal_argument::development::candidate_digest(
                report,
            )?,
            admitted_report_sha256: digest(report)?,
            source_digest: run["source_digest"].as_str().unwrap_or("").into(),
            executable_digest: run["executable_digest"].as_str().unwrap_or("").into(),
            collection_id: run["collection"]["collection_id"]
                .as_str()
                .unwrap_or("")
                .into(),
            assessment_id: run["assessment"]["assessment_id"]
                .as_str()
                .unwrap_or("")
                .into(),
            normalization_checked,
            replay_checked,
        });
        if !normalization_checked {
            self.checks.push(guards::guard("execution_normalization",CheckStatus::Fail,vec!["Admitted candidate changed fields beyond checked dissent-history normalization".into()]));
        }
        Ok(())
    }
}
