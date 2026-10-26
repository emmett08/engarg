//! Register explicitly reviewed generated packages without model or source calls.
use clap::Args;
use engarg::{
    Error,
    report::{self, EvidencePlanningReport},
    route_planning::PlanningConfig,
    tooling::{
        capability::{ImplementedCapability, revalidate_implemented_capability, revalidate_report},
        contracts::digest_json,
        generated_registration::{
            native_provider_template, validate_provider_binding, validate_qualified_provider,
        },
    },
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub struct RegisterToolsArgs {
    /// Saved graph analysis retaining verified generated package receipts.
    #[arg(long)]
    input: PathBuf,
    /// Operator-reviewed generated-native-providers.json, including semantics.
    #[arg(long)]
    qualifications: PathBuf,
    /// Existing operator-owned capability planning configuration.
    #[arg(long)]
    evidence_config: PathBuf,
    /// New planning configuration; a sibling connections file is also retained.
    #[arg(long)]
    output: PathBuf,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn available(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(invalid(
            "registration output already exists; select a new output path",
        )),
    }
}
fn retain(path: &Path, value: &impl serde::Serialize) -> Result<(), Error> {
    let mut file = crate::private_file(path)?;
    write_json(&mut file, value)
}
fn write_json(file: &mut fs::File, value: &impl serde::Serialize) -> Result<(), Error> {
    serde_json::to_writer_pretty(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

/// Previously generated packages remain usable after a deterministic replan.
/// The caller validates the bounded provenance chain before this traversal.
pub(super) fn packages(
    raw: &Value,
    executable: &Path,
) -> Result<BTreeMap<String, ImplementedCapability>, Error> {
    let mut records = BTreeMap::<String, ImplementedCapability>::new();
    let mut current = raw;
    for _ in 0..=report::MAX_REPLANNING_HISTORY {
        if let Some(capabilities) = current
            .get("tooling")
            .filter(|v| !v.is_null())
            .map(|v| &v["capabilities"])
        {
            for item in capabilities
                .as_array()
                .ok_or_else(|| invalid("generated tooling has no capability records"))?
            {
                let record: ImplementedCapability = serde_json::from_value(item.clone())?;
                revalidate_implemented_capability(&record, executable)?;
                if let Some(previous) = records.get(&record.collector.id) {
                    if previous.capability_sha256 != record.capability_sha256
                        || previous.manifest_sha256 != record.manifest_sha256
                    {
                        return Err(invalid(
                            "generated collector identity has conflicting package receipts",
                        ));
                    }
                } else {
                    records.insert(record.collector.id.clone(), record);
                }
            }
        }
        let Some(previous) = current.get("replanning").filter(|v| !v.is_null()) else {
            return Ok(records);
        };
        current = &previous["previous_extensions"];
    }
    Err(invalid(
        "generated package provenance exceeds its history bound",
    ))
}

fn qualifications(raw: &Value) -> Result<&Vec<Value>, Error> {
    let object = raw
        .as_object()
        .ok_or_else(|| invalid("qualifications must be an evidence configuration object"))?;
    if raw["schema"] != "eal.evidence-config/1"
        || object.keys().any(|k| {
            !matches!(
                k.as_str(),
                "schema"
                    | "native_providers"
                    | "connections"
                    | "local_catalogues"
                    | "approved_catalogues"
                    | "discovery"
            )
        })
    {
        return Err(invalid("unsupported generated qualification configuration"));
    }
    for field in ["connections", "local_catalogues", "approved_catalogues"] {
        if let Some(value) = raw.get(field)
            && !value.as_array().is_some_and(Vec::is_empty)
        {
            return Err(invalid(
                "generated qualifications may only register verified native providers",
            ));
        }
    }
    let providers = raw["native_providers"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("no qualified generated providers were supplied"))?;
    if providers.len() > 256 {
        return Err(invalid("too many generated qualifications"));
    }
    Ok(providers)
}

fn inherit_env(provider: &Value) -> Result<BTreeSet<String>, Error> {
    provider["transport"]["inherit_env"]
        .as_array()
        .ok_or_else(|| invalid("generated command requires explicit environment references"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("environment references must be names"))
        })
        .collect()
}

fn merge_provider(
    connections: &mut Value,
    template: &Value,
    qualified: Value,
) -> Result<(), Error> {
    let id = qualified["capability"]["id"]
        .as_str()
        .ok_or_else(|| invalid("qualified capability has no identity"))?;
    if connections
        .get("local_catalogues")
        .and_then(Value::as_array)
        .is_some_and(|catalogues| {
            catalogues.iter().any(|c| {
                c["capabilities"]
                    .as_array()
                    .is_some_and(|entries| entries.iter().any(|c| c["id"] == id))
            })
        })
    {
        return Err(invalid(
            "generated capability identity conflicts with a local catalogue entry",
        ));
    }
    if connections.get("native_providers").is_none() {
        connections["native_providers"] = json!([]);
    }
    let providers = connections["native_providers"]
        .as_array_mut()
        .ok_or_else(|| invalid("existing native_providers must be an array"))?;
    let indices = providers
        .iter()
        .enumerate()
        .filter(|(_, p)| p["capability"]["id"] == id)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    match indices.as_slice() {
        [] => providers.push(qualified),
        [index] => {
            validate_provider_binding(template, &providers[*index])?;
            providers[*index] = qualified;
        }
        _ => {
            return Err(invalid(
                "existing configuration repeats the generated capability identity",
            ));
        }
    }
    Ok(())
}

pub async fn register_tools(args: RegisterToolsArgs) -> Result<(), Error> {
    let raw: Value = crate::read_json(&args.input, 16 * 1_048_576)?;
    let EvidencePlanningReport::Graph(_) = report::classify_evidence_planning(&raw)? else {
        return Err(invalid(
            "register-tools requires saved graph evidence planning",
        ));
    };
    let executable = std::env::current_exe()?.canonicalize()?;
    report::validate_replanning(&raw, &executable)?;
    revalidate_report(&raw, &executable)?;
    let packages = packages(&raw, &executable)?;
    let reviewed: Value = crate::read_json(&args.qualifications, 16 * 1_048_576)?;
    let providers = qualifications(&reviewed)?;
    let mut config: PlanningConfig = crate::read_json(&args.evidence_config, 4 * 1_048_576)?;
    config.validate()?;
    let base = args
        .evidence_config
        .canonicalize()?
        .parent()
        .ok_or_else(|| invalid("planning configuration has no parent"))?
        .to_owned();
    for path in [
        &mut config.collector,
        &mut config.connections,
        &mut config.store,
    ] {
        if !path.is_absolute() {
            *path = base.join(&*path);
        }
    }
    config.collector = config.collector.canonicalize()?;
    config.connections = config.connections.canonicalize()?;
    let mut connections: Value = crate::read_json(&config.connections, 16 * 1_048_576)?;
    if !connections.is_object() || connections["schema"] != "eal.evidence-config/1" {
        return Err(invalid(
            "unsupported existing evidence connections configuration",
        ));
    }
    let output = if args.output.is_absolute() {
        args.output
    } else {
        std::env::current_dir()?.join(args.output)
    };
    let output = output
        .parent()
        .ok_or_else(|| invalid("registration output has no directory"))?
        .canonicalize()?
        .join(
            output
                .file_name()
                .ok_or_else(|| invalid("registration output has no filename"))?,
        );
    let companion = output.with_extension("connections.json");
    let work = output.with_extension("registration");
    for path in [&output, &companion, &work] {
        available(path)?;
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&work)?;
    let supervisor = super::supervisor()?.canonicalize()?;
    let mut seen = BTreeSet::new();
    let mut environment = config.environment.iter().cloned().collect::<BTreeSet<_>>();
    let mut registered = Vec::new();
    for (index, qualified) in providers.iter().enumerate() {
        let id = qualified["capability"]["id"]
            .as_str()
            .ok_or_else(|| invalid("qualification requires a capability ID"))?;
        if !seen.insert(id.to_owned()) {
            return Err(invalid("qualification repeats a generated capability"));
        }
        let record = packages
            .get(id)
            .ok_or_else(|| invalid("qualification has no retained verified generated package"))?;
        let selected_environment = inherit_env(qualified)?;
        let template =
            native_provider_template(record, &executable, &supervisor, &selected_environment)?;
        validate_qualified_provider(&template, qualified)?;
        let prepared = super::native_provider::prepare(
            &config.collector,
            &supervisor,
            qualified,
            &work.join(format!("provider-{index}.request.json")),
        )
        .await?;
        validate_qualified_provider(&template, &prepared)?;
        environment.extend(selected_environment);
        registered.push(json!({"id":id,"package_manifest":record.package_manifest,"manifest_sha256":record.manifest_sha256,"qualification_sha256":digest_json(&prepared)?}));
        merge_provider(&mut connections, &template, prepared)?;
    }
    config.environment = environment.into_iter().collect();
    eal_collector_contracts::validate_environment_names(&config.environment, 64)
        .map_err(Error::InvalidInput)?;
    config.connections = companion.clone();
    config.validate()?;
    retain(
        &work.join("registration.json"),
        &json!({"schema":"engarg.generated-registration/1","input_report_sha256":digest_json(&raw)?,"registered":registered,"planning":output,"connections":companion,"model_calls":0,"source_calls":0,"qualification_truth_verified":false}),
    )?;
    // New files only; remove an incomplete pair rather than publishing half a config.
    let mut connections_file = crate::private_file(&companion)?;
    let mut output_file = match crate::private_file(&output) {
        Ok(file) => file,
        Err(error) => {
            drop(connections_file);
            let _ = fs::remove_file(&companion);
            return Err(error);
        }
    };
    let publish = (|| {
        write_json(&mut connections_file, &connections)?;
        write_json(&mut output_file, &config)
    })();
    if let Err(error) = publish {
        drop(output_file);
        drop(connections_file);
        let _ = fs::remove_file(&output);
        let _ = fs::remove_file(&companion);
        return Err(error);
    }
    println!(
        "{}",
        serde_json::to_string(
            &json!({"planning":output,"connections":companion,"registration":work.join("registration.json"),"model_calls":0,"source_calls":0})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qualification_file_cannot_introduce_other_authorities_or_duplicate_identity() {
        let mut pending = json!({"schema":"eal.evidence-config/1","native_providers":[{}],"connections":[],"local_catalogues":[],"approved_catalogues":[]});
        assert!(qualifications(&pending).is_ok());
        for field in ["connections", "local_catalogues", "approved_catalogues"] {
            pending[field] = json!([{}]);
            assert!(qualifications(&pending).is_err());
            pending[field] = json!([]);
        }
        pending["unexpected"] = json!(true);
        assert!(qualifications(&pending).is_err());
        let mut connections = json!({"native_providers":[{"capability":{"id":"generated"}},{"capability":{"id":"generated"}}]});
        assert!(
            merge_provider(
                &mut connections,
                &json!({}),
                json!({"capability":{"id":"generated"}})
            )
            .is_err()
        );
    }
}
