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
    sync_parent: fn(&Path) -> std::io::Result<()>,
}

/// Publication and confirmed durability are different outcomes. A directory
/// sync failure must not undo memory after rename has changed the file.
struct PersistError {
    source: Error,
    published: bool,
}

fn sync_parent(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
impl Store {
    pub fn create(path: &Path, checkpoint: Checkpoint) -> Result<Self, Error> {
        let path = output_identity(path)?;
        let file = private_new(&path)?;
        drop(file);
        let store = Self {
            path,
            state: Mutex::new(checkpoint),
            sync_parent,
        };
        store
            .persist(
                &*store
                    .state
                    .lock()
                    .map_err(|_| Error::InvalidInput("checkpoint lock poisoned".into()))?,
            )
            .map_err(|error| error.source)?;
        Ok(store)
    }
    fn persist(&self, state: &Checkpoint) -> Result<(), PersistError> {
        let before_publication = |source| PersistError {
            source,
            published: false,
        };
        let bytes = serde_json::to_vec(state).map_err(|error| before_publication(error.into()))?;
        if bytes.len() > MAX_BYTES {
            return Err(before_publication(Error::Budget(
                "authoring checkpoint exceeds 128 MiB".into(),
            )));
        }
        let parent = self.path.parent().ok_or_else(|| {
            before_publication(Error::InvalidInput(
                "checkpoint needs a parent directory".into(),
            ))
        })?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| before_publication(std::io::Error::other(error).into()))?
            .as_nanos();
        let temporary = parent.join(format!(".authoring-{}-{stamp}.tmp", std::process::id()));
        let mut published = false;
        let result: Result<(), Error> = (|| {
            let mut file = private_new(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            published = true;
            (self.sync_parent)(parent)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result.map_err(|source| PersistError { source, published })
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
        // The lock owns the mutable history. Only the new exchange needs a
        // copy; rolling back its append preserves the previous in-memory cut
        // when persistence fails before publication.
        state.transcript.push(call.clone());
        if let Err(error) = self.persist(&state) {
            if !error.published {
                state.transcript.pop();
            }
            return Err(error.source);
        }
        Ok(())
    }
    pub fn failure(&self, error: &Error) -> Result<(), Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::InvalidInput("checkpoint lock poisoned".into()))?;
        let previous = state.failure.replace(error.to_string());
        if let Err(error) = self.persist(&state) {
            if !error.published {
                state.failure = previous;
            }
            return Err(error.source);
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::Mode,
        provider::{Completion, CompletionRequest, Role},
    };

    #[test]
    fn a_post_publication_sync_failure_retains_the_published_cut() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("checkpoint.json");
        let request = crate::domain::Request {
            claim: "Fixture".into(),
            context: String::new(),
            evidence: vec![],
        };
        let config = crate::engine::RunConfig {
            mode: Mode::Single,
            ..Default::default()
        };
        let mut store = Store::create(
            &path,
            Checkpoint::new(request.clone(), config, json!({}), "fixture".into()),
        )
        .unwrap();
        let call = CallRecord {
            role: Role::Single,
            prompt: CompletionRequest {
                role: Role::Single,
                system: "fixture".into(),
                user: json!({"request":request}).to_string(),
                schema: json!({}),
            },
            response: Completion {
                content: "{}".into(),
                model: "fixture".into(),
                input_tokens: None,
                output_tokens: None,
                request_id: None,
                service_tier: None,
                reasoning_effort: None,
            },
            response_content_bytes: 2,
            response_content_sha256: digest("{}"),
            response_content_omitted: false,
            elapsed_ms: 0,
        };
        store.sync_parent = |_| Err(std::io::Error::other("injected directory sync failure"));
        assert!(store.completion(&call).is_err());
        assert_eq!(Checkpoint::read(&path).unwrap().transcript.len(), 1);
        assert_eq!(store.state.lock().unwrap().transcript.len(), 1);
        assert!(
            store
                .failure(&Error::Budget("fixture error".into()))
                .is_err()
        );
        assert_eq!(
            store.state.lock().unwrap().failure,
            Checkpoint::read(&path).unwrap().failure
        );
        store.sync_parent = sync_parent;
        store.completion(&call).unwrap();
        assert_eq!(Checkpoint::read(&path).unwrap().transcript.len(), 2);
    }
}
