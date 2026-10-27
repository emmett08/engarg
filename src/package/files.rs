//! Bounded package resource copying, without following paths outside its root.
use crate::Error;
use eal_collector_contracts::RegistryDocument;
pub(super) const MAX_BYTES: usize = 256 * 1024 * 1024;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn resources(
    root: &Path,
    manifest: &RegistryDocument,
) -> Result<BTreeSet<PathBuf>, Error> {
    let mut resources = BTreeSet::new();
    for binding in manifest.tools.values() {
        for path in binding
            .path
            .iter()
            .chain(binding.pinned_files.iter().map(|file| &file.path))
            .chain(binding.direct.iter().flat_map(|direct| {
                direct
                    .input_schema
                    .iter()
                    .chain(direct.output_schema.iter())
                    .map(|file| &file.path)
            }))
        {
            add(root, Path::new(path), &mut resources)?;
        }
        for argument in &binding.argv {
            if argument.starts_with("./")
                || argument.starts_with("tools/")
                || (!Path::new(argument).is_absolute() && root.join(argument).is_file())
            {
                add(root, Path::new(argument), &mut resources)?;
            }
        }
    }
    let tools = root.join("tools");
    if tools.exists() {
        visit(root, &tools, &mut resources, 0)?;
    }
    let mut bytes = 0u64;
    for resource in &resources {
        bytes = bytes.saturating_add(root.join(resource).metadata()?.len());
        if bytes > 256 * 1024 * 1024 || resources.len() > 4096 {
            return Err(Error::Budget(
                "Package resources exceed 256 MiB or 4096 files".into(),
            ));
        }
    }
    Ok(resources)
}

fn add(root: &Path, path: &Path, resources: &mut BTreeSet<PathBuf>) -> Result<(), Error> {
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::InvalidInput("Package data and pinned files must use relative paths within the tools manifest directory".into()));
    }
    let file = root.join(path);
    if !file.symlink_metadata()?.is_file() || !file.canonicalize()?.starts_with(root) {
        return Err(Error::InvalidInput(
            "Package resources must be regular files inside the manifest directory".into(),
        ));
    }
    let relative = path
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect::<PathBuf>();
    if matches!(
        relative.to_str(),
        Some("case.eal" | "tools.toml" | "build.json")
    ) {
        return Err(Error::InvalidInput(
            "A tool resource conflicts with a generated package file".into(),
        ));
    }
    resources.insert(relative);
    Ok(())
}

fn visit(
    root: &Path,
    directory: &Path,
    resources: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<(), Error> {
    if depth > 16 || resources.len() > 4096 {
        return Err(Error::Budget("Package resource tree exceeds bounds".into()));
    }
    if directory.symlink_metadata()?.file_type().is_symlink() {
        return Err(Error::InvalidInput(
            "Package resource directories cannot be symbolic links".into(),
        ));
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            visit(root, &path, resources, depth + 1)?;
        } else {
            add(
                root,
                path.strip_prefix(root).expect("rooted resource"),
                resources,
            )?;
        }
    }
    Ok(())
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn copy(
    root: &Path,
    manifest: &RegistryDocument,
    resources: BTreeSet<PathBuf>,
    publication: &mut super::publication::Publication,
) -> Result<BTreeMap<String, String>, Error> {
    let mut expected = BTreeMap::new();
    for binding in manifest.tools.values() {
        for pin in binding.pinned_files.iter().chain(
            binding
                .direct
                .iter()
                .flat_map(|d| d.input_schema.iter().chain(d.output_schema.iter())),
        ) {
            let key = Path::new(&pin.path)
                .components()
                .filter(|p| !matches!(p, Component::CurDir))
                .collect::<PathBuf>();
            if let Some(previous) = expected.insert(key, pin.sha256.as_str())
                && previous != pin.sha256
            {
                return Err(Error::InvalidInput(
                    "Package file has conflicting declared pins".into(),
                ));
            }
        }
    }
    let mut pins = BTreeMap::new();
    for relative in resources {
        let bytes = crate::bounded_file::read_within(
            root,
            &relative,
            publication.remaining(),
            "Package resource",
        )?;
        let identity = digest(&bytes);
        if expected
            .get(&relative)
            .is_some_and(|expected| *expected != identity)
        {
            return Err(Error::InvalidInput(
                "Package resource differs from its declared SHA-256 pin".into(),
            ));
        }
        let metadata = root.join(&relative).symlink_metadata()?;
        if !metadata.is_file() {
            return Err(Error::InvalidInput(
                "Package resource identity changed during copying".into(),
            ));
        }
        let mut permissions = metadata.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o600 | (permissions.mode() & 0o111));
        }
        publication.write(&relative, &bytes, Some(permissions))?;
        pins.insert(relative.to_string_lossy().into_owned(), identity);
    }
    Ok(pins)
}
