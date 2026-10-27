//! Export verified implementations as directly executable EAL tool packages.
use super::{
    package::load_verified, runner::DOCKER_ENVIRONMENT,
    verification::requires_original_observation_time,
};
use crate::Error;
use eal_collector_contracts::PinnedFile;
use eal_tool_runtime::package::{PACKAGE_SCHEMA, ToolPackage, digest};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct ToolExport {
    pub tool_id: String,
    pub package: PathBuf,
    pub tools: PathBuf,
}
fn write_new(
    publication: &mut crate::package::publication::Publication,
    path: &str,
    bytes: &[u8],
) -> Result<PinnedFile, Error> {
    publication.write(Path::new(path), bytes, None)?;
    Ok(PinnedFile {
        path: path.into(),
        sha256: digest(bytes),
    })
}
#[derive(Serialize)]
struct Tools {
    tools: BTreeMap<String, Binding>,
}
#[derive(Serialize)]
struct Binding {
    kind: &'static str,
    version: &'static str,
    argv: Vec<String>,
    inherit_env: Vec<String>,
    timeout_seconds: u64,
    max_output_bytes: usize,
    parallel_safe: bool,
    pinned_files: Vec<PinnedFile>,
}
/// The output directory is new, private and self-contained. Export does not execute code.
pub fn export_verified(
    package: &Path,
    output: &Path,
    tool_id: Option<&str>,
) -> Result<ToolExport, Error> {
    let verified = load_verified(package)?;
    let tool_id = tool_id
        .map(str::to_owned)
        .unwrap_or_else(|| verified.acquisition_plan().tool_id);
    if !eal_collector_contracts::identifier(&tool_id) {
        return Err(Error::InvalidInput(
            "export tool ID must be an EAL identifier".into(),
        ));
    }
    if output.exists() {
        return Err(Error::InvalidInput(
            "tool export requires a new output directory".into(),
        ));
    }
    let mut publication = crate::package::publication::Publication::new(output)?;
    let mut source = Vec::new();
    let mut files = Vec::new();
    for file in &verified.candidate().files {
        let pin = write_new(
            &mut publication,
            &format!("source/{}", file.path),
            file.content.as_bytes(),
        )?;
        source.push(PinnedFile {
            path: file.path.clone(),
            sha256: pin.sha256.clone(),
        });
        files.push(pin);
    }
    let input_schema = write_new(
        &mut publication,
        "input.schema.json",
        &serde_json::to_vec_pretty(&verified.contract().draft.input_schema)?,
    )?;
    let output_schema = write_new(
        &mut publication,
        "output.schema.json",
        &serde_json::to_vec_pretty(&verified.contract().draft.result_schema)?,
    )?;
    // Keep authoring identity/fixture results as provenance, without requiring its executable on the new host.
    let verification = write_new(
        &mut publication,
        "verification.json",
        &serde_json::to_vec_pretty(verified.receipt())?,
    )?;
    files.extend([
        input_schema.clone(),
        output_schema.clone(),
        verification.clone(),
    ]);
    let manifest = ToolPackage {
        schema: PACKAGE_SCHEMA.into(),
        tool: tool_id.clone(),
        version: "1".into(),
        image_id: verified.identity().image_id.clone(),
        require_observed_at: requires_original_observation_time(verified.contract()),
        profile: verified.profile().clone(),
        source,
        input_schema,
        output_schema,
        verification,
    };
    let text =
        toml::to_string_pretty(&manifest).map_err(|e| Error::InvalidOutput(e.to_string()))?;
    files.push(write_new(&mut publication, "tool.toml", text.as_bytes())?);
    let mut inherit_env = DOCKER_ENVIRONMENT
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<Vec<_>>();
    for name in &manifest.profile.inherit_env {
        if !inherit_env.contains(name) {
            inherit_env.push(name.clone());
        }
    }
    let binding = Binding {
        kind: "command",
        version: "1",
        argv: vec![
            "eal-tool-run".into(),
            "--package".into(),
            "tool.toml".into(),
        ],
        inherit_env,
        timeout_seconds: manifest.profile.timeout_seconds + 60,
        max_output_bytes: manifest.profile.output_bytes,
        parallel_safe: true,
        pinned_files: files,
    };
    let tools = Tools {
        tools: BTreeMap::from([(tool_id.clone(), binding)]),
    };
    write_new(
        &mut publication,
        "tools.toml",
        toml::to_string_pretty(&tools)
            .map_err(|e| Error::InvalidOutput(e.to_string()))?
            .as_bytes(),
    )?;
    let instructions = format!(
        "# Executable tool {tool_id}\n\nInstall `eal-tool-run` and its `eal-process-supervisor` companion from the same EAL release. Install Docker and load the already verified immutable image `{}`. The runner never pulls images.\n\n`tools.toml` is the direct EAL command binding. Copy this directory as a unit; paths are relative to the TOML directory. Run an EAL file with that binding through `eal-host run CASE.eal --tools tools.toml --workspace RUN`.\n\nThe original source is in `source/`, schemas are pinned, and `verification.json` retains the authoring fixture results and original execution identity. Runtime uses the same isolated build/run arguments, limits, environment name grants and network policy in `tool.toml`; it does not assert that this host has the original authoring executable identity.\n\nBuild occurs inside the pinned image before each run using `profile.build_argv`, with `/src` read only and `/build` temporary. Dependencies and compilers belong to that image. No EngArg executable, registry or model is needed. Changing pinned files requires intentional re-export or a new operator-reviewed deployment binding.\n",
        manifest.image_id
    );
    write_new(&mut publication, "README.md", instructions.as_bytes())?;
    ToolPackage::load(&publication.root().join("tool.toml"))?;
    publication.publish()?;
    Ok(ToolExport {
        tool_id,
        package: output.join("tool.toml"),
        tools: output.join("tools.toml"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooling::{
        contracts::digest_json,
        package::PackageStore,
        profile::ToolchainProfile,
        verification::tests::{candidate, contract, identity, passing_receipt},
    };
    fn retained(root: &Path) -> PathBuf {
        let store = PackageStore::new(root).unwrap();
        let contract = contract();
        let candidate = candidate();
        let profile = ToolchainProfile {
            schema: super::super::profile::TOOLCHAIN_SCHEMA.into(),
            language: "python".into(),
            docker: "docker".into(),
            image: "python:3.13-slim".into(),
            build_argv: vec![],
            run_argv: vec!["python3".into(), "/src/collector.py".into()],
            inherit_env: vec![],
            runtime_network: "none".into(),
            timeout_seconds: 30,
            memory_mb: 128,
            output_bytes: 1_048_576,
        };
        let mut identity = identity();
        identity.profile_sha256 = digest_json(&profile).unwrap();
        let key = PackageStore::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            &identity,
        )
        .unwrap();
        store.freeze_contract(&key, &contract, &identity).unwrap();
        let attempt = store.new_attempt(&key).unwrap();
        store
            .save_candidate(&attempt, &contract, &candidate, &profile, &identity)
            .unwrap();
        let receipt = passing_receipt(identity.clone());
        std::fs::write(
            attempt.join("test-results.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        store
            .commit_verified(
                &key, &attempt, contract, candidate, profile, identity, receipt,
            )
            .unwrap()
            .manifest_path()
            .to_path_buf()
    }
    #[test]
    fn verified_export_moves_with_all_pins_and_no_engarg_execution_dependency() {
        let root = tempfile::tempdir().unwrap();
        let verified = retained(&root.path().join("authoring"));
        let output = root.path().join("exported");
        let result = export_verified(&verified, &output, Some("catalogue")).unwrap();
        assert_eq!(result.tool_id, "catalogue");
        let manifest: eal_collector_contracts::RegistryDocument =
            toml::from_str(&std::fs::read_to_string(&result.tools).unwrap()).unwrap();
        let binding = &manifest.tools["catalogue"];
        assert_eq!(binding.argv, vec!["eal-tool-run", "--package", "tool.toml"]);
        for pin in &binding.pinned_files {
            assert_eq!(
                digest(&std::fs::read(output.join(&pin.path)).unwrap()),
                pin.sha256
            );
        }
        let moved = root.path().join("moved");
        std::fs::rename(&output, &moved).unwrap();
        let package = ToolPackage::load(&moved.join("tool.toml")).unwrap();
        assert_eq!(package.manifest.tool, "catalogue");
        let request=serde_json::to_vec(&serde_json::json!({"evidence_id":"e","environment":"local","tool":"different_operator_alias","tool_version":"1","input":{"mode":"negative"},"context":{}})).unwrap();
        assert!(package.validate_request(&request).is_ok());
        assert!(
            !std::fs::read_to_string(moved.join("tools.toml"))
                .unwrap()
                .contains("engarg")
        );
        assert!(export_verified(&verified, &moved, None).is_err());
    }
    #[test]
    fn a_modified_verified_source_is_rejected_before_publishing_any_export() {
        let root = tempfile::tempdir().unwrap();
        let verified = retained(&root.path().join("authoring"));
        std::fs::write(
            verified.parent().unwrap().join("source/collector.py"),
            "different implementation",
        )
        .unwrap();
        let output = root.path().join("rejected");
        assert!(export_verified(&verified, &output, None).is_err());
        assert!(!output.exists());
    }
}
