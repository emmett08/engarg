//! Build portable EAL source and direct tool bindings from an authored argument.
mod files;
pub(crate) mod publication;
mod tools;
pub use tools::merge_tool_exports;

use crate::{
    Error,
    domain::{ArgumentCase, Request},
    export_policy::ExportPolicy,
    support_requirements::SupportElaboration,
};
use eal_collector_contracts::RegistryDocument;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Compile only authored obligations and explicit operator bindings. Discovery
/// and source qualification are optional authoring inputs, never runtime files.
pub fn build(
    request: &Request,
    case: &ArgumentCase,
    elaboration: &SupportElaboration,
    tools: &Path,
    output: &Path,
    policy: &ExportPolicy,
) -> Result<Value, Error> {
    if output.symlink_metadata().is_ok() {
        return Err(Error::InvalidInput(
            "Package output must be a new directory".into(),
        ));
    }
    let (manifest_path, mut manifest) = load_manifest(tools)?;
    let root = manifest_path
        .parent()
        .ok_or_else(|| Error::InvalidInput("Tools manifest has no parent".into()))?;
    let mut versions = BTreeMap::new();
    for acquisition in &elaboration.acquisitions {
        let emitted = crate::support_export::operation_tool_name(&acquisition.id);
        let binding = match (
            manifest.tools.remove(&acquisition.id),
            manifest.tools.remove(&emitted),
        ) {
            (Some(_), Some(_)) => {
                return Err(Error::InvalidInput(format!(
                    "Duplicate tool binding for {}",
                    acquisition.id
                )));
            }
            (Some(binding), None) | (None, Some(binding)) => binding,
            (None, None) => {
                return Err(Error::InvalidInput(format!(
                    "Missing direct tool binding for {}",
                    acquisition.id
                )));
            }
        };
        versions.insert(acquisition.id.clone(), binding.version.clone());
        manifest.tools.insert(emitted, binding);
    }
    let compiled =
        crate::support_export::compile_direct(request, case, elaboration, &versions, policy)?;
    for (key, value) in compiled
        .bundle
        .context
        .as_object()
        .ok_or_else(|| Error::InvalidInput("Compiler context must be an object".into()))?
    {
        if let Some(supplied) = manifest.context.get(key) {
            if supplied != value {
                return Err(Error::InvalidInput(format!(
                    "Operator context conflicts with compiled argument identity at {key}"
                )));
            }
        } else {
            manifest.context.insert(key.clone(), value.clone());
        }
    }
    manifest.validate().map_err(Error::InvalidInput)?;
    let program = eal_core::composition::compile(
        &compiled.bundle.source,
        None,
        "",
        eal_core::ExecutionLimits::default(),
    )
    .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let executable_digest = eal_core::semantic_format::executable_digest(
        &program,
        eal_core::methods::default_registry(),
    )
    .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let resources = files::resources(root, &manifest)?;
    let mut publication = publication::Publication::new(output)?;
    let mut pins = files::copy(root, &manifest, resources, &mut publication)?;
    files::pin_implementations(&mut manifest, &pins)?;
    let tools_toml = toml::to_string_pretty(&manifest)
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    publication.write(
        Path::new("case.eal"),
        compiled.bundle.source.as_bytes(),
        None,
    )?;
    publication.write(Path::new("tools.toml"), tools_toml.as_bytes(), None)?;
    pins.insert(
        "case.eal".into(),
        files::digest(compiled.bundle.source.as_bytes()),
    );
    pins.insert("tools.toml".into(), files::digest(tools_toml.as_bytes()));
    let receipt = json!({
        "schema":"engarg.executable-package/1",
        "language":eal_core::LANGUAGE,
        "source_digest":program.source_digest,
        "executable_digest":executable_digest,
        "files":pins,
        "claims":compiled.bundle.claim_names,
        "pending_reviews":compiled.review_requests,
        "limitations":compiled.bundle.limitations,
        "model_calls":0
    });
    publication.write(
        Path::new("build.json"),
        &serde_json::to_vec_pretty(&receipt)?,
        None,
    )?;
    publication.publish()?;
    Ok(receipt)
}

/// Identify package inputs for a caller that must freeze them before authoring.
/// This performs the same manifest and resource admission as the build itself.
pub fn input_files(tools: &Path) -> Result<Vec<PathBuf>, Error> {
    let (manifest_path, manifest) = load_manifest(tools)?;
    let root = manifest_path
        .parent()
        .ok_or_else(|| Error::InvalidInput("Tools manifest has no parent".into()))?;
    let mut paths = vec![manifest_path.clone()];
    paths.extend(
        files::resources(root, &manifest)?
            .into_iter()
            .map(|relative| root.join(relative)),
    );
    Ok(paths)
}

fn load_manifest(tools: &Path) -> Result<(PathBuf, RegistryDocument), Error> {
    let path = tools.canonicalize()?;
    let text = String::from_utf8(crate::bounded_file::read(
        &path,
        1_048_576,
        "Tools manifest",
    )?)
    .map_err(|_| Error::InvalidInput("Tools manifest must be UTF-8".into()))?;
    let manifest = RegistryDocument::from_toml(&text).map_err(Error::InvalidInput)?;
    Ok((path, manifest))
}
