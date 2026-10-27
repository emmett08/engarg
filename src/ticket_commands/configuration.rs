//! Admission and immutable input identities for operator-submitted tickets.
use super::TicketArgs;
use clap::Parser;
use engarg::{Error, domain::Request, engine::topology::DebateConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn timeout() -> u64 {
    3600
}
fn environment() -> Vec<String> {
    vec!["PATH".into(), "HOME".into()]
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema_version: String,
    pub ticket_id: String,
    pub request: Request,
    pub debate: DebateConfig,
    pub tools: PathBuf,
    pub authoring_args: Vec<String>,
    pub eal_host: PathBuf,
    #[serde(default = "environment")]
    pub environment: Vec<String>,
    #[serde(default)]
    pub collector_environment: Vec<String>,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "timeout")]
    pub stage_timeout_seconds: u64,
    #[serde(default)]
    pub export_policy: Option<PathBuf>,
    /// Additional operator-owned files retained with the submitted ticket.
    #[serde(default)]
    pub input_files: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub retained: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub configuration: Configuration,
    pub pins: Vec<Pin>,
    pub tool_files: Vec<PathBuf>,
    pub coordinator: PathBuf,
}

pub struct Prepared {
    pub configuration: Configuration,
    pub workspace: PathBuf,
    pub coordinator: PathBuf,
    pub snapshot: PathBuf,
    pub snapshot_sha256: String,
    pub snapshot_bytes: u64,
    pub inputs: Vec<eal_coordination_contracts::ticket::Artifact>,
}

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Error> {
    crate::read_json(path, 16 * 1_048_576)
}
fn resolve(base: &Path, path: &Path) -> Result<PathBuf, Error> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    };
    Ok(path.canonicalize()?)
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn validate_authoring(args: &[String]) -> Result<(), Error> {
    let allowed = [
        "--model",
        "--service-tier",
        "--reasoning-effort",
        "--role-model",
        "--api",
        "--endpoint",
        "--api-key-env",
        "--max-calls",
        "--argument-depth",
        "--max-repairs",
        "--max-input-bytes",
        "--max-response-bytes",
        "--max-output-tokens",
        "--http-retries",
        "--retry-max-delay-secs",
        "--max-concurrent-requests",
        "--request-timeout-secs",
        "--timeout-secs",
    ];
    if args.len() > 128
        || !args.len().is_multiple_of(2)
        || args.as_chunks::<2>().0.iter().any(|pair| {
            !allowed.contains(&pair[0].as_str())
                || pair[1].is_empty()
                || pair[1].starts_with("--")
                || pair[1].len() > 8192
        })
    {
        return Err(Error::InvalidInput(
            "authoring_args requires declared provider/budget option-value pairs".into(),
        ));
    }
    Ok(())
}
pub fn prepare(args: TicketArgs) -> Result<Prepared, Error> {
    let source = args.spec.canonicalize()?;
    let base = source
        .parent()
        .ok_or_else(|| Error::InvalidInput("ticket spec has no directory".into()))?;
    let mut config: Configuration = read(&source)?;
    if config.schema_version != "engarg.ticket/2"
        || config.ticket_id.is_empty()
        || config.ticket_id.len() > 80
        || !config
            .ticket_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        || !(1..=86400).contains(&config.timeout_seconds)
        || !(1..=3600).contains(&config.stage_timeout_seconds)
        || config.stage_timeout_seconds > config.timeout_seconds
    {
        return Err(Error::InvalidInput(
            "unsupported ticket schema, identifier or deadline".into(),
        ));
    }
    engarg::domain::validate_request(&config.request)
        .map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
    config.debate.graphs()?;
    validate_authoring(&config.authoring_args)?;
    eal_collector_contracts::validate_environment_names(&config.environment, 64)
        .map_err(Error::InvalidInput)?;
    eal_collector_contracts::validate_environment_names(&config.collector_environment, 64)
        .map_err(Error::InvalidInput)?;
    if config
        .environment
        .iter()
        .any(|name| std::env::var_os(name).is_none())
        || config
            .collector_environment
            .iter()
            .any(|name| !config.environment.contains(name))
    {
        return Err(Error::InvalidInput(
            "ticket environment must exist and include every collector credential reference".into(),
        ));
    }
    let mut admission = vec![
        "engarg".to_owned(),
        "analyse".into(),
        "--claim".into(),
        config.request.claim.clone(),
    ];
    admission.extend(config.authoring_args.clone());
    let parsed = crate::Cli::try_parse_from(admission).map_err(|_| {
        Error::InvalidInput("invalid ticket authoring provider or budget options".into())
    })?;
    if let crate::Command::Analyse(selected) = parsed.command {
        let required = config.debate.specialist_calls()? + 3 + u32::from(selected.max_repairs);
        if config
            .authoring_args
            .iter()
            .any(|value| value == "--max-calls")
            && selected.max_calls < required
        {
            return Err(Error::Budget(format!(
                "selected ticket topology requires at least {required} authoring calls"
            )));
        }
    }
    config.tools = resolve(base, &config.tools)?;
    config.eal_host = resolve(base, &config.eal_host)?;
    if let Some(path) = &mut config.export_policy {
        *path = resolve(base, path)?;
    }
    for path in &mut config.input_files {
        *path = resolve(base, path)?;
    }
    let manifest_bytes = engarg::bounded_file::read(&config.tools, 1_048_576, "ticket tools")?;
    let manifest = eal_collector_contracts::RegistryDocument::from_toml(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| Error::InvalidInput("ticket tools require UTF-8".into()))?,
    )
    .map_err(Error::InvalidInput)?;
    for binding in manifest.tools.values() {
        let mut names = binding.inherit_env.iter().flatten().chain(
            binding
                .direct
                .iter()
                .flat_map(|direct| direct.token_env.iter().chain(direct.headers_env.values())),
        );
        if names.any(|name| !config.collector_environment.contains(name)) {
            return Err(Error::InvalidInput(
                "tool environment references must be explicitly granted through collector_environment".into(),
            ));
        }
    }
    let tool_files = engarg::package::input_files(&config.tools)?;
    let coordinator = resolve(
        base,
        args.eal_coordinate
            .as_deref()
            .unwrap_or(&config.eal_host.with_file_name("eal-coordinate")),
    )?;
    if !executable_file(&config.eal_host) || !executable_file(&coordinator) {
        return Err(Error::InvalidInput(
            "ticket host and coordinator must be executable files".into(),
        ));
    }
    let mut paths = BTreeSet::from([source, config.eal_host.clone(), coordinator.clone()]);
    paths.extend(tool_files.iter().cloned());
    paths.extend(config.export_policy.iter().cloned());
    paths.extend(config.input_files.iter().cloned());
    let executables = paths
        .iter()
        .filter(|path| executable_file(path))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut retained_bytes = 0u64;
    let mut retained_files = 0usize;
    for path in paths.iter().filter(|path| !executables.contains(*path)) {
        let size = path.metadata()?.len();
        retained_bytes = retained_bytes.saturating_add(size);
        retained_files += 1;
        if size > 16 * 1_048_576 || retained_bytes > 48 * 1_048_576 || retained_files > 127 {
            return Err(Error::Budget(
                "ticket retained inputs exceed their file, byte or count bound".into(),
            ));
        }
    }
    let workspace = crate::path_admission::identity(&args.run)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&workspace)?;
    let input_directory = workspace.join("inputs");
    fs::create_dir(&input_directory)?;
    let mut pins = Vec::new();
    let mut inputs = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        let (sha256, byte_count, retained) = if executables.contains(&path) {
            let identity = engarg::bounded_file::sha256(&path, 256 * 1_048_576, "ticket input")?;
            (identity.sha256, identity.bytes, None)
        } else {
            let bytes = engarg::bounded_file::read(&path, 16 * 1_048_576, "ticket input")?;
            let sha256 = digest(&bytes);
            let target = input_directory.join(format!("input-{index:03}.bin"));
            let mut file = crate::private_file(&target)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            inputs.push(eal_coordination_contracts::ticket::Artifact {
                kind: format!("input_{index}"),
                path: target.clone(),
                sha256: sha256.clone(),
                bytes: bytes.len() as u64,
            });
            (sha256, bytes.len() as u64, Some(target))
        };
        pins.push(Pin {
            path,
            sha256,
            bytes: byte_count,
            retained,
        });
    }
    let snapshot = workspace.join("engarg-ticket.json");
    write_json(
        &snapshot,
        &Snapshot {
            configuration: config,
            pins,
            tool_files,
            coordinator: coordinator.clone(),
        },
    )?;
    let bytes = fs::read(&snapshot)?;
    let snapshot_value: Snapshot = read(&snapshot)?;
    Ok(Prepared {
        configuration: snapshot_value.configuration,
        workspace,
        coordinator,
        snapshot,
        snapshot_sha256: digest(&bytes),
        snapshot_bytes: bytes.len() as u64,
        inputs,
    })
}

pub fn load(path: &Path, sha256: &str) -> Result<Snapshot, Error> {
    let bytes = engarg::bounded_file::read(path, 16 * 1_048_576, "ticket snapshot")?;
    if digest(&bytes) != sha256 {
        return Err(Error::InvalidInput(
            "ticket snapshot identity changed".into(),
        ));
    }
    let value = eal_collector_contracts::strict_json_exact(
        &bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: 16 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    let snapshot: Snapshot = serde_json::from_value(value)?;
    if engarg::package::input_files(&snapshot.configuration.tools)? != snapshot.tool_files {
        return Err(Error::InvalidInput(
            "ticket tool resource inventory changed".into(),
        ));
    }
    for pin in &snapshot.pins {
        let identity = engarg::bounded_file::sha256(
            &pin.path,
            if pin.retained.is_none() { 256 } else { 16 } * 1_048_576,
            "ticket input",
        )?;
        if let Some(retained) = &pin.retained {
            let retained_identity =
                engarg::bounded_file::sha256(retained, 16 * 1_048_576, "retained ticket input")?;
            if retained_identity.bytes != pin.bytes || retained_identity.sha256 != pin.sha256 {
                return Err(Error::InvalidInput(
                    "retained ticket input identity changed".into(),
                ));
            }
        }
        if identity.bytes != pin.bytes || identity.sha256 != pin.sha256 {
            return Err(Error::InvalidInput("a pinned ticket input changed".into()));
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn script(path: &Path, output: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, format!("#!/bin/sh\nprintf '%s' '{output}'\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    fn submission(root: &Path) -> TicketArgs {
        let executable = root.join("eal-host");
        script(&executable, "fixture");
        fs::create_dir(root.join("tools")).unwrap();
        fs::write(root.join("tools/source.json"), "{}").unwrap();
        fs::write(
            root.join("tools.toml"),
            "[tools.source]\nkind='json_file'\nversion='1'\npath='tools/source.json'\n",
        )
        .unwrap();
        let source = root.join("ticket.json");
        write_json(
            &source,
            &serde_json::json!({
                "schema_version":"engarg.ticket/2", "ticket_id":"direct_admission",
                "request":{"claim":"A bounded fixture claim.","context":"","evidence":[]},
                "debate":{"participants":3,"rounds":1,"topology":{"kind":"ring"}},
                "tools":"tools.toml", "authoring_args":["--model","fixture"],
                "eal_host":executable, "environment":["PATH"],
            }),
        )
        .unwrap();
        TicketArgs {
            spec: source,
            run: root.join("run"),
            eal_coordinate: Some(executable),
        }
    }

    #[cfg(unix)]
    #[test]
    fn direct_tool_resources_are_pinned_and_changed_inputs_reject_the_next_stage() {
        let root = tempfile::tempdir().unwrap();
        let prepared = prepare(submission(root.path())).unwrap();
        let snapshot = load(&prepared.snapshot, &prepared.snapshot_sha256).unwrap();
        let source = root
            .path()
            .join("tools/source.json")
            .canonicalize()
            .unwrap();
        assert!(
            snapshot
                .pins
                .iter()
                .any(|pin| pin.path == source && pin.retained.is_some())
        );
        fs::write(source, "{\"changed\":true}").unwrap();
        assert!(load(&prepared.snapshot, &prepared.snapshot_sha256).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn new_tool_resource_cannot_enter_an_admitted_ticket() {
        let root = tempfile::tempdir().unwrap();
        let prepared = prepare(submission(root.path())).unwrap();
        fs::write(root.path().join("tools/new-helper.py"), "print('changed')").unwrap();
        let error = load(&prepared.snapshot, &prepared.snapshot_sha256)
            .err()
            .unwrap();
        assert!(error.to_string().contains("resource inventory changed"));
    }

    #[cfg(unix)]
    #[test]
    fn ungranted_tool_environment_and_old_ticket_fields_reject_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let args = submission(root.path());
        fs::write(
            root.path().join("tools.toml"),
            "[tools.source]\nkind='command'\nversion='1'\nargv=['python3']\ninherit_env=['PATH']\n",
        )
        .unwrap();
        assert!(prepare(args).is_err());
        assert!(!root.path().join("run").exists());
        let mut value: serde_json::Value = read(&root.path().join("ticket.json")).unwrap();
        value["evidence_config"] = serde_json::json!("planning.json");
        assert!(serde_json::from_value::<Configuration>(value).is_err());
    }

    fn invalid_submission(args: Vec<&str>, collector_environment: Vec<&str>) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ticket.json");
        let value = serde_json::json!({"schema_version":"engarg.ticket/2","ticket_id":"admission","request":{"claim":"A bounded fixture claim.","context":"","evidence":[]},"debate":{"participants":3,"rounds":1,"topology":{"kind":"ring"}},"tools":"/missing/tools.toml","authoring_args":args,"eal_host":"/missing/eal-host","environment":["PATH"],"collector_environment":collector_environment});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let workspace = root.path().join("run");
        assert!(
            prepare(TicketArgs {
                spec: path,
                run: workspace.clone(),
                eal_coordinate: None
            })
            .is_err()
        );
        assert!(!workspace.exists());
    }
    #[test]
    fn invalid_authoring_budgets_and_ungranted_credentials_precede_side_effects() {
        invalid_submission(
            vec!["--model", "fixture", "--max-repairs", "4294967295"],
            vec![],
        );
        invalid_submission(vec!["--model", "fixture", "--max-calls", "1"], vec![]);
        invalid_submission(vec!["--model", "fixture"], vec!["UNGRANTED_SECRET"]);
    }
}
