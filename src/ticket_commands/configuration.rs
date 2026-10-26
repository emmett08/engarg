//! Admission and immutable input identities for operator-submitted tickets.
use super::{TicketArgs, process};
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
    pub evidence_config: PathBuf,
    pub authoring_args: Vec<String>,
    #[serde(default)]
    pub implementation_args: Vec<String>,
    #[serde(default)]
    pub mcp_registrations: Option<PathBuf>,
    #[serde(default)]
    pub qualification_argv: Option<Vec<String>>,
    pub eal_host: PathBuf,
    #[serde(default)]
    pub eal_application: Option<PathBuf>,
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
    #[serde(default)]
    pub review_binding: Option<PathBuf>,
    /// Additional operator files, including qualification policy dependencies.
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

fn resolve_review_binding(
    mut binding: eal_collector_contracts::CommandBinding,
    base: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Result<eal_collector_contracts::CommandBinding, Error> {
    let program = Path::new(&binding.argv[0]);
    let program = if program.is_absolute() || program.components().count() > 1 {
        let path = base.join(program);
        executable_file(&path).then_some(path)
    } else {
        search_path.and_then(|paths| {
            std::env::split_paths(paths)
                .map(|directory| directory.join(program))
                .find(|path| executable_file(path))
        })
    }
    .ok_or_else(|| Error::InvalidInput("review executable unavailable".into()))?;
    binding.argv[0] = process::path(&program.canonicalize()?)?;
    // A pinned script must be invoked through the same resolved path, too.
    for argument in binding.argv.iter_mut().skip(1) {
        let path = Path::new(argument);
        if path.is_absolute() && path.is_file() {
            *argument = process::path(&path.canonicalize()?)?;
        }
    }
    Ok(binding)
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
    if config.schema_version != "engarg.ticket/1"
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
    config.evidence_config = resolve(base, &config.evidence_config)?;
    config.eal_host = resolve(base, &config.eal_host)?;
    config.eal_application = Some(resolve(
        base,
        config
            .eal_application
            .as_deref()
            .unwrap_or(&config.eal_host.with_file_name("eal")),
    )?);
    for value in [
        &mut config.mcp_registrations,
        &mut config.export_policy,
        &mut config.review_binding,
    ]
    .into_iter()
    .flatten()
    {
        *value = resolve(base, value)?;
    }
    let review_binding_source = config.review_binding.clone();
    let review_binding =
        crate::eal_commands::selected_review_binding(config.review_binding.as_deref())?
            .map(|binding| {
                resolve_review_binding(binding, base, std::env::var_os("PATH").as_deref())
            })
            .transpose()?;
    if review_binding.as_ref().is_some_and(|binding| {
        binding
            .inherit_env
            .iter()
            .any(|name| !config.collector_environment.contains(name))
    }) {
        return Err(Error::InvalidInput(
            "review credential references must be explicitly granted through collector_environment"
                .into(),
        ));
    }
    for path in &mut config.input_files {
        *path = resolve(base, path)?;
    }
    if let Some(argv) = &mut config.qualification_argv {
        if argv.is_empty() || argv.len() > 64 {
            return Err(Error::InvalidInput(
                "qualification_argv must be a bounded executable argument vector".into(),
            ));
        }
        for argument in argv.iter_mut() {
            let candidate = base.join(&*argument);
            if candidate.is_file() {
                *argument = process::path(&candidate.canonicalize()?)?;
            }
        }
        let executable = Path::new(&argv[0]);
        if !executable.is_absolute() || !executable.is_file() {
            return Err(Error::InvalidInput(
                "qualification executable must be an existing absolute path".into(),
            ));
        }
    }
    for pair in config.implementation_args.as_chunks_mut::<2>().0 {
        if pair[0] == "--toolchain" {
            pair[1] = process::path(&resolve(base, Path::new(&pair[1]))?)?;
        }
    }
    let coordinator = resolve(
        base,
        args.eal_coordinate
            .as_deref()
            .unwrap_or(&config.eal_host.with_file_name("eal-coordinate")),
    )?;
    let workspace = crate::path_admission::identity(&args.run)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&workspace)?;
    if let Some(binding) = &review_binding {
        let path = workspace.join("admitted-review-binding.json");
        write_json(&path, binding)?;
        config.review_binding = Some(path);
    }
    let mut paths = BTreeSet::from([
        source,
        config.evidence_config.clone(),
        config.eal_host.clone(),
        coordinator.clone(),
        config.eal_application.clone().expect("resolved"),
    ]);
    paths.extend(config.mcp_registrations.iter().cloned());
    paths.extend(config.export_policy.iter().cloned());
    paths.extend(review_binding_source);
    paths.extend(config.review_binding.iter().cloned());
    paths.extend(config.input_files.iter().cloned());
    if let Some(argv) = &config.qualification_argv {
        for argument in argv.iter() {
            let path = PathBuf::from(argument);
            if path.is_file() {
                paths.insert(path.canonicalize()?);
            }
        }
    }
    if let Some(binding) = &review_binding {
        for argument in &binding.argv {
            let path = PathBuf::from(argument);
            if path.is_absolute() && path.is_file() {
                paths.insert(path.canonicalize()?);
            }
        }
    }
    for pair in config.implementation_args.as_chunks::<2>().0.iter() {
        if pair[0] == "--toolchain" {
            paths.insert(PathBuf::from(&pair[1]));
        }
    }
    let planning: serde_json::Value = read(&config.evidence_config)?;
    let planning_base = config.evidence_config.parent().expect("resolved");
    if planning["environment"].as_array().is_some_and(|names| {
        names.iter().any(|name| {
            name.as_str()
                .is_none_or(|name| !config.environment.iter().any(|allowed| allowed == name))
        })
    }) {
        return Err(Error::InvalidInput(
            "planning environment references must be granted by the ticket environment".into(),
        ));
    }
    if let Some(connection) = planning["connections"].as_str() {
        paths.insert(resolve(planning_base, Path::new(connection))?);
    }
    let input_directory = workspace.join("inputs");
    fs::create_dir(&input_directory)?;
    let mut executables = BTreeSet::from([
        config.eal_host.clone(),
        coordinator.clone(),
        config.eal_application.clone().expect("resolved"),
    ]);
    if let Some(argv) = &config.qualification_argv {
        executables.insert(PathBuf::from(&argv[0]).canonicalize()?);
    }
    if let Some(binding) = &review_binding {
        executables.insert(PathBuf::from(&binding.argv[0]));
    }
    let mut pins = Vec::new();
    let mut inputs = Vec::new();
    for (index, path) in paths.into_iter().enumerate() {
        let bytes = engarg::bounded_file::read(
            &path,
            if executables.contains(&path) { 256 } else { 16 } * 1_048_576,
            "ticket input",
        )?;
        let sha256 = digest(&bytes);
        let retained = if !executables.contains(&path) {
            if bytes.len() > 16 * 1_048_576 {
                return Err(Error::Budget("ticket input document exceeds 16 MiB".into()));
            }
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
            Some(target)
        } else {
            None
        };
        pins.push(Pin {
            path,
            sha256,
            bytes: bytes.len() as u64,
            retained,
        });
    }
    let snapshot = workspace.join("engarg-ticket.json");
    write_json(
        &snapshot,
        &Snapshot {
            configuration: config,
            pins,
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
    let snapshot: Snapshot = read(path)?;
    for pin in &snapshot.pins {
        let bytes = engarg::bounded_file::read(
            &pin.path,
            if pin.retained.is_none() { 256 } else { 16 } * 1_048_576,
            "ticket input",
        )?;
        if let Some(retained) = &pin.retained {
            let retained_bytes =
                engarg::bounded_file::read(retained, 16 * 1_048_576, "retained ticket input")?;
            if retained_bytes.len() as u64 != pin.bytes || digest(&retained_bytes) != pin.sha256 {
                return Err(Error::InvalidInput(
                    "retained ticket input identity changed".into(),
                ));
            }
        }
        if bytes.len() as u64 != pin.bytes || digest(&bytes) != pin.sha256 {
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
    fn admitted_review(root: &Path, argv: Vec<String>) -> Prepared {
        let executable = root.join("eal");
        script(&executable, "fixture");
        let planning = root.join("planning.json");
        write_json(&planning, &serde_json::json!({})).unwrap();
        let binding = root.join("review-binding.json");
        write_json(
            &binding,
            &serde_json::json!({
                "kind":"command", "version":"1", "argv":argv,
                "timeout_seconds":30, "max_output_bytes":1048576,
                "inherit_env":[], "parallel_safe":false,
            }),
        )
        .unwrap();
        let source = root.join("ticket.json");
        write_json(
            &source,
            &serde_json::json!({
                "schema_version":"engarg.ticket/1", "ticket_id":"review_admission",
                "request":{"claim":"A bounded fixture claim.","context":"","evidence":[]},
                "debate":{"participants":3,"rounds":1,"topology":{"kind":"ring"}},
                "evidence_config":planning, "authoring_args":["--model","fixture"],
                "eal_host":executable, "eal_application":executable,
                "review_binding":binding, "environment":["PATH"],
            }),
        )
        .unwrap();
        prepare(TicketArgs {
            spec: source,
            run: root.join("run"),
            eal_coordinate: Some(executable),
        })
        .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn basename_reviewers_are_retained_with_their_exact_pinned_executable() {
        let root = tempfile::tempdir().unwrap();
        let prepared = admitted_review(root.path(), vec!["sh".into()]);
        let snapshot = load(&prepared.snapshot, &prepared.snapshot_sha256).unwrap();
        let admitted = snapshot.configuration.review_binding.unwrap();
        assert_ne!(admitted, root.path().join("review-binding.json"));
        let binding: eal_collector_contracts::CommandBinding = read(&admitted).unwrap();
        let program = Path::new(&binding.argv[0]);
        assert!(program.is_absolute());
        assert_eq!(program.canonicalize().unwrap(), program);
        assert!(
            snapshot
                .pins
                .iter()
                .any(|pin| { pin.path == program && pin.retained.is_none() && pin.bytes > 0 })
        );
        assert!(snapshot.pins.iter().any(|pin| {
            pin.path == root.path().join("review-binding.json") && pin.retained.is_some()
        }));
    }

    #[cfg(unix)]
    #[test]
    fn retargeted_review_program_symlinks_cannot_change_the_admitted_command() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let replacement = root.path().join("replacement");
        script(&original, "original");
        script(&replacement, "replacement");
        let alias = root.path().join("reviewer");
        symlink(&original, &alias).unwrap();
        let prepared = admitted_review(root.path(), vec![process::path(&alias).unwrap()]);
        fs::remove_file(&alias).unwrap();
        symlink(&replacement, &alias).unwrap();
        let snapshot = load(&prepared.snapshot, &prepared.snapshot_sha256).unwrap();
        let binding: eal_collector_contracts::CommandBinding =
            read(snapshot.configuration.review_binding.as_deref().unwrap()).unwrap();
        assert_eq!(Path::new(&binding.argv[0]), original);
        let output = std::process::Command::new(&binding.argv[0])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"original");
        script(&original, "changed");
        assert!(load(&prepared.snapshot, &prepared.snapshot_sha256).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn review_script_arguments_use_the_pinned_target_after_symlink_retargeting() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original.sh");
        let replacement = root.path().join("replacement.sh");
        script(&original, "original");
        script(&replacement, "replacement");
        let alias = root.path().join("review-policy.sh");
        symlink(&original, &alias).unwrap();
        let prepared = admitted_review(
            root.path(),
            vec!["sh".into(), process::path(&alias).unwrap()],
        );
        fs::remove_file(&alias).unwrap();
        symlink(&replacement, &alias).unwrap();
        let snapshot = load(&prepared.snapshot, &prepared.snapshot_sha256).unwrap();
        let binding: eal_collector_contracts::CommandBinding =
            read(snapshot.configuration.review_binding.as_deref().unwrap()).unwrap();
        assert_eq!(Path::new(&binding.argv[1]), original);
        let output = std::process::Command::new(&binding.argv[0])
            .args(&binding.argv[1..])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"original");
    }

    fn invalid_submission(args: Vec<&str>, collector_environment: Vec<&str>) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ticket.json");
        let value = serde_json::json!({"schema_version":"engarg.ticket/1","ticket_id":"admission","request":{"claim":"A bounded fixture claim.","context":"","evidence":[]},"debate":{"participants":3,"rounds":1,"topology":{"kind":"ring"}},"evidence_config":"/missing/planning.json","authoring_args":args,"eal_host":"/missing/eal-host","environment":["PATH"],"collector_environment":collector_environment});
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
