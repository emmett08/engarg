//! Combine authorised bindings and verified exports without changing their authority.
use super::{files, load_manifest, publication::Publication};
use crate::Error;
use eal_collector_contracts::RegistryBinding;
use std::path::{Path, PathBuf};

fn prefixed(prefix: &Path, path: &str) -> Result<String, Error> {
    prefix
        .join(path)
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::InvalidInput("tool resource path must be UTF-8".into()))
}

fn rebase(binding: &mut RegistryBinding, root: &Path, prefix: &Path) -> Result<(), Error> {
    for argument in &mut binding.argv {
        if !Path::new(argument).is_absolute() && root.join(&*argument).is_file() {
            *argument = prefixed(prefix, argument)?;
        }
    }
    if let Some(path) = &mut binding.path {
        *path = prefixed(prefix, path)?;
    }
    for pin in &mut binding.pinned_files {
        pin.path = prefixed(prefix, &pin.path)?;
    }
    if let Some(direct) = &mut binding.direct {
        for pin in direct
            .input_schema
            .iter_mut()
            .chain(&mut direct.output_schema)
        {
            pin.path = prefixed(prefix, &pin.path)?;
        }
    }
    Ok(())
}

/// Publish a fresh manifest tree. Existing bindings keep their command, endpoint,
/// environment grants and package-relative data. Each generated export is copied
/// under its own operation directory with its original source and schema pins.
pub fn merge_tool_exports(
    original: &Path,
    exports: &[(String, PathBuf)],
    output: &Path,
) -> Result<PathBuf, Error> {
    let (original_path, mut manifest) = load_manifest(original)?;
    manifest.validate().map_err(Error::InvalidInput)?;
    let root = original_path.parent().expect("canonical manifest parent");
    let resources = files::resources(root, &manifest)?;
    let mut publication = Publication::new(output)?;
    let mut pins = files::copy(root, &manifest, resources, &mut publication)?;
    for (operation, exported) in exports {
        if !eal_collector_contracts::identifier(operation) {
            return Err(Error::InvalidInput(
                "generated operation requires an identifier".into(),
            ));
        }
        let (path, selected) = load_manifest(exported)?;
        selected.validate().map_err(Error::InvalidInput)?;
        if selected.tools.len() != 1 || !selected.context.is_empty() {
            return Err(Error::InvalidInput(
                "verified tool export must contain one context-free binding".into(),
            ));
        }
        let mut binding = selected.tools.get(operation).cloned().ok_or_else(|| {
            Error::InvalidInput("verified tool export differs from its selected operation".into())
        })?;
        if manifest.tools.contains_key(operation)
            || manifest
                .tools
                .contains_key(&crate::support_export::operation_tool_name(operation))
        {
            return Err(Error::InvalidInput(
                "generated tool cannot replace an authorised binding".into(),
            ));
        }
        let selected_root = path.parent().expect("canonical export parent");
        let prefix = Path::new("tools").join("generated").join(operation);
        pins.extend(files::copy_prefixed(
            selected_root,
            &selected,
            files::resources(selected_root, &selected)?,
            &mut publication,
            &prefix,
        )?);
        rebase(&mut binding, selected_root, &prefix)?;
        manifest.tools.insert(operation.clone(), binding);
    }
    files::pin_implementations(&mut manifest, &pins)?;
    let bytes = toml::to_string_pretty(&manifest)
        .map_err(|error| Error::InvalidOutput(error.to_string()))?;
    publication.write(Path::new("tools.toml"), bytes.as_bytes(), None)?;
    publication.publish()?;
    Ok(output.join("tools.toml").canonicalize()?)
}
