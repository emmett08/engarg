//! Execute the standalone package through the official EAL MCP operation.
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

fn verify_package_files(root: &Path, receipt: &Value) -> Result<(), Error> {
    let files = receipt["files"]
        .as_object()
        .ok_or_else(|| Error::InvalidInput("ticket build receipt lacks file identities".into()))?;
    if files.len() > 4098 {
        return Err(Error::Budget("ticket package has too many files".into()));
    }
    let mut remaining = 256 * 1_048_576;
    for (name, expected) in files {
        let bytes = engarg::bounded_file::read_within(
            root,
            Path::new(name),
            remaining,
            "ticket package input",
        )?;
        remaining -= bytes.len();
        if expected != &configuration::digest(&bytes) {
            return Err(Error::InvalidInput(
                "ticket package input changed after building".into(),
            ));
        }
    }
    Ok(())
}

pub(super) async fn run(
    request: &StageRequest,
    config: &Configuration,
) -> Result<ResultData, Error> {
    let dir = request.workspace.join("execute");
    directory(&dir)?;
    let receipt: Value = configuration::read(&previous(request, "build")?)?;
    let source = previous(request, "source")?;
    let source_bytes = engarg::bounded_file::read(&source, 16 * 1_048_576, "ticket source")?;
    let source_text = std::str::from_utf8(&source_bytes)
        .map_err(|_| Error::InvalidInput("ticket source requires UTF-8".into()))?;
    let tools = previous(request, "tools")?;
    verify_package_files(
        tools
            .parent()
            .ok_or_else(|| Error::InvalidInput("ticket tools have no directory".into()))?,
        &receipt,
    )?;
    let manifest_bytes = engarg::bounded_file::read(&tools, 1_048_576, "ticket tools")?;
    if receipt["files"]["case.eal"] != configuration::digest(&source_bytes)
        || receipt["files"]["tools.toml"] != configuration::digest(&manifest_bytes)
    {
        return Err(Error::InvalidInput(
            "ticket package differs from its retained build identity".into(),
        ));
    }
    let manifest = eal_collector_contracts::RegistryDocument::from_toml(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| Error::InvalidInput("ticket tools require UTF-8".into()))?,
    )
    .map_err(Error::InvalidInput)?;
    let claim = receipt["claims"]["current_claim"]
        .as_str()
        .ok_or_else(|| Error::InvalidOutput("ticket build has no selected claim".into()))?;
    let host = HostConfig {
        executable: config.eal_host.clone(),
        workspace: request.workspace.clone(),
        database: Some(request.workspace.join("evidence.sqlite")),
        registry: Some(tools),
        collector_env: config.collector_environment.clone(),
        public_collector_env: vec![],
        url: None,
        token_env: None,
        timeout: Duration::from_secs(config.stage_timeout_seconds),
    };
    let plan = operation(
        &host,
        &json!({"operation":"plan","source":source_text,"claim":claim}),
        &dir.join("plan.json"),
    )
    .await?;
    let execution = operation(&host, &json!({"operation":"execute","source":source_text,"context":manifest.context,"claim":claim}), &dir.join("execution.json")).await?;
    let collection = &execution["collection"];
    let assessment = &execution["assessment"];
    configuration::write_json(&dir.join("evidence-collection.json"), collection)?;
    configuration::write_json(&dir.join("assessment.json"), assessment)?;
    let valid = assessment["valid"] == true;
    let complete = selected_evidence_complete(&plan, collection, assessment);
    let status = &assessment["claims"][claim]["status"];
    Ok(ResultData {
        artifacts: vec![
            artifact("execution", &dir.join("execution.json"))?,
            artifact("collection", &dir.join("evidence-collection.json"))?,
            artifact("assessment", &dir.join("assessment.json"))?,
        ],
        completed: valid && complete,
        outcome: json!({"complete":complete,"valid":valid,"claim_status":status,
            "source_digest":execution["source_digest"],"executable_digest":execution["executable_digest"],
            "collection_id":collection["collection_id"],"claim":claim,"assessment_id":assessment["assessment_id"],
            "reason":if complete {"assessed"} else {"required_evidence_unavailable"}}),
    })
}

fn selected_evidence_complete(plan: &Value, collection: &Value, assessment: &Value) -> bool {
    assessment["complete"] == true
        && plan["evidence_ids"].as_array().is_some_and(|ids| {
            ids.iter().all(|id| {
                id.as_str().is_some_and(|id| {
                    collection["records"]
                        .get(id)
                        .is_some_and(|record| record["status"] == "ok")
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
    fn package_implementation_mutation_or_path_escape_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("source.py"), "original").unwrap();
        let receipt = json!({"files":{"source.py":configuration::digest(b"original")}});
        verify_package_files(root.path(), &receipt).unwrap();
        std::fs::write(root.path().join("source.py"), "changed").unwrap();
        assert!(verify_package_files(root.path(), &receipt).is_err());
        assert!(verify_package_files(root.path(), &json!({"files":{"../outside":"bad"}})).is_err());
    }

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
