use super::*;
use crate::tooling::{
    contracts::SourceFile,
    runner::{ExecutionMode, RunOutcome},
    verification::tests::{candidate, contract, identity},
};
use eal_collector_contracts::AcquisitionRequest;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn retained() -> (tempfile::TempDir, PathBuf, RunnerIdentity) {
    let directory = tempfile::tempdir().unwrap();
    let store = PackageStore::new(directory.path()).unwrap();
    let contract = contract();
    let candidate = candidate();
    let profile = ToolchainProfile {
        schema: super::super::profile::TOOLCHAIN_SCHEMA.into(),
        language: "python".into(),
        docker: "docker".into(),
        image: "python:3.13-slim".into(),
        build_argv: vec![],
        run_argv: vec!["python3".into(), "/src/collector.py".into()],
        inherit_env: vec![],
        runtime_network: "none".into(),
        timeout_seconds: 30,
        memory_mb: 128,
        output_bytes: 1_048_576,
    };
    let mut identity = identity();
    identity.profile_sha256 = digest_json(&profile).unwrap();
    let key = PackageStore::key_for(
        &contract.need,
        &contract.request_context,
        &contract.language,
        &identity,
    )
    .unwrap();
    store.freeze_contract(&key, &contract, &identity).unwrap();
    let attempt = store.new_attempt(&key).unwrap();
    store
        .save_candidate(&attempt, &contract, &candidate, &profile, &identity)
        .unwrap();
    fs::write(
        attempt.join("candidate-response.json"),
        serde_json::to_vec(&candidate).unwrap(),
    )
    .unwrap();
    (directory, attempt, identity)
}

struct PassingRunner {
    identity: RunnerIdentity,
    calls: AtomicUsize,
}
impl FixtureRunner for PassingRunner {
    fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }
    fn execute(
        &self,
        _root: &Path,
        _source: &[SourceFile],
        _fixtures: &[SourceFile],
        bytes: &[u8],
        _mode: ExecutionMode,
        _cancel: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let parsed = eal_collector_contracts::strict_json_exact(bytes, Default::default())
            .ok()
            .and_then(|value| serde_json::from_value::<AcquisitionRequest>(value).ok());
        let valid = parsed.filter(|request| request.validate().is_ok());
        let mut outcome = RunOutcome {
            stdout: vec![],
            stderr: vec![],
            returncode: 1,
            execution_error: None,
            elapsed_ms: 1,
        };
        if let Some(request) = valid {
            let mode = request.input["mode"].as_str().unwrap_or("error");
            if matches!(mode, "positive" | "negative") {
                outcome.returncode = 0;
                outcome.stdout = serde_json::to_vec(
                    &json!({"value":{"collected":true,"present":mode=="positive"},"context":request.context,"request":request.acquisition()}),
                )?;
            }
        }
        Ok(outcome)
    }
}

#[tokio::test]
async fn replay_keeps_old_source_and_fixtures_and_rebuilds_cache_without_a_model() {
    let (directory, attempt, mut identity) = retained();
    let before = fs::read(attempt.join("candidate-response.json")).unwrap();
    identity.runner_sha256 = "f".repeat(64);
    let runner = Arc::new(PassingRunner {
        identity: identity.clone(),
        calls: AtomicUsize::new(0),
    });
    let receipt = replay_with_runner(
        RetainedAttempt::load(&attempt).unwrap(),
        directory.path(),
        runner.clone(),
        &CancellationToken::default(),
    )
    .await
    .unwrap();
    assert!(receipt.passed);
    assert_eq!(receipt.model_calls, 0);
    assert_ne!(receipt.manifest.parent().unwrap(), attempt);
    assert_eq!(
        before,
        fs::read(attempt.join("candidate-response.json")).unwrap()
    );
    assert!(!attempt.join("manifest.json").exists());
    let package = super::super::package::load_verified(&receipt.manifest).unwrap();
    assert_eq!(
        contract_digest(package.contract()).unwrap(),
        contract_digest(&contract()).unwrap()
    );
    let key = PackageStore::key_for(
        &package.contract().need,
        &package.contract().request_context,
        &package.contract().language,
        &identity,
    )
    .unwrap();
    assert!(
        PackageStore::new(directory.path())
            .unwrap()
            .lookup(&key, &identity)
            .unwrap()
            .is_some()
    );
    assert!(runner.calls.load(Ordering::Relaxed) > 3);
    assert!(
        receipt
            .manifest
            .with_file_name("replay-origin.json")
            .is_file()
    );
}

#[test]
fn replay_rejects_modified_source_and_contract_before_execution() {
    for file in [
        "source/collector.py",
        "contract.json",
        "candidate-response.json",
    ] {
        let (_directory, attempt, _) = retained();
        fs::write(attempt.join(file), b"{}").unwrap();
        assert!(RetainedAttempt::load(&attempt).is_err(), "{file}");
    }
}

#[tokio::test]
async fn replay_refuses_changed_image_before_any_execution() {
    let (directory, attempt, mut identity) = retained();
    identity.image_id = format!("sha256:{}", "e".repeat(64));
    let runner = Arc::new(PassingRunner {
        identity,
        calls: AtomicUsize::new(0),
    });
    assert!(
        replay_with_runner(
            RetainedAttempt::load(&attempt).unwrap(),
            directory.path(),
            runner.clone(),
            &CancellationToken::default()
        )
        .await
        .is_err()
    );
    assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn cancelled_replay_cannot_create_a_verified_package() {
    let (directory, attempt, identity) = retained();
    let runner = Arc::new(PassingRunner {
        identity,
        calls: AtomicUsize::new(0),
    });
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(
        replay_with_runner(
            RetainedAttempt::load(&attempt).unwrap(),
            directory.path(),
            runner.clone(),
            &cancel
        )
        .await
        .is_err()
    );
    assert_eq!(runner.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn dropped_replay_retains_cancellation_and_cannot_commit_its_package() {
    struct WaitingRunner {
        identity: RunnerIdentity,
        started: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    }
    impl FixtureRunner for WaitingRunner {
        fn identity(&self) -> &RunnerIdentity {
            &self.identity
        }
        fn execute(
            &self,
            _: &Path,
            _: &[SourceFile],
            _: &[SourceFile],
            _: &[u8],
            _: ExecutionMode,
            cancellation: &CancellationToken,
        ) -> Result<RunOutcome, Error> {
            if let Some(started) = self.started.lock().unwrap().take() {
                let _ = started.send(());
            }
            while !cancellation.is_cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Ok(RunOutcome {
                stdout: vec![],
                stderr: vec![],
                returncode: 124,
                execution_error: Some("cancelled".into()),
                elapsed_ms: 10,
            })
        }
    }
    let (directory, attempt, identity) = retained();
    let (started, observed) = tokio::sync::oneshot::channel();
    let runner = Arc::new(WaitingRunner {
        identity,
        started: std::sync::Mutex::new(Some(started)),
    });
    let cancellation = CancellationToken::default();
    let mut work = Box::pin(replay_with_runner(
        RetainedAttempt::load(&attempt).unwrap(),
        directory.path(),
        runner,
        &cancellation,
    ));
    tokio::select! {
        started=observed => started.unwrap(),
        result=&mut work => panic!("replay finished before cancellation: {result:?}"),
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), work.as_mut())
            .await
            .is_err()
    );
    drop(work);
    cancellation.cancel();
    let failure = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            for entry in fs::read_dir(directory.path().join("attempts")).unwrap() {
                let path = entry.unwrap().path().join("execution-failure.json");
                if path.is_file() {
                    return path;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(failure).unwrap()).unwrap();
    assert_eq!(receipt["failure"]["class"], "cancelled");
    assert_eq!(
        fs::read_dir(directory.path().join("index"))
            .unwrap()
            .count(),
        0
    );
}
