//! Exercise prepared evidence APIs through the same isolated runner as generated tools.
use engarg::tooling::{
    contracts::SourceFile,
    profile::ToolchainConfiguration,
    runner::{DockerRunner, ExecutionMode, FixtureRunner},
};
use std::{fs, path::Path};

fn exercise(profile_name: &str, smoke: &str, entry: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/toolchains");
    let configuration =
        ToolchainConfiguration::load(&root.join(format!("{profile_name}.json"))).unwrap();
    let mut profile = configuration.profile;
    assert_eq!(profile.runtime_network, "none");
    assert!(profile.inherit_env.is_empty());
    if let Ok(docker) = std::env::var("ENGARG_TOOL_DOCKER") {
        profile.docker = docker;
    }
    let supervisor =
        Path::new(env!("CARGO_BIN_EXE_engarg")).with_file_name("engarg-process-supervisor");
    let runner = DockerRunner::new(profile, supervisor).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let result = runner
        .execute(
            directory.path(),
            &[SourceFile {
                path: entry.into(),
                content: fs::read_to_string(root.join(smoke)).unwrap(),
            }],
            &[],
            b"{}",
            ExecutionMode::Fixture,
            &eal_process::CancellationToken::default(),
        )
        .unwrap();
    assert!(
        result.returncode == 0 && result.execution_error.is_none(),
        "{profile_name}: {result:?}"
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("evidence-packages-ok"),
        "smoke did not reach its final assertion: {result:?}"
    );
}

#[test]
#[ignore = "requires the prepared Python evidence image and Docker"]
fn python_evidence_apis_execute_offline() {
    exercise(
        "python-evidence",
        "python-evidence/smoke.py",
        "collector.py",
    );
}

#[test]
#[ignore = "requires the prepared Rust evidence image and Docker"]
fn rust_evidence_apis_execute_offline() {
    exercise("rust", "rust/smoke.rs", "main.rs");
}

#[test]
#[ignore = "requires the prepared Go evidence image and Docker"]
fn go_evidence_apis_execute_offline() {
    exercise("go", "go/smoke.go", "main.go");
}

#[test]
#[ignore = "requires the prepared JavaScript evidence image and Docker"]
fn javascript_evidence_apis_execute_offline() {
    exercise(
        "javascript",
        "typescript/evidence-smoke.mjs",
        "collector.mjs",
    );
}

#[test]
#[ignore = "requires the prepared TypeScript evidence image and Docker"]
fn typescript_evidence_apis_execute_offline() {
    exercise("typescript", "typescript/evidence-smoke.ts", "collector.ts");
}
