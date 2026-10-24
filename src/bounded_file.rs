//! Read bounded regular files without blocking on special files on POSIX hosts.

use crate::Error;
use std::{
    fs::{File, Metadata},
    io::Read,
    path::Path,
};

fn invalid(label: &str) -> Error {
    Error::InvalidInput(format!("{label} must be an unchanged regular file"))
}

#[cfg(unix)]
fn open(path: &Path) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::path::Component;
    let mut parent = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let parts: Vec<_> = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect();
    let (leaf, directories) = parts
        .split_last()
        .ok_or_else(|| std::io::Error::other("regular file required"))?;
    for directory in directories {
        parent = openat(
            &parent,
            Path::new(directory),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
    }
    let file = openat(
        &parent,
        Path::new(leaf),
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(file.into())
}

#[cfg(not(unix))]
fn open(path: &Path) -> std::io::Result<File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("regular file required"));
    }
    File::open(path)
}

fn same_file(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        before.dev() == after.dev() && before.ino() == after.ino()
    }
    #[cfg(not(unix))]
    {
        before.len() == after.len() && before.modified().ok() == after.modified().ok()
    }
}

/// Canonicalise the parent, then open every resolved component without following
/// links on POSIX. Validate the open file before reading and check path identity
/// afterwards. Error messages contain the caller's label without file contents.
pub(crate) fn read(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, Error> {
    let absolute = std::path::absolute(path).map_err(|_| invalid(label))?;
    let name = absolute.file_name().ok_or_else(|| invalid(label))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| invalid(label))?
        .canonicalize()
        .map_err(|_| invalid(label))?;
    let resolved = parent.join(name);
    let file = open(&resolved).map_err(|_| invalid(label))?;
    let metadata = file.metadata().map_err(|_| invalid(label))?;
    if !metadata.is_file() {
        return Err(invalid(label));
    }
    if metadata.len() > limit as u64 {
        return Err(Error::Budget(format!("{label} exceeds its byte limit")));
    }
    let mut bytes = Vec::new();
    file.take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| invalid(label))?;
    if bytes.len() > limit {
        return Err(Error::Budget(format!("{label} exceeds its byte limit")));
    }
    let after = std::fs::symlink_metadata(resolved).map_err(|_| invalid(label))?;
    if !after.is_file() || !same_file(&metadata, &after) {
        return Err(invalid(label));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_files_are_read_with_an_exact_byte_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inputs.json");
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(read(&path, 2, "inputs").unwrap(), b"{}");
        assert!(matches!(read(&path, 1, "inputs"), Err(Error::Budget(_))));
        assert!(read(directory.path(), 10, "inputs").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fifo_and_symlink_inputs_are_rejected_before_reading() {
        let directory = tempfile::tempdir().unwrap();
        let fifo = directory.path().join("inputs.fifo");
        // POSIX mkfifo is available on both Linux and macOS. rustix::mkfifoat
        // is absent on macOS even though the production openat reader works there.
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let alias = directory.path().join("inputs-link");
        std::os::unix::fs::symlink(&fifo, &alias).unwrap();
        for path in [fifo, alias] {
            let (send, receive) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || send.send(read(&path, 1024, "inputs")).unwrap());
            assert!(
                receive
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .expect("special files must be rejected without blocking")
                    .is_err()
            );
        }
    }
}
