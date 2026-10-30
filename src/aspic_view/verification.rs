//! Attach supplied EngArg assurance reports to the exact replayed native execution.
use super::{MAX_OUTPUT, invalid};
use crate::{
    Error,
    optimal_argument::assurance::{AssuranceReceipt, RECEIPT_SCHEMA},
};
use eal_core::verification::{REPORT_SCHEMA, Status};
use serde_json::{Value, json};

/// Call after `export` has independently replayed the retained native execution.
/// Identity checks establish correspondence, not authentication of this supplied
/// receipt or independent re-execution of its operator verification contract.
pub fn attach_verification(
    view: &mut Value,
    execution: &Value,
    receipt: &AssuranceReceipt,
) -> Result<(), Error> {
    if view["schema"] != "aspic-view/3" || receipt.schema != RECEIPT_SCHEMA {
        return Err(invalid(
            "ASPIC verification attachment requires aspic-view/3 and engarg.assurance-receipt/2",
        ));
    }
    let run = execution
        .get("result")
        .filter(|_| execution["is_error"] == false)
        .filter(|run| run["schema"] == "EAL/execution/1")
        .ok_or_else(|| {
            invalid("ASPIC verification attachment requires a successful native execution envelope")
        })?;
    let bindings = receipt.lifecycle.bindings.as_ref()
        .filter(|_| receipt.lifecycle.phase == "post_execution")
        .filter(|bindings| bindings.normalization_checked && bindings.replay_checked)
        .filter(|_| matches!(receipt.lifecycle.runtime_status.as_deref(),Some("supported" | "unsupported")))
        .ok_or_else(|| invalid("ASPIC assurance receipt requires completed normalization and native replay in post_execution phase"))?;
    for (name, expected, actual) in [
        (
            "source_digest",
            bindings.source_digest.as_str(),
            &run["source_digest"],
        ),
        (
            "executable_digest",
            bindings.executable_digest.as_str(),
            &run["executable_digest"],
        ),
        (
            "collection_id",
            bindings.collection_id.as_str(),
            &run["collection"]["collection_id"],
        ),
        (
            "assessment_id",
            bindings.assessment_id.as_str(),
            &run["assessment"]["assessment_id"],
        ),
    ] {
        if expected.is_empty() || actual.as_str() != Some(expected) {
            return Err(invalid(format!(
                "ASPIC assurance receipt {name} does not match the native execution"
            )));
        }
    }
    if run["collection"]["source_digest"] != run["source_digest"]
        || run["assessment"]["source_digest"] != run["source_digest"]
        || run["assessment"]["collection_id"] != run["collection"]["collection_id"]
        || run["assessment"]["valid"] != true
        || run["assessment"]["complete"] != true
    {
        return Err(invalid(
            "ASPIC verification attachment requires matching, valid and complete collection/assessment identities",
        ));
    }
    let mut reports = match view.get("verification_reports") {
        Some(Value::Array(reports)) => reports.clone(),
        None => vec![],
        _ => return Err(invalid("ASPIC verification_reports must be an array")),
    };
    for report in &receipt.verification_reports {
        if report.schema != REPORT_SCHEMA
            || report.target_id.is_empty()
            || report.status == Status::NotRequired
        {
            return Err(invalid(
                "ASPIC attachment requires a complete typed verification report with pass, fail or unknown status",
            ));
        }
        reports.push(json!({
            "argument_name":report.target_id, "source":"engarg_assurance",
            // Completion of this supplied evaluation is separate from whether
            // its obligations passed. Retain fail/unknown inside the report.
            "method_status":"supported",
            "reasons":["EngArg operator-contract verification after native replay; supplied receipt, not an authenticated native grounded-method run"],
            "report":report,
        }));
    }
    let mut attached = view.clone();
    attached["verification_reports"] = json!(reports);
    if serde_json::to_vec(&attached)?.len() > MAX_OUTPUT {
        return Err(Error::Budget(
            "ASPIC view including verification reports exceeds the 4 MiB browser import limit"
                .into(),
        ));
    }
    *view = attached;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> AssuranceReceipt {
        serde_json::from_value(json!({
            "schema":RECEIPT_SCHEMA,"plan_sha256":"plan","source_report_sha256":"source-report",
            "candidate_sha256":"candidate","checks":[],"discharged_requirement_ids":[],"limitations":[],
            "verification_reports":[{"schema":REPORT_SCHEMA,"target_id":"current_claim","mode":"mathematical",
                "status":"unknown","obligations":[],"atoms":[],"formulas":[]}],
            "lifecycle":{"phase":"post_execution","authored_status":"conditional","runtime_status":"supported",
                "bindings":{"raw_candidate_sha256":"candidate","admitted_candidate_sha256":"admitted",
                    "admitted_report_sha256":"admitted-report","source_digest":"source","executable_digest":"executable",
                    "collection_id":"collection","assessment_id":"assessment","normalization_checked":true,"replay_checked":true}}
        })).unwrap()
    }

    fn execution() -> Value {
        json!({"is_error":false,"result":{"schema":"EAL/execution/1","source_digest":"source",
            "executable_digest":"executable","collection":{"source_digest":"source","collection_id":"collection"},
            "assessment":{"valid":true,"complete":true,"source_digest":"source","collection_id":"collection","assessment_id":"assessment"}}})
    }

    #[test]
    fn attaches_asserted_reports_without_relabelling_their_outcome_or_native_origin() {
        let mut receipt = receipt();
        for status in [Status::Pass, Status::Fail, Status::Unknown] {
            receipt.verification_reports[0].status = status;
            let native = json!({"argument_name":"native_argument","source":"native_method","method_status":"not_run","reasons":[],"report":null});
            let mut view = json!({"schema":"aspic-view/3","verification_reports":[native.clone()]});
            attach_verification(&mut view, &execution(), &receipt).unwrap();
            assert_eq!(view["verification_reports"][0], native);
            let attached = &view["verification_reports"][1];
            assert_eq!(attached["source"], "engarg_assurance");
            assert_eq!(attached["argument_name"], "current_claim");
            assert_eq!(attached["method_status"], "supported");
            assert_eq!(
                attached["report"],
                serde_json::to_value(&receipt.verification_reports[0]).unwrap()
            );
        }
    }

    #[test]
    fn rejects_changed_bindings_unchecked_lifecycle_and_incomplete_runs_atomically() {
        let good = receipt();
        let original = json!({"schema":"aspic-view/3"});
        for field in [
            "source_digest",
            "executable_digest",
            "collection_id",
            "assessment_id",
            "phase",
            "normalization_checked",
            "replay_checked",
        ] {
            let mut value = serde_json::to_value(&good).unwrap();
            match field {
                "phase" => value["lifecycle"]["phase"] = json!("pre_execution"),
                "normalization_checked" | "replay_checked" => {
                    value["lifecycle"]["bindings"][field] = json!(false)
                }
                _ => value["lifecycle"]["bindings"][field] = json!("changed"),
            }
            let wrong: AssuranceReceipt = serde_json::from_value(value).unwrap();
            let mut view = original.clone();
            assert!(
                attach_verification(&mut view, &execution(), &wrong).is_err(),
                "{field}"
            );
            assert_eq!(view, original);
        }
        let mut run = execution();
        run["result"]["assessment"]["complete"] = json!(false);
        assert!(attach_verification(&mut original.clone(), &run, &good).is_err());
    }

    #[test]
    fn attached_reports_share_the_browser_byte_limit() {
        let mut receipt = receipt();
        receipt.verification_reports[0].obligations.push(
            eal_core::verification::ObligationResult {
                id: "large".into(),
                role: eal_core::verification::Role::Grounding,
                status: Status::Unknown,
                reasons: vec!["x".repeat(MAX_OUTPUT)],
                details: json!({}),
            },
        );
        let original = json!({"schema":"aspic-view/3"});
        let mut view = original.clone();
        assert!(matches!(
            attach_verification(&mut view, &execution(), &receipt),
            Err(Error::Budget(_))
        ));
        assert_eq!(view, original);
    }
}
