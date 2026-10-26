//! Resolve prospective artifact destinations before model or coordinator effects.

use engarg::Error;
use std::path::{Component, Path, PathBuf};

/// Resolve existing ancestors, including symlinks, while retaining a new leaf.
/// Lexical parent components apply after existing symlink targets are resolved.
pub(crate) fn identity(path: &Path) -> Result<PathBuf, Error> {
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidInput(
            "artifact destination must be a nonempty path".into(),
        ));
    }
    let absolute = std::path::absolute(path)?;
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => resolved.push(prefix.as_os_str()),
            Component::RootDir => resolved.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                match std::fs::canonicalize(&resolved) {
                    Ok(existing) => resolved = existing,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(resolved)
}

pub(crate) fn analyse(
    output: Option<&Path>,
    audit: Option<&Path>,
    coordinator: Option<&Path>,
) -> Result<(), Error> {
    let output = output.map(identity).transpose()?;
    let audit = audit.map(identity).transpose()?;
    let coordinator = coordinator.map(identity).transpose()?;
    if let (Some(output), Some(audit)) = (&output, &audit)
        && (output.starts_with(audit) || audit.starts_with(output))
    {
        return Err(Error::InvalidInput(
            "output and audit require different paths in the filesystem".into(),
        ));
    }
    if let Some(directory) = coordinator {
        if output
            .as_ref()
            .is_some_and(|file| directory.starts_with(file) || file.starts_with(&directory))
        {
            return Err(Error::InvalidInput(
                "coordination directory overlaps the output file destination".into(),
            ));
        }
        if audit
            .as_ref()
            .is_some_and(|file| directory.starts_with(file) || file.starts_with(&directory))
        {
            return Err(Error::InvalidInput(
                "coordination directory overlaps the audit file destination".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prospective_aliases_and_file_directory_ancestors_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("report.json");
        let alias = directory.path().join("unused/../report.json");
        assert_eq!(identity(&output).unwrap(), identity(&alias).unwrap());
        assert!(analyse(Some(&output), None, Some(&alias)).is_err());
        assert!(analyse(Some(&output), None, Some(&output.join("journal"))).is_err());
        assert!(analyse(Some(&output.join("frame-0000-in.bin")), None, Some(&output)).is_err());
        assert!(analyse(Some(&output), Some(&alias), None).is_err());
        assert!(analyse(None, Some(&output.join("audit.jsonl")), Some(&output)).is_err());
        assert!(analyse(Some(&output), None, Some(&directory.path().join("session"))).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_existing_parents_resolve_to_the_same_new_destination() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert!(
            analyse(
                Some(&real.join("new.json")),
                None,
                Some(&alias.join("new.json"))
            )
            .is_err()
        );
    }
}
