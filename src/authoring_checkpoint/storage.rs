//! Private, durable atomic persistence for authoring checkpoints.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

/// Atomic replacement is allowed only for a checkpoint created by this store.
/// New paths are create-new, private, and checked before any provider request.
pub struct Store {
    path: PathBuf,
    state: Mutex<Checkpoint>,
}
impl Store {
    pub fn create(path: &Path, checkpoint: Checkpoint) -> Result<Self, Error> {
        let path = output_identity(path)?;
        let file = private_new(&path)?;
        drop(file);
        let store = Self {
            path,
            state: Mutex::new(checkpoint),
        };
        store.persist(
            &*store
                .state
                .lock()
                .map_err(|_| Error::InvalidInput("checkpoint lock poisoned".into()))?,
        )?;
        Ok(store)
    }
    fn persist(&self, state: &Checkpoint) -> Result<(), Error> {
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Budget("authoring checkpoint exceeds 128 MiB".into()));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::InvalidInput("checkpoint needs a parent directory".into()))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let temporary = parent.join(format!(".authoring-{}-{stamp}.tmp", std::process::id()));
        let result = (|| {
            let mut file = private_new(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn completion(&self, call: &CallRecord) -> Result<(), Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::InvalidInput("checkpoint lock poisoned".into()))?;
        if state.transcript.len() >= MAX_CALLS {
            return Err(Error::Budget(
                "authoring checkpoint call history exhausted".into(),
            ));
        }
        let mut next = state.clone();
        next.transcript.push(call.clone());
        self.persist(&next)?;
        *state = next;
        Ok(())
    }
    pub fn failure(&self, error: &Error) -> Result<(), Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::InvalidInput("checkpoint lock poisoned".into()))?;
        state.failure = Some(error.to_string());
        self.persist(&state)
    }
}

pub fn private_new(path: &Path) -> Result<fs::File, Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

/// Resolve existing parent aliases before comparing create-new output paths.
pub fn output_identity(path: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(path)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::InvalidInput("output needs an existing parent directory".into()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::InvalidInput("output needs a filename".into()))?;
    Ok(parent.canonicalize()?.join(name))
}
