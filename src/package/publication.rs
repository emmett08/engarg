//! Private staging and atomic, non-replacing publication of one complete package.
use crate::Error;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Publication {
    stage: PathBuf,
    output: PathBuf,
    remaining: usize,
    published: bool,
}
impl Publication {
    pub fn new(output: &Path) -> Result<Self, Error> {
        let absolute = std::path::absolute(output)?;
        let parent = absolute.parent().ok_or_else(|| {
            Error::InvalidInput("Package output requires a parent directory".into())
        })?;
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let name = absolute.file_name().ok_or_else(|| {
            Error::InvalidInput("Package output requires a directory name".into())
        })?;
        let output = parent.join(name);
        if output.symlink_metadata().is_ok() {
            return Err(Error::InvalidInput(
                "Package output must be a new directory".into(),
            ));
        }
        for _ in 0..64 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::InvalidInput("Cannot identify package staging time".into()))?
                .as_nanos();
            let stage = parent.join(format!(
                ".engarg-package-{}-{epoch}-{sequence}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&stage) {
                Ok(()) => {
                    return Ok(Self {
                        stage,
                        output,
                        remaining: super::files::MAX_BYTES,
                        published: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::InvalidInput(
            "Could not allocate a private package stage".into(),
        ))
    }
    pub(crate) fn root(&self) -> &Path {
        &self.stage
    }
    pub fn remaining(&self) -> usize {
        self.remaining
    }
    pub fn write(
        &mut self,
        relative: &Path,
        bytes: &[u8],
        permissions: Option<fs::Permissions>,
    ) -> Result<(), Error> {
        if bytes.len() > self.remaining {
            return Err(Error::Budget(
                "Package exceeds 256 MiB total byte budget".into(),
            ));
        }
        if relative.is_absolute()
            || relative
                .components()
                .any(|p| !matches!(p, std::path::Component::Normal(_)))
        {
            return Err(Error::InvalidInput(
                "Package output path must be relative".into(),
            ));
        }
        let destination = self.stage.join(relative);
        fs::create_dir_all(destination.parent().expect("staged parent"))?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(destination)?;
        file.write_all(bytes)?;
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        file.sync_all()?;
        self.remaining -= bytes.len();
        Ok(())
    }
    pub fn publish(mut self) -> Result<(), Error> {
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        {
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                &self.stage,
                rustix::fs::CWD,
                &self.output,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(std::io::Error::from)?;
            self.published = true;
            Ok(())
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
        {
            Err(Error::InvalidInput(
                "Atomic package publication requires Linux or an Apple platform".into(),
            ))
        }
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.stage);
        }
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
mod tests {
    use super::*;

    #[test]
    fn an_output_created_during_staging_is_preserved_without_partial_publication() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("package");
        let mut publication = Publication::new(&output).unwrap();
        let stage = publication.stage.clone();
        publication
            .write(Path::new("case.eal"), b"staged evidence program", None)
            .unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(output.join("existing"), b"other publisher").unwrap();

        assert!(publication.publish().is_err());
        assert_eq!(
            fs::read(output.join("existing")).unwrap(),
            b"other publisher"
        );
        assert!(!output.join("case.eal").exists());
        assert!(!stage.exists());
    }
}
