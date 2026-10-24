//! Private, immutable snapshots passed into the container as read-only mounts.
use super::contracts::SourceFile;
use crate::Error;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Snapshot {
    root: PathBuf,
    pub(super) source: PathBuf,
    pub(super) fixtures: PathBuf,
}

impl Snapshot {
    pub(super) fn new(source: &[SourceFile], fixtures: &[SourceFile]) -> Result<Self, Error> {
        let base = std::env::temp_dir().canonicalize()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::InvalidInput("system clock predates the epoch".into()))?
            .as_nanos();
        let mut selected = None;
        for _ in 0..100 {
            let path = base.join(format!(
                ".engarg-tool-{}-{stamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match private_directory(&path) {
                Ok(()) => {
                    selected = Some(path);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        let root = selected.ok_or_else(|| {
            Error::InvalidInput("could not reserve a private collector snapshot".into())
        })?;
        let snapshot = Self {
            source: root.join("source"),
            fixtures: root.join("fixtures"),
            root,
        };
        for directory in [&snapshot.source, &snapshot.fixtures] {
            fs::create_dir(directory)?;
            public_directory(directory)?;
            mount_path(directory)?;
        }
        write_files(&snapshot.source, source)?;
        write_files(&snapshot.fixtures, fixtures)?;
        Ok(snapshot)
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn private_directory(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn public_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn mount_path(path: &Path) -> Result<&str, Error> {
    let text = path
        .to_str()
        .ok_or_else(|| Error::InvalidInput("collector mount requires UTF-8".into()))?;
    if !path.is_absolute()
        || text
            .chars()
            .any(|character| character.is_control() || matches!(character, ',' | ':'))
    {
        return Err(Error::InvalidInput(
            "collector mount path is not supported".into(),
        ));
    }
    Ok(text)
}

fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '\\' | ':'))
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn write_files(root: &Path, files: &[SourceFile]) -> Result<(), Error> {
    let mut total = 0usize;
    let mut paths = std::collections::BTreeSet::new();
    if files.len() > 64 {
        return Err(Error::Budget(
            "collector snapshot contains too many files".into(),
        ));
    }
    for file in files {
        total = total.saturating_add(file.content.len());
        if !safe_path(&file.path)
            || !paths.insert(file.path.to_ascii_lowercase())
            || file.content.len() > 1_048_576
            || total > 2_097_152
        {
            return Err(Error::InvalidInput(
                "collector snapshot files exceed path or byte limits".into(),
            ));
        }
        let path = root.join(&file.path);
        let parent = path.parent().expect("relative file has parent");
        fs::create_dir_all(parent)?;
        let mut ancestor = Some(parent);
        while let Some(directory) = ancestor {
            if !directory.starts_with(root) {
                break;
            }
            public_directory(directory)?;
            if directory == root {
                break;
            }
            ancestor = directory.parent();
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        output.write_all(file.content.as_bytes())?;
        output.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output.set_permissions(fs::Permissions::from_mode(0o444))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_contain_exact_files_and_reject_traversal() {
        let snapshot = Snapshot::new(
            &[SourceFile {
                path: "nested/collector.py".into(),
                content: "print(1)".into(),
            }],
            &[SourceFile {
                path: "response.json".into(),
                content: "{}".into(),
            }],
        )
        .unwrap();
        let root = snapshot.root.clone();
        assert_eq!(
            fs::read_to_string(snapshot.source.join("nested/collector.py")).unwrap(),
            "print(1)"
        );
        assert_eq!(
            fs::read_to_string(snapshot.fixtures.join("response.json")).unwrap(),
            "{}"
        );
        drop(snapshot);
        assert!(!root.exists());
        assert!(
            Snapshot::new(
                &[SourceFile {
                    path: "../escape".into(),
                    content: "".into()
                }],
                &[]
            )
            .is_err()
        );
    }
}
