//! Register the exported program and collect and assess its selected claim.
use super::*;

async fn operation(host: &HostConfig, request: &Value, path: &Path) -> Result<Value, Error> {
    let envelope = invoke(host, request).await?;
    configuration::write_json(path, &envelope)?;
    if envelope["is_error"] != false {
        return Err(Error::InvalidOutput(
            "EAL ticket operation returned an error; retained envelope identifies the outcome"
                .into(),
        ));
    }
    Ok(envelope["result"].clone())
}
pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("execute");
    directory(&dir)?;
    let bundle: Value = configuration::read(&previous(request, "bundle")?)?;
    let source = previous(request, "source")?;
    let source_bytes = engarg::bounded_file::read(&source, 16 * 1_048_576, "ticket source")?;
    if bundle["source"].as_str().map(str::as_bytes) != Some(source_bytes.as_slice()) {
        return Err(Error::InvalidInput(
            "ticket bundle and source artifact differ".into(),
        ));
    }
    let tools = previous(request, "tools")?;
    let context = dir.join("context.json");
    configuration::write_json(&context, &bundle["context"])?;
    let claim = bundle["claim_names"]["current_claim"]
        .as_str()
        .ok_or_else(|| Error::InvalidOutput("ticket bundle has no selected claim".into()))?;
    let entry = format!("ticket_{}", config.ticket_id);
    let database = request.workspace.join("evidence.sqlite");
    let application = config
        .eal_application
        .as_deref()
        .ok_or_else(|| Error::InvalidInput("EAL application executable unavailable".into()))?;
    let args = vec![
        "--workspace".into(),
        process::path(&request.workspace)?,
        "--registry".into(),
        process::path(&tools)?,
        "--database".into(),
        process::path(&database)?,
        "register".into(),
        process::path(source.strip_prefix(&request.workspace).map_err(|_| {
            Error::InvalidInput("registered ticket source escapes its workspace".into())
        })?)?,
        "--entry-id".into(),
        entry.clone(),
        "--claim".into(),
        claim.into(),
        "--context".into(),
        format!("@{}", process::path(&context)?),
    ];
    let registration = process::invoke(
        application,
        &args,
        &request.workspace,
        config.stage_timeout_seconds,
        &dir.join("register-command"),
    )
    .await?;
    if !registration.success {
        return Ok(ResultData {
            artifacts: vec![],
            completed: false,
            outcome: json!({"reason":"source_registration_failed"}),
        });
    }
    let registered =
        eal_collector_contracts::strict_json_exact(&registration.stdout, Default::default())
            .map_err(Error::InvalidOutput)?;
    configuration::write_json(&dir.join("source-registration.json"), &registered)?;
    let host = HostConfig {
        executable: config.eal_host.clone(),
        workspace: request.workspace.clone(),
        database: Some(database),
        registry: Some(tools.clone()),
        collector_env: config.collector_environment.clone(),
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: Duration::from_secs(config.stage_timeout_seconds),
    };
    let plan = operation(
        &host,
        &json!({"operation":"plan","source":bundle["source"],"claim":claim}),
        &dir.join("plan.json"),
    )
    .await?;
    let registry_text = String::from_utf8(engarg::bounded_file::read(
        &tools,
        1_048_576,
        "ticket tools registry",
    )?)
    .map_err(|_| Error::InvalidInput("ticket registry requires UTF-8".into()))?;
    let registry = eal_collector_contracts::RegistryDocument::from_toml(&registry_text)
        .map_err(Error::InvalidInput)?;
    let missing = plan["calls"]
        .as_array()
        .ok_or_else(|| Error::InvalidOutput("EAL plan lacks collector calls".into()))?
        .iter()
        .filter_map(|call| call["tool"].as_str())
        .filter(|tool| !registry.tools.contains_key(*tool))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Ok(ResultData {
            artifacts: vec![artifact(
                "source_registration",
                &dir.join("source-registration.json"),
            )?],
            completed: false,
            outcome: json!({"complete":false,"phase":"execute","reason":"required_collector_binding_missing","missing_tools":missing}),
        });
    }
    let collection=operation(&host,&json!({"operation":"collect","source":bundle["source"],"context":bundle["context"],"evidence_ids":plan["evidence_ids"]}),&dir.join("evidence-collection.json")).await?;
    let known_envelope = engarg::eal_host::invoke_registered(
        &host,
        &json!({"operation":"assess_known","entry_id":entry,"claim":claim}),
        &entry,
    )
    .await?;
    configuration::write_json(&dir.join("registered-assessment.json"), &known_envelope)?;
    if known_envelope["is_error"] != false {
        return Err(Error::InvalidOutput(
            "registered ticket assessment failed; inspect retained envelope".into(),
        ));
    }
    let known = known_envelope["result"].clone();
    let assessment = operation(
        &host,
        &json!({"operation":"explain","assessment_id":known["assessment_id"]}),
        &dir.join("assessment.json"),
    )
    .await?;
    let valid = assessment["valid"] == true;
    let complete = selected_evidence_complete(&plan, &collection, &assessment);
    let artifacts = vec![
        artifact("source_registration", &dir.join("source-registration.json"))?,
        artifact("collection", &dir.join("evidence-collection.json"))?,
        artifact("assessment", &dir.join("assessment.json"))?,
        artifact(
            "registered_assessment",
            &dir.join("registered-assessment.json"),
        )?,
    ];
    Ok(ResultData {
        artifacts,
        completed: valid && complete,
        outcome: json!({"complete":complete,"valid":valid,"claim_status":known["status"],"entry_id":entry,"source_digest":known["source_digest"],"initial_collection_id":collection["collection_id"],"collection_id":known["collection_id"],"claim":claim,"assessment_id":known["assessment_id"],"reason":if complete{"assessed"}else{"required_evidence_unavailable"}}),
    })
}

fn selected_evidence_complete(plan: &Value, collection: &Value, assessment: &Value) -> bool {
    assessment["complete"] == true
        && plan["evidence_ids"].as_array().is_some_and(|ids| {
            ids.iter().all(|id| {
                id.as_str().is_some_and(|id| {
                    collection["records"].get(id).is_some()
                        && assessment["evidence"].get(id).is_some_and(|evidence| {
                            eal_coordination_contracts::ticket::evidence_is_measured(evidence)
                        })
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ticket_completion_uses_selected_evidence_and_keeps_negative_measurements() {
        let plan = json!({"evidence_ids":["required"]});
        let collection = json!({"records":{"required":{"status":"ok","value":false}}});
        let mut assessment = json!({"complete":true,"evidence":{"required":{"status":"available"},"unselected":{"status":"unavailable"}}});
        assert!(selected_evidence_complete(&plan, &collection, &assessment));
        assessment["evidence"]["required"]["status"] = json!("unavailable");
        assert!(!selected_evidence_complete(&plan, &collection, &assessment));
        assessment["evidence"]["required"]["availability_issues"] = json!(["predicate_not_met"]);
        assert!(selected_evidence_complete(&plan, &collection, &assessment));
        assessment["evidence"]["required"]["status"] = json!("available");
        assert!(!selected_evidence_complete(
            &plan,
            &json!({"records":{}}),
            &assessment
        ));
        assessment["complete"] = json!(false);
        assert!(!selected_evidence_complete(&plan, &collection, &assessment));
    }
}
