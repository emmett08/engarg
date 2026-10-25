//! Pure collector-side native configuration preparation, without source calls.
use engarg::{
    Error,
    tooling::{contracts::digest_json, generated_registration::canonical_mapping_defaults},
};
use serde_json::Value;
use std::{collections::BTreeMap, io::Write, path::Path, time::Duration};

// Qualification structs deliberately accept omitted optional fields. Compare
// their shared typed representation so serializing those defaults (for example
// coverage.sample_size_pointer:null) cannot look like a changed qualification.
// Execution authorities, package pins and opaque schemas remain in the exact
// comparison; no native hash calculation is copied into EngArg.
fn preparation_identity(value: &Value) -> Result<Value, Error> {
    let mut normalized = canonical_mapping_defaults(value)?;
    let mut capability: eal_collector_contracts::capability::CapabilityDescriptor =
        serde_json::from_value(value["capability"].clone())?;
    capability.validate().map_err(Error::InvalidInput)?;
    capability.provider.configuration_sha256 = "0".repeat(64);
    normalized["capability"] = serde_json::to_value(capability)?;
    Ok(normalized)
}

fn validate_prepared(input: &Value, prepared: &Value) -> Result<(), Error> {
    if digest_json(&preparation_identity(input)?)? != digest_json(&preparation_identity(prepared)?)?
    {
        return Err(Error::InvalidOutput("native preparation changed fields other than the configuration identity or serialized contract defaults".into()));
    }
    Ok(())
}

pub async fn prepare(
    collector: &Path,
    supervisor: &Path,
    input: &Value,
    path: &Path,
) -> Result<Value, Error> {
    // Canonicalize accepted absent-only mapping defaults before sending the
    // operator's request, rather than repairing the collector's returned file.
    let input = canonical_mapping_defaults(input)?;
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, &input)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    let cancellation = eal_process::CancellationToken::default();
    struct Cancel(eal_process::CancellationToken);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let _guard = Cancel(cancellation.clone());
    let command = eal_process::CommandRequest {
        argv: vec![
            collector.to_string_lossy().into_owned(),
            "prepare-native-provider".into(),
            "--input".into(),
            path.to_string_lossy().into_owned(),
        ],
        environment: BTreeMap::new(),
        cwd: path.parent().unwrap_or(Path::new(".")).to_owned(),
        input: Vec::new(),
        timeout: Duration::from_secs(30),
        max_output_bytes: 4 * 1_048_576,
    };
    let supervisor = supervisor.to_owned();
    let outcome = tokio::task::spawn_blocking(move || {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            command,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: 4 * 1_048_576,
                max_stderr_bytes: 65536,
            },
        )
    })
    .await
    .map_err(|_| Error::Provider("native registration preparation worker failed".into()))?
    .map_err(|_| Error::Provider("native registration preparation could not start".into()))?;
    let response = path.with_extension("response.json");
    let mut retained = crate::private_file(&response)?;
    retained.write_all(&outcome.stdout)?;
    retained.sync_all()?;
    if outcome.returncode != 0 || outcome.execution_error.is_some() {
        return Err(Error::InvalidInput(format!(
            "collector rejected native registration; complete bounded response retained at {}. Use a matching collector supporting prepare-native-provider and valid operator qualification",
            response.display()
        )));
    }
    let prepared = eal_collector_contracts::strict_json_exact(
        &outcome.stdout,
        eal_collector_contracts::JsonLimits {
            max_bytes: 4 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    validate_prepared(&input, &prepared)?;
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eal_collector_contracts::capability::schema_identity;
    use serde_json::json;

    fn qualified() -> Value {
        let input = json!({"type":"object","properties":{"mode":{"type":"string"}},"required":["mode"],"additionalProperties":false});
        let output = json!({"type":"object","properties":{"collected":{"type":"boolean"},"present":{"type":"boolean"}},"required":["collected","present"],"additionalProperties":false});
        let estimate = json!({"value":null,"basis":"unknown","reference":"synthetic qualification regression"});
        json!({"capability":{"id":"generated_fixture","version":"1","description":"Synthetic function only","provider":{"id":"generated_fixture","version":"1","transport":"command","connection":null,"tool":null,"authentication_scope":"public","configuration_sha256":"0".repeat(64)},"input_schema":input,"input_schema_sha256":schema_identity(&input).unwrap(),"output_schema":output,"output_schema_sha256":schema_identity(&output).unwrap(),
        "semantics":{"qualification_reference":"synthetic-review/1","support_types":["observation"],"subject_kinds":["synthetic_flag"],"methods":["synthetic_flag/1"],"identity_dimensions":["mode"],"scope_constraints":{},"scope_bindings":{"mode":"/mode"},"quantities":[{"name":"present","unit":"boolean","output_pointer":"/present"}],"coverage":[{"strength":"existential_witness","population":null,"completeness_pointer":null}],"time":{"historical_intervals":false,"historical_input":null,"original_observation_time":false,"uncertainty_seconds":0},"provenance":{"authorities":["fixture"],"transformation_history":true},"limitations":["Synthetic regression only."]},"configuration_requirements":[],"limits":{"timeout_seconds":30,"output_bytes":1048576,"concurrency":1,"requests_per_minute":null},"costs":{"configuration_effort_seconds":estimate,"implementation_effort_seconds":estimate,"execution_requests":estimate,"execution_latency_ms":estimate,"monetary_microunits":estimate,"transfer_bytes":estimate}},
        "transport":{"kind":"command","program":"/operator/collector","supervisor":"/operator/supervisor","arguments":[],"cwd":"/operator","inherit_env":[],"protocol":{"kind":"eal_acquisition","environment":"synthetic","tool":"generated_fixture","tool_version":"1","context":{}}},
        "mapping":{"tool":"generated_fixture","raw_output_schema_sha256":null,"response_format":{"kind":"structured"},"value_pointer":"/value","observation_time":{"kind":"collection_time"},"scope_pointers":{"mode":"/request/input/mode"}},"pinned_files":[],"credential_env":[]})
    }

    #[test]
    fn serialized_qualification_defaults_are_equal_but_source_and_schema_changes_are_rejected() {
        let mut input = qualified();
        input["capability"]["input_schema"]["x-opaque"] =
            engarg::tooling::verification::protocol_precision_context().unwrap();
        input["capability"]["input_schema_sha256"] =
            json!(schema_identity(&input["capability"]["input_schema"]).unwrap());
        let mut prepared = input.clone();
        prepared["capability"] = preparation_identity(&input).unwrap()["capability"].clone();
        prepared["capability"]["provider"]["configuration_sha256"] = json!("a".repeat(64));
        assert!(
            input["capability"]["semantics"]["coverage"][0]
                .get("sample_size_pointer")
                .is_none()
        );
        assert_eq!(
            prepared["capability"]["semantics"]["coverage"][0]["sample_size_pointer"],
            Value::Null
        );
        validate_prepared(&input, &prepared).unwrap();
        assert_eq!(
            prepared["capability"]["input_schema"],
            input["capability"]["input_schema"]
        );
        for pointer in [
            "/transport/program",
            "/transport/protocol/context",
            "/mapping/value_pointer",
            "/capability/provider/id",
            "/capability/input_schema",
            "/capability/input_schema_sha256",
            "/capability/semantics/qualification_reference",
        ] {
            let mut changed = prepared.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("changed");
            assert!(validate_prepared(&input, &changed).is_err(), "{pointer}");
        }
    }

    #[test]
    fn mapping_defaults_are_absent_only_and_do_not_prune_execution_or_supplied_fields() {
        let mut omitted = qualified();
        let mapping = omitted["mapping"].as_object_mut().unwrap();
        mapping.remove("raw_output_schema_sha256");
        mapping.remove("scope_pointers");
        let explicit = canonical_mapping_defaults(&omitted).unwrap();
        assert!(explicit["mapping"]["raw_output_schema_sha256"].is_null());
        assert_eq!(explicit["mapping"]["scope_pointers"], json!({}));
        assert!(omitted["mapping"].get("scope_pointers").is_none());
        assert_eq!(explicit["transport"], omitted["transport"]);
        assert_eq!(explicit["pinned_files"], omitted["pinned_files"]);
        let mut supplied = explicit.clone();
        supplied["mapping"]["scope_pointers"] = Value::Null;
        supplied["mapping"]["unknown"] = json!({"keep":"for authoritative rejection"});
        assert_eq!(canonical_mapping_defaults(&supplied).unwrap(), supplied);
        let mut changed = explicit.clone();
        changed["mapping"]["raw_output_schema_sha256"] = json!("a".repeat(64));
        assert!(validate_prepared(&omitted, &changed).is_err());
        let mut changed = explicit;
        changed["mapping"]["scope_pointers"] = json!({"mode":"/changed"});
        assert!(validate_prepared(&omitted, &changed).is_err());
    }

    #[tokio::test]
    #[ignore = "requires installed matching EAL_COLLECTOR; no Docker or evidence calls"]
    async fn native_qualification_preparation_accepts_real_collector_default_expansion_without_source_calls()
     {
        let collector = std::path::PathBuf::from(
            std::env::var_os("EAL_COLLECTOR").expect("EAL_COLLECTOR is required"),
        )
        .canonicalize()
        .unwrap();
        let supervisor = collector.with_file_name(if cfg!(windows) {
            "eal-process-supervisor.exe"
        } else {
            "eal-process-supervisor"
        });
        let directory = tempfile::tempdir().unwrap();
        let input = qualified();
        let prepared = prepare(
            &collector,
            &supervisor,
            &input,
            &directory.path().join("qualified.request.json"),
        )
        .await
        .unwrap();
        assert_ne!(
            prepared["capability"]["provider"]["configuration_sha256"],
            input["capability"]["provider"]["configuration_sha256"]
        );
        assert!(
            prepared["capability"]["semantics"]["coverage"][0]
                .get("sample_size_pointer")
                .unwrap()
                .is_null()
        );
        validate_prepared(&input, &prepared).unwrap();
        assert_eq!(prepared["transport"]["program"], "/operator/collector");

        let mut omitted = qualified();
        omitted["capability"]["semantics"]["identity_dimensions"] = json!([]);
        omitted["capability"]["semantics"]["scope_bindings"] = json!({});
        let mapping = omitted["mapping"].as_object_mut().unwrap();
        mapping.remove("raw_output_schema_sha256");
        mapping.remove("scope_pointers");
        let request = directory.path().join("omitted.request.json");
        let prepared = prepare(&collector, &supervisor, &omitted, &request)
            .await
            .unwrap();
        assert!(prepared["mapping"]["raw_output_schema_sha256"].is_null());
        assert_eq!(prepared["mapping"]["scope_pointers"], json!({}));
        validate_prepared(&omitted, &prepared).unwrap();
        let retained: Value = serde_json::from_slice(&std::fs::read(&request).unwrap()).unwrap();
        assert!(
            retained["mapping"]
                .get("raw_output_schema_sha256")
                .unwrap()
                .is_null()
        );
        assert_eq!(retained["mapping"]["scope_pointers"], json!({}));
        assert!(omitted["mapping"].get("scope_pointers").is_none());
    }
}
