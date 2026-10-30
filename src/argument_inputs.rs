//! Freeze operator tool resources and runtime identity across argument investigations.
use engarg::{Error, optimal_argument::execution::ExecutionConfig};
use std::path::{Path, PathBuf};

/// Resolve an operator-selected host before any candidate changes the working directory.
pub(crate) fn host(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() || path.components().count() > 1 {
        return path.canonicalize().map_err(Error::from);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(path))
        .find(|path| path.is_file())
        .ok_or_else(|| Error::InvalidInput("selected eal-host not found on PATH".into()))?
        .canonicalize()
        .map_err(Error::from)
}

/// Reject outputs which would overwrite input state or enter the copied resource tree.
pub(crate) fn output(tools: &Path, output: &Path) -> Result<PathBuf, Error> {
    let output = crate::path_admission::identity(output)?;
    if output.symlink_metadata().is_ok() {
        return Err(Error::InvalidInput(
            "investigation output must be a new directory".into(),
        ));
    }
    let root = tools
        .parent()
        .ok_or_else(|| Error::InvalidInput("tools manifest has no parent".into()))?;
    if output.starts_with(crate::path_admission::identity(&root.join("tools"))?) {
        return Err(Error::InvalidInput(
            "investigation output must be outside the operator tools resource tree".into(),
        ));
    }
    Ok(output)
}

pub(crate) fn pins(paths: &[PathBuf]) -> Result<Vec<String>, Error> {
    paths
        .iter()
        .map(|path| {
            engarg::bounded_file::sha256(path, 256 * 1_048_576, "optimisation input")
                .map(|digest| digest.sha256)
        })
        .collect()
}

pub(crate) fn verify_inputs(
    config: &ExecutionConfig,
    paths: &[PathBuf],
    expected: &[String],
) -> Result<(), Error> {
    let mut current = engarg::package::input_files(&config.tools)?;
    current.push(config.eal_host.clone());
    if current != paths || pins(&current)? != expected {
        return Err(Error::InvalidInput(
            "optimisation tool inputs or host changed during the run".into(),
        ));
    }
    Ok(())
}
