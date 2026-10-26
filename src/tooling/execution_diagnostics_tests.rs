//! Infrastructure faults retain operator diagnostics and never enter source repair.
use super::*;
use crate::tooling::{
    contracts::digest_json,
    package::PackageStore,
    verification::tests::{candidate, contract, identity},
    verified_build::{BuildAttempt, VerifiedBuild},
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

struct OutcomeRunner {
    identity: RunnerIdentity,
    code: i32,
    label: Option<&'static str>,
    before_result: bool,
    calls: AtomicUsize,
}

impl OutcomeRunner {
    fn new(profile: &ToolchainProfile, code: i32, label: Option<&'static str>) -> Self {
        let mut identity = identity();
        identity.profile_sha256 = digest_json(profile).unwrap();
        Self {
            identity,
            code,
            label,
            before_result: false,
            calls: AtomicUsize::new(0),
        }
    }
}

impl FixtureRunner for OutcomeRunner {
    fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }

    fn execute(
        &self,
        _root: &Path,
        _source: &[SourceFile],
        _fixtures: &[SourceFile],
        _request: &[u8],
        _mode: ExecutionMode,
        _cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.before_result {
            return Err(Error::Provider("supervisor failed secret-value".into()));
        }
        Ok(RunOutcome {
            stdout: Vec::new(),
            stderr: b"container diagnostic secret-value".to_vec(),
            returncode: self.code,
            execution_error: self.label.map(str::to_owned),
            elapsed_ms: 19,
        })
    }
}

fn profile() -> ToolchainProfile {
    ToolchainProfile {
        schema: "engarg.toolchain/1".into(),
        language: "python".into(),
        docker: "docker".into(),
        image: "python:3.13-slim".into(),
        build_argv: Vec::new(),
        run_argv: vec!["python3".into(), "/src/collector.py".into()],
        inherit_env: Vec::new(),
        runtime_network: "none".into(),
        timeout_seconds: 60,
        memory_mb: 1024,
        output_bytes: 1_048_576,
    }
}

#[test]
fn infrastructure_receipts_preserve_status_class_bounds_and_redaction() {
    for (code, label, class) in [
        (125, None, "container_exit"),
        (124, Some("timeout"), "timeout"),
        (137, Some("container_execution_error"), "container_exit"),
        (-1, Some("output_limit"), "output_limit"),
        (-1, Some("cancelled"), "cancelled"),
        (-1, Some("spawn_error"), "supervisor_failure"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let selected = profile();
        let inner = Arc::new(OutcomeRunner::new(&selected, code, label));
        let runner = DiagnosticRunner::new(inner.clone(), contract(), vec!["secret-value".into()]);
        let cancellation = CancellationToken::default();
        runner
            .execute(
                directory.path(),
                &[],
                &[],
                b"{}",
                ExecutionMode::Fixture,
                &cancellation,
            )
            .unwrap();
        // The first infrastructure fault prevents all subsequent fixture launches.
        assert!(
            runner
                .execute(
                    directory.path(),
                    &[],
                    &[],
                    b"{}",
                    ExecutionMode::Fixture,
                    &cancellation
                )
                .is_err()
        );
        assert_eq!(inner.calls.load(Ordering::Relaxed), 1);
        let error = runner.finish(directory.path(), &selected).unwrap_err();
        assert!(matches!(error, Error::ToolExecution(_)));
        let message = error.to_string();
        assert!(message.contains(class));
        assert!(message.contains(&format!("exit status {code}")));
        assert!(message.contains("execution-failure.json"));
        assert!(!message.contains("secret-value"));
        let bytes = std::fs::read(directory.path().join(FAILURE_FILE)).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("secret-value"));
        let receipt: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(receipt["schema"], FAILURE_SCHEMA);
        assert_eq!(receipt["stage"], "verification");
        assert_eq!(receipt["failure"]["class"], class);
        assert_eq!(receipt["failure"]["returncode"], code);
        assert_eq!(receipt["failure"]["elapsed_ms"], 19);
        assert_eq!(
            receipt["failure"]["stderr"],
            "container diagnostic [redacted]"
        );
        assert_eq!(
            receipt["bounds"],
            json!({
                "timeout_seconds":60, "memory_mb":1024,
                "max_stdout_bytes":1_048_576, "max_stderr_bytes":1_048_576
            })
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(directory.path().join(FAILURE_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn runner_failure_has_no_fabricated_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let selected = profile();
    let mut inner = OutcomeRunner::new(&selected, 0, None);
    inner.before_result = true;
    let runner = DiagnosticRunner::new(Arc::new(inner), contract(), vec!["secret-value".into()]);
    assert!(
        runner
            .execute(
                directory.path(),
                &[],
                &[],
                b"{}",
                ExecutionMode::Fixture,
                &CancellationToken::default()
            )
            .is_err()
    );
    let error = runner.finish(directory.path(), &selected).unwrap_err();
    assert!(!error.to_string().contains("secret-value"));
    let receipt: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join(FAILURE_FILE)).unwrap())
            .unwrap();
    assert_eq!(receipt["failure"]["class"], "runner_failure");
    assert!(receipt["failure"]["returncode"].is_null());
    assert_eq!(
        receipt["failure"]["diagnostic"],
        "provider error: supervisor failed [redacted]"
    );
}

#[tokio::test]
async fn build_retains_infrastructure_receipt_but_compiler_exit_one_remains_repairable() {
    for code in [125, 1] {
        let directory = tempfile::tempdir().unwrap();
        let store = PackageStore::new(directory.path()).unwrap();
        let selected = profile();
        let runner = Arc::new(OutcomeRunner::new(&selected, code, None));
        let cancellation = CancellationToken::default();
        let key = "5".repeat(64);
        let build = VerifiedBuild {
            store: &store,
            key: &key,
            profile: &selected,
            runner,
            cancellation: &cancellation,
        };
        let response = serde_json::to_vec(&candidate()).unwrap();
        let observed = AtomicUsize::new(0);
        let result = build
            .attempt(contract(), &response, |_| {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .await;
        assert_eq!(observed.load(Ordering::Relaxed), 1);
        let attempt = std::fs::read_dir(directory.path().join("attempts"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(attempt.join("test-results.json").is_file());
        assert!(attempt.join("candidate.json").is_file());
        if code == 125 {
            assert!(matches!(result, Err(Error::ToolExecution(_))));
            assert!(attempt.join(FAILURE_FILE).is_file());
            assert!(!attempt.join("manifest.json").exists());
        } else {
            assert!(matches!(result, Ok(BuildAttempt::Rejected(_))));
            assert!(attempt.join("build-failure.json").is_file());
            assert!(!attempt.join(FAILURE_FILE).exists());
        }
    }
}
