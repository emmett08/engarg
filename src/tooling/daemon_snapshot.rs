//! Transfer bounded immutable inputs into daemon-owned volumes without host binds.
use super::contracts::SourceFile;
use crate::Error;
use serde::Deserialize;
use std::{collections::BTreeMap, io::Cursor};

const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: usize = 1_048_576;
const MAX_SNAPSHOT_BYTES: usize = 2_097_152;

pub(super) struct DaemonSnapshot {
    staging: String,
    owner: String,
    source_archive: Vec<u8>,
    fixture_archive: Vec<u8>,
    volumes: Option<[String; 2]>,
}

impl DaemonSnapshot {
    pub(super) fn new(
        name: &str,
        source: &[SourceFile],
        fixtures: &[SourceFile],
    ) -> Result<Self, Error> {
        if !safe_name(name) {
            return Err(invalid("invalid daemon snapshot resource name"));
        }
        Ok(Self {
            staging: format!("{name}-inputs"),
            owner: name.into(),
            source_archive: archive(source)?,
            fixture_archive: archive(fixtures)?,
            volumes: None,
        })
    }

    /// This container is never started. The daemon alone populates its writable
    /// anonymous volumes, which remain attached until the worker has stopped.
    pub(super) fn staging_argv(&self, image: &str) -> Vec<String> {
        [
            "create",
            "--pull",
            "never",
            "--name",
            &self.staging,
            "--label",
            &format!("engarg.snapshot-owner={}", self.owner),
            "--network",
            "none",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--user",
            "65534:65534",
            "--mount",
            "type=volume,target=/src,volume-nocopy",
            "--mount",
            "type=volume,target=/fixtures,volume-nocopy",
            "--entrypoint",
            "/bin/sh",
            image,
            "-c",
            "exit 0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    pub(super) fn inspect_argv(&self) -> Vec<String> {
        [
            "container",
            "inspect",
            "--format",
            "{{json .Mounts}}",
            &self.staging,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    pub(super) fn ownership_argv(&self) -> Vec<String> {
        [
            "container",
            "inspect",
            "--format",
            "{{index .Config.Labels \"engarg.snapshot-owner\"}}",
            &self.staging,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    pub(super) fn expected_owner(&self) -> &str {
        &self.owner
    }

    /// Accept only the two volumes allocated with this stopped container. Host
    /// paths in Docker's inspection data never become worker mount sources.
    pub(super) fn admit_mounts(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.volumes = None;
        #[derive(Deserialize)]
        struct Mount {
            #[serde(rename = "Type")]
            kind: String,
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "Destination")]
            destination: String,
            #[serde(rename = "RW")]
            writable: bool,
        }
        let value = eal_collector_contracts::strict_json_exact(
            bytes,
            eal_collector_contracts::JsonLimits {
                max_bytes: 32_768,
                ..Default::default()
            },
        )
        .map_err(|_| invalid("Docker returned invalid snapshot volume metadata"))?;
        let mounts: Vec<Mount> = serde_json::from_value(value)
            .map_err(|_| invalid("Docker returned invalid snapshot volume metadata"))?;
        if mounts.len() != 2 {
            return Err(invalid(
                "snapshot staging requires exactly two anonymous volumes",
            ));
        }
        let mut selected: [Option<String>; 2] = [None, None];
        for mount in mounts {
            let index = match mount.destination.as_str() {
                "/src" => 0,
                "/fixtures" => 1,
                _ => return Err(invalid("snapshot staging contains an unexpected mount")),
            };
            if mount.kind != "volume"
                || !mount.writable
                || !safe_name(&mount.name)
                || selected[index].replace(mount.name).is_some()
            {
                return Err(invalid(
                    "Docker returned unsuitable snapshot volume metadata",
                ));
            }
        }
        let [Some(source), Some(fixtures)] = selected else {
            return Err(invalid("snapshot staging omits a required volume"));
        };
        if source == fixtures {
            return Err(invalid(
                "source and fixture snapshots require separate volumes",
            ));
        }
        self.volumes = Some([source, fixtures]);
        Ok(())
    }

    pub(super) fn source_copy_argv(&self) -> Vec<String> {
        vec![
            "cp".into(),
            "--archive".into(),
            "-".into(),
            format!("{}:/src", self.staging),
        ]
    }

    pub(super) fn fixture_copy_argv(&self) -> Vec<String> {
        vec![
            "cp".into(),
            "--archive".into(),
            "-".into(),
            format!("{}:/fixtures", self.staging),
        ]
    }

    pub(super) fn source_archive(&self) -> &[u8] {
        &self.source_archive
    }
    pub(super) fn fixture_archive(&self) -> &[u8] {
        &self.fixture_archive
    }

    pub(super) fn worker_mount_args(&self) -> Result<Vec<String>, Error> {
        let [source, fixtures] = self
            .volumes
            .as_ref()
            .ok_or_else(|| invalid("snapshot volumes have not been admitted"))?;
        Ok(vec![
            "--mount".into(),
            format!("type=volume,source={source},target=/src,readonly,volume-nocopy"),
            "--mount".into(),
            format!("type=volume,source={fixtures},target=/fixtures,readonly,volume-nocopy"),
        ])
    }

    pub(super) fn cleanup_argv(&self) -> Vec<String> {
        ["container", "rm", "--force", "--volumes", &self.staging]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn safe_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':'))
        && value
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".."))
}

/// Sorted directory and regular-file entries preserve exact text and portable
/// permissions. SourceFile cannot encode links, device nodes or archive headers.
fn archive(files: &[SourceFile]) -> Result<Vec<u8>, Error> {
    if files.len() > MAX_FILES {
        return Err(Error::Budget(
            "collector snapshot contains too many files".into(),
        ));
    }
    let mut total = 0usize;
    // None is a directory. The lower-case key rejects aliases and file/directory
    // conflicts before any bytes reach Docker's archive extraction endpoint.
    let mut entries: BTreeMap<String, (String, Option<&str>)> = BTreeMap::new();
    for file in files {
        total = total.saturating_add(file.content.len());
        if !safe_path(&file.path)
            || file.content.len() > MAX_FILE_BYTES
            || total > MAX_SNAPSHOT_BYTES
        {
            return Err(invalid(
                "collector snapshot files exceed path or byte limits",
            ));
        }
        let components: Vec<_> = file.path.split('/').collect();
        for end in 1..components.len() {
            insert_entry(&mut entries, components[..end].join("/"), None)?;
        }
        insert_entry(&mut entries, file.path.clone(), Some(&file.content))?;
    }
    let mut builder = tar::Builder::new(Vec::new());
    builder.mode(tar::HeaderMode::Deterministic);
    for (_, (path, content)) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_uid(65534);
        header.set_gid(65534);
        header.set_mtime(0);
        if let Some(content) = content {
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o444);
            header.set_size(content.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, path, Cursor::new(content.as_bytes()))?;
        } else {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_mode(0o755);
            header.set_size(0);
            header.set_cksum();
            builder.append_data(&mut header, path, std::io::empty())?;
        }
    }
    Ok(builder.into_inner()?)
}

fn insert_entry<'a>(
    entries: &mut BTreeMap<String, (String, Option<&'a str>)>,
    path: String,
    content: Option<&'a str>,
) -> Result<(), Error> {
    let key = path.to_ascii_lowercase();
    if let Some((existing, previous)) = entries.get(&key) {
        if existing != &path || previous.is_some() || content.is_some() {
            return Err(invalid(
                "collector snapshot contains duplicate, aliased or conflicting paths",
            ));
        }
    } else {
        entries.insert(key, (path, content));
    }
    Ok(())
}

#[cfg(test)]
#[path = "daemon_snapshot_tests.rs"]
mod tests;
