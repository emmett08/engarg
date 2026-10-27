//! Public file admission rejects special files and bounds allocation before parsing.

use engarg::{Error, bounded_file};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const MIB: usize = 1_048_576;
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Clone, Copy, Debug)]
enum Entry {
    AnalyseRequest,
    AnalyseTools,
    RenderReport,
    ValidateRequest,
    ValidateCase,
    PrepareReport,
    PrepareCatalogue,
    PreparePolicy,
    ExportReport,
    ExportPlan,
    ExportCatalogue,
    ExportPolicy,
    SourceValidate,
    SourcePlan,
    SourceCollect,
    SourceAssess,
    CollectContext,
    AssessContext,
    Registry,
    ScopedInputs,
    ArtifactInputs,
    ReviewInputs,
    RetainedArtifact,
}

const ENTRIES: &[Entry] = &[
    Entry::AnalyseRequest,
    Entry::AnalyseTools,
    Entry::RenderReport,
    Entry::ValidateRequest,
    Entry::ValidateCase,
    Entry::PrepareReport,
    Entry::PrepareCatalogue,
    Entry::PreparePolicy,
    Entry::ExportReport,
    Entry::ExportPlan,
    Entry::ExportCatalogue,
    Entry::ExportPolicy,
    Entry::SourceValidate,
    Entry::SourcePlan,
    Entry::SourceCollect,
    Entry::SourceAssess,
    Entry::CollectContext,
    Entry::AssessContext,
    Entry::Registry,
    Entry::ScopedInputs,
    Entry::ArtifactInputs,
    Entry::ReviewInputs,
    Entry::RetainedArtifact,
];

struct Fixtures {
    directory: TempDir,
    request: PathBuf,
    case: PathBuf,
    report: PathBuf,
    source: PathBuf,
    registry: PathBuf,
}
impl Fixtures {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let request = directory.path().join("request.json");
        let case = directory.path().join("case.json");
        let report = directory.path().join("report.json");
        let source = directory.path().join("source.eal");
        let registry = directory.path().join("tools.toml");
        fs::write(&request, include_bytes!("../examples/request.json")).unwrap();
        fs::write(&case, include_bytes!("../examples/case.json")).unwrap();
        fs::write(&report, serde_json::to_vec(&json!({
            "request":serde_json::from_slice::<Value>(include_bytes!("../examples/request.json")).unwrap(),
            "argument":serde_json::from_slice::<Value>(include_bytes!("../examples/case.json")).unwrap()
        })).unwrap()).unwrap();
        fs::write(&source, "language \"EAL/3\"\n").unwrap();
        fs::write(&registry, "").unwrap();
        Self {
            directory,
            request,
            case,
            report,
            source,
            registry,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}

impl Entry {
    fn limit(self) -> usize {
        match self {
            Self::AnalyseRequest => 4096,
            Self::AnalyseTools
            | Self::ValidateRequest
            | Self::ValidateCase
            | Self::PrepareCatalogue
            | Self::PreparePolicy
            | Self::ExportCatalogue
            | Self::ExportPolicy
            | Self::Registry
            | Self::ScopedInputs
            | Self::ArtifactInputs
            | Self::ReviewInputs
            | Self::RetainedArtifact => MIB,
            _ => 16 * MIB,
        }
    }
    fn command(self, fixtures: &Fixtures, target: &Path) -> (Command, Option<Vec<u8>>) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
        command
            .env_remove("ENGARG_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("ENGARG_MODEL");
        let mut input = None;
        match self {
            Self::AnalyseRequest | Self::AnalyseTools => {
                command.args([
                    "analyse",
                    "--model",
                    "fixture",
                    "--endpoint",
                    "http://127.0.0.1:1/v1/responses",
                    "--timeout-secs",
                    "1",
                    "--http-retries",
                    "0",
                    "--max-input-bytes",
                    "4096",
                ]);
                if matches!(self, Self::AnalyseRequest) {
                    command.arg("--input").arg(target);
                } else {
                    command
                        .args(["--claim", "A bounded claim", "--tools"])
                        .arg(target);
                }
            }
            Self::RenderReport => {
                command
                    .args(["render", "--input"])
                    .arg(target)
                    .args(["--format", "json"]);
            }
            Self::ValidateRequest | Self::ValidateCase => {
                command.args(["validate", "--input"]).arg(
                    if matches!(self, Self::ValidateRequest) {
                        target
                    } else {
                        &fixtures.request
                    },
                );
                command
                    .arg("--case")
                    .arg(if matches!(self, Self::ValidateCase) {
                        target
                    } else {
                        &fixtures.case
                    });
            }
            Self::PrepareReport | Self::PrepareCatalogue | Self::PreparePolicy => {
                command.args(["eal", "prepare", "--input"]).arg(
                    if matches!(self, Self::PrepareReport) {
                        target
                    } else {
                        &fixtures.report
                    },
                );
                command.arg("--output-dir").arg(fixtures.path("prepared"));
                if matches!(self, Self::PrepareCatalogue) {
                    command.arg("--collector-catalogue").arg(target);
                }
                if matches!(self, Self::PreparePolicy) {
                    command.arg("--export-policy").arg(target);
                }
            }
            Self::ExportReport | Self::ExportPlan | Self::ExportCatalogue | Self::ExportPolicy => {
                command.args(["eal", "export", "--input"]).arg(
                    if matches!(self, Self::ExportReport) {
                        target
                    } else {
                        &fixtures.report
                    },
                );
                command.arg("--output").arg(fixtures.path("export.eal"));
                match self {
                    Self::ExportPlan => {
                        command.arg("--acquisition-plan").arg(target);
                    }
                    Self::ExportCatalogue => {
                        command.arg("--collector-catalogue").arg(target);
                    }
                    Self::ExportPolicy => {
                        command.arg("--export-policy").arg(target);
                    }
                    _ => {}
                }
            }
            Self::SourceValidate
            | Self::SourcePlan
            | Self::SourceCollect
            | Self::SourceAssess
            | Self::CollectContext
            | Self::AssessContext => {
                let operation = match self {
                    Self::SourceValidate => "validate",
                    Self::SourcePlan => "plan",
                    Self::SourceCollect | Self::CollectContext => "collect",
                    _ => "assess",
                };
                command.args(["eal", operation, "--input"]).arg(
                    if matches!(self, Self::CollectContext | Self::AssessContext) {
                        &fixtures.source
                    } else {
                        target
                    },
                );
                command
                    .args(["--input-format", "eal", "--eal-host"])
                    .arg(fixtures.path("host-must-not-start"))
                    .args(["--timeout-secs", "1"]);
                if matches!(self, Self::SourceCollect | Self::CollectContext) {
                    command.arg("--tools").arg(&fixtures.registry);
                }
                if matches!(self, Self::SourceAssess | Self::AssessContext) {
                    command.args(["--now", "2026-01-01T00:00:00Z"]);
                }
                if matches!(self, Self::CollectContext | Self::AssessContext) {
                    command.arg("--context").arg(target);
                }
            }
            Self::Registry => {
                command
                    .args(["eal", "describe", "--tools"])
                    .arg(target)
                    .arg("--eal-host")
                    .arg(fixtures.path("host-must-not-start"))
                    .args(["--timeout-secs", "1"]);
            }
            Self::ScopedInputs
            | Self::ArtifactInputs
            | Self::ReviewInputs
            | Self::RetainedArtifact => {
                let tool = match self {
                    Self::ScopedInputs => "scoped_json",
                    Self::ReviewInputs => "review_json",
                    _ => "artifact_json",
                };
                command.args(["eal", "observe", "--tool", tool]);
                let mut request = json!({"evidence_id":"acquisition_E1","environment":"engarg_case","tool":"acquisition_tool_E1","tool_version":"1",
                    "input":{"schema":"engarg.eal-acquisition/1","case_digest":DIGEST,"need":{"id":"E1"},"arguments":{"inputs":target}},
                    "context":{"engarg_export_digest":DIGEST}});
                if matches!(self, Self::ReviewInputs) {
                    request["tool"] = json!("engarg_review_contract");
                    request["input"]["obligation"] = json!({"id":"review:E1"});
                    command.arg("--inputs").arg(target);
                }
                if matches!(self, Self::RetainedArtifact) {
                    let inputs = fixtures.path("artifact-inputs.json");
                    fs::write(&inputs, serde_json::to_vec(&json!({"schema":"engarg.artifact-inputs/1","case_digest":DIGEST,
                        "needs":{"E1":{"artifacts":[{"path":target,"sha256":DIGEST,"observed_at":"2025-01-01T00:00:00Z","source":"independently retained fixture"}]}}})).unwrap()).unwrap();
                    request["input"]["arguments"]["inputs"] = json!(inputs);
                }
                input = Some(serde_json::to_vec(&request).unwrap());
            }
        }
        (command, input)
    }
}

/// A parent watchdog makes a regression fail without leaving a blocked child.
/// Production deadlines remain responsible for production work.
fn output(entry: Entry, fixtures: &Fixtures, path: &Path) -> Output {
    let (mut command, input) = entry.command(fixtures, path);
    // Regular capture files avoid waiting for EOF from an unrelated descendant
    // after the directly supervised CLI has already exited.
    let stdout_path = fixtures.path("capture.stdout");
    let stderr_path = fixtures.path("capture.stderr");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(File::create(&stdout_path).unwrap())
        .stderr(File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(&input).unwrap();
    } else {
        drop(child.stdin.take());
    }
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{entry:?} blocked on {path:?}: {}",
                String::from_utf8_lossy(&fs::read(&stderr_path).unwrap())
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Output {
        status,
        stdout: fs::read(stdout_path).unwrap(),
        stderr: fs::read(stderr_path).unwrap(),
    }
}

fn failure(entry: Entry, fixtures: &Fixtures, path: &Path) -> String {
    let output = output(entry, fixtures, path);
    assert!(!output.status.success(), "{entry:?} accepted {path:?}");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn every_public_input_rejects_missing_directory_empty_malformed_and_invalid_utf8() {
    for entry in ENTRIES {
        for payload in [None, Some(&b""[..]), Some(&b"{"[..]), Some(&b"\xff"[..])] {
            let fixtures = Fixtures::new();
            let path = fixtures.path("selected-input");
            if let Some(bytes) = payload {
                fs::write(&path, bytes).unwrap();
            }
            failure(*entry, &fixtures, &path);
        }
        let fixtures = Fixtures::new();
        let path = fixtures.path("input-directory");
        fs::create_dir(&path).unwrap();
        failure(*entry, &fixtures, &path);
    }
}

#[test]
fn every_file_entry_checks_sparse_size_before_parsing() {
    for entry in ENTRIES {
        let fixtures = Fixtures::new();
        let path = fixtures.path("oversized-input");
        File::create(&path)
            .unwrap()
            .set_len(entry.limit() as u64 + 1)
            .unwrap();
        let message = failure(*entry, &fixtures, &path);
        assert!(
            message.contains("byte limit") || message.contains("exceeds"),
            "{entry:?}: {message}"
        );
    }
}

#[test]
fn exact_limits_are_admitted_before_format_or_schema_validation() {
    for entry in ENTRIES {
        let fixtures = Fixtures::new();
        let path = fixtures.path("exact-input");
        // Invalid semantics deliberately stop processing after the file boundary.
        // The independently specified size is admitted without requiring a host.
        let mut bytes = vec![b' '; entry.limit()];
        bytes[..2].copy_from_slice(b"{}");
        fs::write(&path, bytes).unwrap();
        let result = output(*entry, &fixtures, &path);
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(
            !message.contains("byte limit") && !message.contains("exceeds 1 MiB"),
            "{entry:?}: {message}"
        );
    }
}

#[cfg(unix)]
#[test]
fn fifo_and_fifo_aliases_are_rejected_through_every_public_input() {
    use std::os::unix::fs::symlink;
    for entry in ENTRIES {
        let fixtures = Fixtures::new();
        let fifo = fixtures.path("input.fifo");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let alias = fixtures.path("fifo-alias");
        symlink(&fifo, &alias).unwrap();
        for path in [&fifo, &alias] {
            failure(*entry, &fixtures, path);
        }
    }
}

#[cfg(unix)]
#[test]
fn untrusted_leaf_aliases_are_rejected_and_registry_aliases_reach_host_admission() {
    use std::os::unix::fs::symlink;
    for entry in ENTRIES {
        let fixtures = Fixtures::new();
        let real = fixtures.path("real-input");
        fs::write(&real, b"{}").unwrap();
        let alias = fixtures.path("input-alias");
        symlink(&real, &alias).unwrap();
        let message = failure(*entry, &fixtures, &alias);
        if matches!(entry, Entry::Registry) {
            // The selected canonical file passes file admission. The deliberately
            // absent host then fails without an external process or service.
            assert!(
                !message.contains("registry must") && !message.contains("regular file"),
                "{message}"
            );
        } else {
            assert!(
                message.contains("regular file") || message.contains("symbolic link"),
                "{entry:?}: {message}"
            );
        }
    }
}

#[test]
fn bounded_reader_handles_empty_exact_sparse_and_missing_files_directly() {
    let fixtures = Fixtures::new();
    let path = fixtures.path("bounded");
    fs::write(&path, b"").unwrap();
    assert_eq!(bounded_file::read(&path, 0, "fixture").unwrap(), b"");
    fs::write(&path, b"1234").unwrap();
    assert_eq!(bounded_file::read(&path, 4, "fixture").unwrap(), b"1234");
    assert!(matches!(
        bounded_file::read(&path, 3, "fixture"),
        Err(Error::Budget(_))
    ));
    File::create(&path)
        .unwrap()
        .set_len(8 * 1024 * 1024 * 1024)
        .unwrap();
    assert!(matches!(
        bounded_file::read(&path, 4, "fixture"),
        Err(Error::Budget(_))
    ));
    assert!(bounded_file::read(&fixtures.path("missing"), 4, "fixture").is_err());
}

#[cfg(unix)]
#[test]
fn parent_aliases_resolve_once_while_untrusted_leaf_aliases_remain_forbidden() {
    use std::os::unix::fs::symlink;
    let fixtures = Fixtures::new();
    let first = fixtures.path("first");
    let second = fixtures.path("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    fs::write(first.join("input"), b"first").unwrap();
    fs::write(second.join("input"), b"second").unwrap();
    let parent = fixtures.path("parent-alias");
    symlink(&first, &parent).unwrap();
    assert_eq!(
        bounded_file::read(&parent.join("input"), 6, "fixture").unwrap(),
        b"first"
    );
    let pinned = parent.canonicalize().unwrap().join("input");
    fs::remove_file(&parent).unwrap();
    symlink(&second, &parent).unwrap();
    assert_eq!(bounded_file::read(&pinned, 6, "fixture").unwrap(), b"first");
    assert_eq!(
        bounded_file::read(&parent.join("input"), 6, "fixture").unwrap(),
        b"second"
    );
    let leaf = fixtures.path("leaf-alias");
    symlink(&pinned, &leaf).unwrap();
    assert!(bounded_file::read(&leaf, 6, "fixture").is_err());
}

#[test]
fn concurrent_path_replacement_returns_one_identified_file_or_a_safe_error() {
    use std::sync::{Arc, Barrier};
    let fixtures = Fixtures::new();
    let target = fixtures.path("replaced-input");
    let first = vec![b'A'; 16_384];
    let second = vec![b'B'; 16_384];
    fs::write(&target, &first).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = Arc::clone(&barrier);
    let writer_target = target.clone();
    let writer_directory = fixtures.directory.path().to_owned();
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        for revision in 0..256 {
            let pending = writer_directory.join("replacement-pending");
            fs::write(
                &pending,
                vec![if revision % 2 == 0 { b'B' } else { b'A' }; 16_384],
            )
            .unwrap();
            fs::rename(pending, &writer_target).unwrap();
        }
    });
    barrier.wait();
    let mut outcomes = Vec::new();
    for _ in 0..128 {
        outcomes.push(bounded_file::read(&target, 16_384, "fixture"));
    }
    writer.join().unwrap();
    for outcome in outcomes {
        match outcome {
            Ok(bytes) => assert!(bytes == first || bytes == second),
            Err(Error::InvalidInput(_)) | Err(Error::Io(_)) => {}
            Err(error) => panic!("Unexpected replacement outcome: {error}"),
        }
    }
}
