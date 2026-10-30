//! Verify retained execution identities and recompute source-bound EAL assessments.
use eal_core::{ExecutionLimits, canonical, composition, evaluation, methods};
use serde_json::Value;

/// Recompute the exact retained assessment with the linked EAL kernel. This
/// checks correspondence and calculation, not external source authenticity.
pub(super) fn verify_execution(
    source: &str,
    receipt: &Value,
    envelope: &Value,
    context: &Value,
) -> Result<(String, Vec<String>), String> {
    if envelope["is_error"] != false {
        return Err("Native EAL execution returned an error; inspect execution.json".into());
    }
    let run = &envelope["result"];
    let collection = &run["collection"];
    let recorded = &run["assessment"];
    let claim = receipt["claims"]["current_claim"]
        .as_str()
        .filter(|claim| !claim.is_empty())
        .ok_or("Build receipt has no selected current claim")?;
    let source_digest = receipt["source_digest"]
        .as_str()
        .filter(|digest| !digest.is_empty())
        .ok_or("Build receipt has no source digest")?;
    if run["schema"] != "EAL/execution/1"
        || run["claim"] != claim
        || run["source_digest"] != source_digest
        || collection["source_digest"] != source_digest
        || recorded["source_digest"] != source_digest
        || run["executable_digest"] != receipt["executable_digest"]
        || collection["context"] != *context
        || !collection["records"].is_object()
        || collection["collection_id"]
            .as_str()
            .is_none_or(str::is_empty)
        || recorded["collection_id"] != collection["collection_id"]
        || recorded["assessment_id"].as_str().is_none_or(str::is_empty)
    {
        return Err("Execution, package, collection or assessment identities disagree".into());
    }
    if recorded["valid"] != true || recorded["complete"] != true {
        return Ok((
            "incomplete".into(),
            vec![format!(
                "Native assessment is invalid or incomplete: {}",
                recorded["diagnostics"]
            )],
        ));
    }
    let now = recorded["assessed_at"]
        .as_str()
        .filter(|time| !time.is_empty())
        .ok_or("Native assessment has no assessment time")?;
    let program = composition::compile(source, None, "", ExecutionLimits::default())
        .map_err(|error| error.to_string())?;
    if program.source_digest != source_digest {
        return Err("Candidate source differs from its build receipt".into());
    }
    let registry = methods::default_registry();
    let executable_digest = eal_core::semantic_format::executable_digest(&program, registry)
        .map_err(|error| error.to_string())?;
    if receipt["executable_digest"] != executable_digest {
        return Err("Candidate executable differs from its build receipt".into());
    }
    let recomputed = evaluation::evaluate(
        &program,
        &collection["records"],
        now,
        context,
        registry,
        None,
    )?;
    let mut expected = recorded.clone();
    let expected = expected
        .as_object_mut()
        .ok_or("Native assessment must be an object")?;
    expected.remove("collection_id");
    expected.remove("assessment_id");
    if !canonical::same_json(
        &Value::Object(expected.clone()),
        &recomputed,
        &program.limits,
    ) {
        return Err("Retained source, observations, context and time do not reproduce the native assessment".into());
    }
    let plan =
        eal_core::planning::EvidencePlanner::new_with_registry(&program, registry)?.plan(claim)?;
    let unresolved: Vec<_> = plan
        .evidence_ids()
        .into_iter()
        .filter(|id| {
            collection["records"][id]["status"] != "ok"
                || !eal_coordination_contracts::ticket::evidence_is_measured(
                    &recomputed["evidence"][id],
                )
        })
        .collect();
    if !unresolved.is_empty() {
        return Ok((
            "incomplete".into(),
            unresolved
                .into_iter()
                .map(|id| {
                    format!(
                        "Selected evidence {id} is unresolved: {}",
                        recomputed["evidence"][&id]
                    )
                })
                .collect(),
        ));
    }
    let native_status = recomputed["claims"][claim]["status"]
        .as_str()
        .filter(|status| !status.is_empty())
        .ok_or("Recomputed assessment has no selected claim status")?;
    let status = if native_status == "supported" {
        "supported"
    } else {
        "unsupported"
    };
    let mut reasons = vec![format!("Recomputed native claim status: {native_status}")];
    if let Some(native_reasons) = recomputed["claims"][claim]["reasons"].as_array() {
        reasons.extend(
            native_reasons
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    Ok((status.into(), reasons))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: &str = "2026-10-30T12:00:00Z";
    const SOURCE: &str = r#"language "EAL/4"
environment bench {
 require site == "synthetic"
}
tool reader {
 version "1"
}
evidence observed {
 tool reader
 kind test
 environment bench
 max_age 60
 require passed == true
}
reasoning measured {
 method "structured/1"
 rationale "The identified synthetic record supplies the bounded test result."
}
claim working {
 statement "The synthetic test passed."
 environment bench
}
argument support = [evidence observed] via measured => working
"#;

    /// Synthetic observations are independent test inputs, never tool results.
    fn fixture(passed: bool, observed_at: &str) -> (Value, Value, Value) {
        let program = composition::compile(SOURCE, None, "", ExecutionLimits::default()).unwrap();
        let registry = methods::default_registry();
        let context = json!({"site":"synthetic"});
        let evidence = &program.evidence["observed"];
        let tool = &program.tools["reader"];
        let acquisition = json!({"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
        let request = json!({"evidence_id":"observed","environment":evidence.environment,"tool":tool.name,"tool_version":tool.version,"input":evidence.input,"context":context});
        let value = json!({"passed":passed});
        let digest = |value: &Value| canonical::canonical_digest(value, &program.limits).unwrap();
        let records = json!({"observed":{
            "schema":"EAL/observation-record/1", "evidence_id":"observed",
            "source_digest":program.source_digest,"tool":tool.name,
            "tool_version":tool.version,"tool_binding_digest":"0".repeat(64),
            "evidence_kind":evidence.kind,"environment":evidence.environment,
            "environment_fingerprint":evaluation::environment_fingerprint(&evidence.environment,&context,&program.limits).unwrap(),
            "input_digest":digest(&evidence.input),"input":evidence.input,
            "context":context,"acquisition_request":acquisition,
            "acquisition_request_digest":digest(&acquisition),
            "request_digest":digest(&request),"collected_at":observed_at,
            "run_id":"synthetic-observed","status":"ok",
            "data_digest":digest(&value),"value":value
        }});
        let mut assessment =
            evaluation::evaluate(&program, &records, NOW, &context, registry, None).unwrap();
        assessment["collection_id"] = json!("synthetic-collection");
        assessment["assessment_id"] = json!("synthetic-assessment");
        let executable_digest =
            eal_core::semantic_format::executable_digest(&program, registry).unwrap();
        let receipt = json!({
            "source_digest":program.source_digest,"executable_digest":executable_digest,
            "claims":{"current_claim":"working"}
        });
        let envelope = json!({"is_error":false,"result":{
            "schema":"EAL/execution/1","source_digest":program.source_digest,
            "executable_digest":executable_digest,"claim":"working",
            "collection":{"collection_id":"synthetic-collection","source_digest":program.source_digest,"context":context,"records":records},
            "assessment":assessment
        }});
        (receipt, envelope, context)
    }

    #[test]
    fn native_support_and_measured_negative_require_the_same_exact_replay() {
        for (passed, expected) in [(true, "supported"), (false, "unsupported")] {
            let (receipt, envelope, context) = fixture(passed, NOW);
            let (status, _) = verify_execution(SOURCE, &receipt, &envelope, &context).unwrap();
            assert_eq!(status, expected);
            assert_eq!(
                envelope["result"]["collection"]["records"]["observed"]["status"],
                "ok"
            );
        }
    }

    #[test]
    fn old_observation_is_not_refreshed_by_recomputation() {
        let (receipt, envelope, context) = fixture(true, "2026-10-29T12:00:00Z");
        let (status, _) = verify_execution(SOURCE, &receipt, &envelope, &context).unwrap();
        assert_eq!(status, "incomplete");
        assert_eq!(
            envelope["result"]["collection"]["records"]["observed"]["collected_at"],
            "2026-10-29T12:00:00Z"
        );
    }

    #[test]
    fn changed_observation_or_claim_status_cannot_inherit_a_positive_assessment() {
        let (receipt, envelope, context) = fixture(true, NOW);
        let mut altered_observation = envelope.clone();
        altered_observation["result"]["collection"]["records"]["observed"]["value"] =
            json!({"passed":false});
        let mut altered_assessment = envelope.clone();
        altered_assessment["result"]["assessment"]["claims"]["working"]["status"] =
            json!("unsupported");
        let mut altered_context = envelope.clone();
        altered_context["result"]["collection"]["context"]["site"] = json!("other");
        let mut altered_collection = envelope;
        altered_collection["result"]["assessment"]["collection_id"] = json!("other");
        for changed in [
            altered_observation,
            altered_assessment,
            altered_context,
            altered_collection,
        ] {
            assert!(verify_execution(SOURCE, &receipt, &changed, &context).is_err());
        }
    }

    #[test]
    fn changed_source_and_incomplete_assessment_are_never_supported() {
        let (receipt, mut envelope, context) = fixture(true, NOW);
        assert!(
            verify_execution(
                &SOURCE.replace("The synthetic test passed.", "A different claim."),
                &receipt,
                &envelope,
                &context,
            )
            .is_err()
        );
        envelope["result"]["assessment"]["complete"] = json!(false);
        assert_eq!(
            verify_execution(SOURCE, &receipt, &envelope, &context)
                .unwrap()
                .0,
            "incomplete"
        );
    }
}
