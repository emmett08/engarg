//! Read bounded regular files without blocking on special files on POSIX hosts.

use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata},
    io::Read,
    path::Path,
};

fn invalid(label: &str) -> Error {
    Error::InvalidInput(format!(
        "{label} must be a regular file with stable path identity"
    ))
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
fn consume<T>(
    path: &Path,
    limit: usize,
    label: &str,
    action: impl FnOnce(&mut dyn Read) -> std::io::Result<(T, u64)>,
) -> Result<T, Error> {
    let absolute = std::path::absolute(path).map_err(|_| invalid(label))?;
    let name = absolute.file_name().ok_or_else(|| invalid(label))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| invalid(label))?
        .canonicalize()
        .map_err(|_| invalid(label))?;
    let resolved = parent.join(name);
    let mut file = open(&resolved).map_err(|_| invalid(label))?;
    let metadata = file.metadata().map_err(|_| invalid(label))?;
    if !metadata.is_file() {
        return Err(invalid(label));
    }
    if metadata.len() > limit as u64 {
        return Err(Error::Budget(format!("{label} exceeds its byte limit")));
    }
    let (result, bytes) = action(&mut (&mut file).take((limit as u64).saturating_add(1)))
        .map_err(|_| invalid(label))?;
    if bytes > limit as u64 {
        return Err(Error::Budget(format!("{label} exceeds its byte limit")));
    }
    let after = std::fs::symlink_metadata(resolved).map_err(|_| invalid(label))?;
    if !after.is_file() || !same_file(&metadata, &after) {
        return Err(invalid(label));
    }
    Ok(result)
}

/// Read a bounded regular file while preserving its path identity.
pub fn read(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, Error> {
    consume(path, limit, label, |file| {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let count = bytes.len() as u64;
        Ok((bytes, count))
    })
}

/// Exact identity of a regular file without retaining its contents in memory.
#[derive(Debug, PartialEq, Eq)]
pub struct FileDigest {
    pub sha256: String,
    pub bytes: u64,
}

/// Hash through the same bounded, nonblocking and link-rejecting file admission.
pub fn sha256(path: &Path, limit: usize, label: &str) -> Result<FileDigest, Error> {
    consume(path, limit, label, digest_reader)
}

fn digest_reader(file: &mut dyn Read) -> std::io::Result<(FileDigest, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];
    let mut bytes = 0u64;
    loop {
        let count = match file.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes += count as u64;
    }
    Ok((
        FileDigest {
            sha256: hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            bytes,
        },
        bytes,
    ))
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

    #[test]
    fn hashing_keeps_exact_limits_and_known_content_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256(&path, 3, "input").unwrap(),
            FileDigest {
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
                bytes: 3,
            }
        );
        assert!(matches!(sha256(&path, 2, "input"), Err(Error::Budget(_))));
        assert!(sha256(directory.path(), 10, "input").is_err());
        std::fs::write(&path, b"").unwrap();
        assert_eq!(
            sha256(&path, 0, "input").unwrap().sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashing_retries_interrupted_reads_without_repeating_bytes() {
        struct Interrupted {
            call: usize,
        }
        impl Read for Interrupted {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.call += 1;
                match self.call {
                    1 | 3 => Err(std::io::ErrorKind::Interrupted.into()),
                    2 => {
                        buffer[..2].copy_from_slice(b"ab");
                        Ok(2)
                    }
                    4 => {
                        buffer[0] = b'c';
                        Ok(1)
                    }
                    _ => Ok(0),
                }
            }
        }
        let (identity, bytes) = digest_reader(&mut Interrupted { call: 0 }).unwrap();
        assert_eq!(bytes, 3);
        assert_eq!(identity.bytes, 3);
        assert_eq!(
            identity.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn replacing_an_open_file_is_rejected_after_consumption() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input.bin");
        let replacement = directory.path().join("replacement.bin");
        std::fs::write(&path, b"original").unwrap();
        std::fs::write(&replacement, b"replaced").unwrap();
        assert!(
            consume(&path, 8, "input", |file| {
                let count = std::io::copy(file, &mut std::io::sink())?;
                std::fs::rename(&replacement, &path)?;
                Ok(((), count))
            })
            .is_err()
        );
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
            std::thread::spawn(move || {
                send.send((
                    read(&path, 1024, "inputs").is_err(),
                    sha256(&path, 1024, "inputs").is_err(),
                ))
                .unwrap()
            });
            assert_eq!(
                receive
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .expect("special files must be rejected without blocking"),
                (true, true)
            );
        }
    }
}
