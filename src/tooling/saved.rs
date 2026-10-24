//! Recover canonical saved inputs without fabricating a provider exchange or replacing source plans.

use serde_json::Value;

use crate::{
    Error,
    acquisition::{self, AcquisitionPlan},
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request, validate_argument, validate_request},
    eal_export::EalExport,
    engine::RunReport,
};

pub(super) struct SavedInput {
    pub request: Request,
    pub argument: ArgumentCase,
    pub plans: Vec<AcquisitionPlan>,
    pub catalogue: CollectorCatalogue,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn empty_catalogue() -> CollectorCatalogue {
    CollectorCatalogue(eal_collector_contracts::Catalogue {
        schema: eal_collector_contracts::CATALOGUE_SCHEMA.into(),
        collectors: Vec::new(),
    })
}

fn catalogue(value: Option<&Value>) -> Result<CollectorCatalogue, Error> {
    match value.filter(|value| !value.is_null()) {
        Some(value) => CollectorCatalogue::from_json(&serde_json::to_vec(value)?),
        None => Ok(empty_catalogue()),
    }
}

/// Parse the original envelope and read the last exact, retained planned argument.
/// The returned records are clones of authored records; no observations or transcript are added.
pub(super) fn parse(source: &Value) -> Result<SavedInput, Error> {
    if serde_json::to_vec(source)?.len() > 16 * 1_048_576 {
        return Err(Error::Budget("saved analysis exceeds 16 MiB".into()));
    }
    let mut canonical = source.clone();
    let object = canonical.as_object_mut().ok_or_else(|| invalid("saved analysis must be an object"))?;
    object.remove("tooling");
    let saved = if object.contains_key("original_request") || object.contains_key("original_case") {
        let bundle: EalExport = serde_json::from_value(canonical)?;
        crate::eal_acquisition::reconstruct_bundle(&bundle)?;
        let plans = crate::eal_acquisition::plan_from_bundle(&bundle)?.unwrap_or_default();
        let selected = catalogue(bundle.collector_catalogue.as_ref())?;
        SavedInput {
            request: bundle.original_request,
            argument: bundle.original_case,
            plans,
            catalogue: selected,
        }
    } else {
        let report: RunReport = serde_json::from_value(canonical)?;
        if !matches!(report.schema_version.as_str(), "engarg.run.v1" | "engarg.run.v2") {
            return Err(invalid("unsupported saved run schema"));
        }
        let expected = serde_json::to_value(&report.argument)?;
        let mut plans = Vec::new();
        for record in report.transcript.iter().rev() {
            if record.response_content_omitted {
                continue;
            }
            if let Ok(planned) = acquisition::parse_completion(&record.response.content)
                && serde_json::to_value(&planned.argument)? == expected
            {
                plans = planned.acquisition_plan;
                break;
            }
        }
        let selected = report.run_config.as_ref()
            .and_then(|config| config.collector_catalogue.as_ref())
            .cloned().unwrap_or_else(empty_catalogue);
        SavedInput { request: report.request, argument: report.argument, plans, catalogue: selected }
    };
    validate_request(&saved.request).map_err(|errors| invalid(errors.join("; ")))?;
    validate_argument(&saved.request, &saved.argument)
        .map_err(|errors| invalid(errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")))?;
    saved.catalogue.validate()?;
    if !saved.plans.is_empty() {
        let errors = acquisition::validate_with_catalogue(&saved.argument, &saved.plans, Some(&saved.catalogue));
        if !errors.is_empty() {
            return Err(invalid(errors.join("; ")));
        }
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report() -> Value {
        let request: Request = serde_json::from_str(include_str!("../../examples/request.json")).unwrap();
        let argument: ArgumentCase = serde_json::from_str(include_str!("../../examples/case.json")).unwrap();
        json!({
            "schema_version":"engarg.run.v2","execution_mode":"saved_fixture","input_sha256":"fixture",
            "mode":"single","request":request,"argument":argument,"transcript":[],"limitations":[]
        })
    }

    #[test]
    fn unplanned_report_stays_unplanned_without_a_transcript() {
        let source = report();
        let saved = parse(&source).unwrap();
        assert!(saved.plans.is_empty());
        assert!(saved.catalogue.collectors.is_empty());
        assert_eq!(source["transcript"], json!([]));
    }

    #[test]
    fn exact_retained_plans_take_precedence_over_tooling_metadata() {
        let mut source = report();
        let argument: ArgumentCase = serde_json::from_value(source["argument"].clone()).unwrap();
        let plans = argument.evidence_plan.iter().map(|need| AcquisitionPlan {
            need_id:need.id.clone(),tool_id:"custom".into(),arguments:vec![],unresolved_arguments:vec!["adapter".into()],
        }).collect::<Vec<_>>();
        let content = json!({"argument":argument,"acquisition_plan":plans}).to_string();
        source["transcript"] = json!([{
            "role":"single",
            "prompt":{"role":"single","system":"fixture","user":"{}","schema":{}},
            "response":{"content":content,"model":"fixture","input_tokens":null,"output_tokens":null,"request_id":null},
            "response_content_bytes":content.len(),"response_content_sha256":"fixture","response_content_omitted":false,
            "elapsed_ms":0
        }]);
        source["tooling"] = json!({"original_plans":[],"original_catalogue":null});
        let saved = parse(&source).unwrap();
        assert_eq!(serde_json::to_value(saved.plans).unwrap(), serde_json::to_value(plans).unwrap());
        assert_eq!(source["transcript"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rejects_incomplete_canonical_source_instead_of_inventing_request() {
        assert!(parse(&json!({"argument":report()["argument"]})).is_err());
    }
}
