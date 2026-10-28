//! Exercise the selected compiler and restricted mounts before model generation.
use super::{
    contracts::SourceFile,
    profile::ToolchainProfile,
    runner::{DockerRunner, ExecutionMode, FixtureRunner, RunnerIdentity},
    verified_build::{feedback_redactions, redact_feedback},
};
use crate::Error;
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Instant};

const PROBE_INPUT: &[u8] = b"engarg-toolchain-check/1\n";
const PROBE_OUTPUT: &[u8] = b"{\"schema\":\"engarg.toolchain-probe/1\",\"passed\":true}\n";
const FIXTURE_TEXT: &str = "engarg-read-only-fixture/1\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainCheck {
    pub schema: String,
    pub language: String,
    pub runner: Option<RunnerIdentity>,
    pub stage: String,
    pub passed: bool,
    pub returncode: Option<i32>,
    pub execution_error: Option<String>,
    pub elapsed_ms: u64,
    pub timeout_seconds: u64,
    pub memory_mb: u64,
    pub output_bytes: usize,
    pub stderr: String,
    pub diagnostic: Option<String>,
}

impl ToolchainCheck {
    fn new(profile: &ToolchainProfile) -> Self {
        Self {
            schema: "engarg.toolchain-check/1".into(),
            language: profile.language.clone(),
            runner: None,
            stage: "admission".into(),
            passed: false,
            returncode: None,
            execution_error: None,
            elapsed_ms: 0,
            timeout_seconds: profile.timeout_seconds,
            memory_mb: profile.memory_mb,
            output_bytes: profile.output_bytes,
            stderr: String::new(),
            diagnostic: None,
        }
    }

    pub fn ensure_passed(&self) -> Result<(), Error> {
        if self.passed {
            return Ok(());
        }
        Err(Error::ToolExecution(format!(
            "toolchain preflight failed during {}; exit status {}; {}; stderr: {}",
            self.stage,
            self.returncode
                .map_or_else(|| "unavailable".into(), |code| code.to_string()),
            self.execution_error
                .as_deref()
                .or(self.diagnostic.as_deref())
                .unwrap_or("unknown execution failure"),
            self.stderr,
        )))
    }
}

fn bounded_redacted(text: &str, sensitive: &[String]) -> String {
    let redacted = redact_feedback(text, sensitive);
    let mut end = redacted.len().min(4096);
    while !redacted.is_char_boundary(end) {
        end -= 1;
    }
    redacted[..end].to_owned()
}

/// Admission and execution failures are data so callers can retain a failed receipt.
pub fn check(profile: ToolchainProfile, supervisor: &Path) -> Result<ToolchainCheck, Error> {
    profile.validate()?;
    let sensitive = feedback_redactions(&profile);
    let started = Instant::now();
    match DockerRunner::new(profile.clone(), supervisor.to_path_buf()) {
        Ok(runner) => Ok(check_runner(&runner, &profile)),
        Err(error) => {
            let mut receipt = ToolchainCheck::new(&profile);
            receipt.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            receipt.diagnostic = Some(bounded_redacted(&error.to_string(), &sensitive));
            Ok(receipt)
        }
    }
}

/// The same runner, image identity, commands, limits and fixture isolation are used
/// for this host-owned probe and later generated source. No runtime credentials enter it.
pub fn check_runner<R: FixtureRunner>(runner: &R, profile: &ToolchainProfile) -> ToolchainCheck {
    let sensitive = feedback_redactions(profile);
    let mut receipt = ToolchainCheck::new(profile);
    receipt.runner = Some(runner.identity().clone());
    receipt.stage = "execution".into();
    let source = match probe_source(profile) {
        Ok(source) => source,
        Err(error) => {
            receipt.stage = "probe_configuration".into();
            receipt.diagnostic = Some(bounded_redacted(&error.to_string(), &sensitive));
            return receipt;
        }
    };
    let fixture = SourceFile {
        path: "engarg-preflight.txt".into(),
        content: FIXTURE_TEXT.into(),
    };
    let started = Instant::now();
    match runner.execute(
        Path::new("."),
        &[source],
        &[fixture],
        PROBE_INPUT,
        ExecutionMode::Fixture,
        &CancellationToken::default(),
    ) {
        Ok(outcome) => {
            receipt.returncode = Some(outcome.returncode);
            receipt.execution_error = outcome.execution_error;
            receipt.elapsed_ms = outcome.elapsed_ms;
            receipt.stderr =
                bounded_redacted(&String::from_utf8_lossy(&outcome.stderr), &sensitive);
            receipt.passed = outcome.returncode == 0
                && receipt.execution_error.is_none()
                && outcome.stdout == PROBE_OUTPUT;
            if !receipt.passed {
                receipt.diagnostic = Some(
                    if outcome.stdout != PROBE_OUTPUT && outcome.returncode == 0 {
                        "toolchain probe did not return its exact synthetic success response".into()
                    } else {
                        "toolchain probe did not complete normally".into()
                    },
                );
            }
        }
        Err(error) => {
            receipt.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            receipt.diagnostic = Some(bounded_redacted(&error.to_string(), &sensitive));
        }
    }
    receipt
}

fn probe_source(profile: &ToolchainProfile) -> Result<SourceFile, Error> {
    let (extension, default_path, content) = match profile.language.as_str() {
        "rust" => (
            "rs",
            "main.rs",
            include_str!("runner_preflight/rust.rs.txt"),
        ),
        "go" => ("go", "main.go", include_str!("runner_preflight/go.go.txt")),
        "typescript" => (
            "ts",
            "collector.ts",
            include_str!("runner_preflight/typescript.ts.txt"),
        ),
        "python" => (
            "py",
            "collector.py",
            include_str!("runner_preflight/python.py.txt"),
        ),
        "javascript" => (
            "mjs",
            "collector.mjs",
            include_str!("runner_preflight/javascript.mjs.txt"),
        ),
        _ => {
            return Err(Error::InvalidInput(
                "unsupported toolchain probe language".into(),
            ));
        }
    };
    let mut paths: Vec<_> = profile
        .build_argv
        .iter()
        .chain(&profile.run_argv)
        .filter_map(|argument| argument.strip_prefix("/src/"))
        .filter(|path| path.ends_with(&format!(".{extension}")))
        .collect();
    // CommonJS is also a supported JavaScript source entry point.
    if profile.language == "javascript" {
        paths.extend(
            profile
                .build_argv
                .iter()
                .chain(&profile.run_argv)
                .filter_map(|argument| argument.strip_prefix("/src/"))
                .filter(|path| path.ends_with(".js")),
        );
    }
    paths.sort_unstable();
    paths.dedup();
    // Wrappers can own their source path internally. Supply the documented
    // language entry when argv contains no literal source pathname.
    if paths.is_empty() {
        paths.push(default_path);
    }
    let [path] = paths.as_slice() else {
        return Err(Error::InvalidInput(
            "toolchain preflight requires one source entry in the configured commands".into(),
        ));
    };
    if Path::new(path)
        .components()
        .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(Error::InvalidInput(
            "toolchain probe source path must be relative and traversal-free".into(),
        ));
    }
    Ok(SourceFile {
        path: (*path).into(),
        content: content.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::runner::RunOutcome;
    use super::*;
    use std::sync::Mutex;

    struct ProbeRunner {
        identity: RunnerIdentity,
        outcome: RunOutcome,
        called: Mutex<bool>,
    }
    impl FixtureRunner for ProbeRunner {
        fn identity(&self) -> &RunnerIdentity {
            &self.identity
        }
        fn execute(
            &self,
            _: &Path,
            source: &[SourceFile],
            fixtures: &[SourceFile],
            input: &[u8],
            mode: ExecutionMode,
            _: &CancellationToken,
        ) -> Result<RunOutcome, Error> {
            assert_eq!(mode, ExecutionMode::Fixture);
            assert_eq!(source[0].path, "main.rs");
            assert!(source[0].content.contains("/fixtures/engarg-preflight.txt"));
            assert_eq!(fixtures[0].content, FIXTURE_TEXT);
            assert_eq!(input, PROBE_INPUT);
            *self.called.lock().unwrap() = true;
            Ok(self.outcome.clone())
        }
    }
    fn profile() -> ToolchainProfile {
        super::super::profile::ToolchainConfiguration::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/toolchains/rust.json"),
        )
        .unwrap()
        .profile
    }
    fn runner(returncode: i32, execution_error: Option<&str>, stdout: &[u8]) -> ProbeRunner {
        ProbeRunner {
            identity: RunnerIdentity {
                profile_sha256: "a".repeat(64),
                image_id: format!("sha256:{}", "b".repeat(64)),
                docker_sha256: "c".repeat(64),
                runner_sha256: "d".repeat(64),
                supervisor_sha256: "e".repeat(64),
            },
            outcome: RunOutcome {
                returncode,
                execution_error: execution_error.map(str::to_owned),
                stdout: stdout.to_vec(),
                stderr: b"synthetic diagnostic".to_vec(),
                elapsed_ms: 13,
            },
            called: Mutex::new(false),
        }
    }
    #[test]
    fn probe_exercises_synthetic_input_source_and_fixture_with_same_runner() {
        let runner = runner(0, None, PROBE_OUTPUT);
        let receipt = check_runner(&runner, &profile());
        assert!(receipt.passed);
        assert!(*runner.called.lock().unwrap());
        assert_eq!(receipt.elapsed_ms, 13);
        receipt.ensure_passed().unwrap();
    }
    #[test]
    fn failure_receipts_preserve_exit_status_bounds_and_stderr() {
        for (code, error) in [
            (125, "container_execution_error"),
            (124, "timeout"),
            (137, "container_execution_error"),
            (-1, "stdout_limit"),
        ] {
            let runner = runner(code, Some(error), b"");
            let receipt = check_runner(&runner, &profile());
            assert!(!receipt.passed);
            assert_eq!(receipt.returncode, Some(code));
            assert_eq!(receipt.execution_error.as_deref(), Some(error));
            assert_eq!(receipt.timeout_seconds, 60);
            assert_eq!(receipt.stderr, "synthetic diagnostic");
            assert!(matches!(
                receipt.ensure_passed(),
                Err(Error::ToolExecution(_))
            ));
        }
        assert!(!check_runner(&runner(0, None, b"unexpected"), &profile()).passed);
    }
    #[test]
    fn canonical_profiles_select_their_configured_source_entries() {
        for (source, path) in [
            (
                include_str!("../../examples/toolchains/rust.json"),
                "main.rs",
            ),
            (include_str!("../../examples/toolchains/go.json"), "main.go"),
            (
                include_str!("../../examples/toolchains/typescript.json"),
                "collector.ts",
            ),
            (
                include_str!("../../examples/toolchains/python.json"),
                "collector.py",
            ),
            (
                include_str!("../../examples/toolchains/javascript.json"),
                "collector.mjs",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let file = directory.path().join("toolchain.json");
            std::fs::write(&file, source).unwrap();
            let profile = super::super::profile::ToolchainConfiguration::load(&file)
                .unwrap()
                .profile;
            assert_eq!(probe_source(&profile).unwrap().path, path);
        }
        let mut profile = profile();
        profile.build_argv.clear();
        assert_eq!(probe_source(&profile).unwrap().path, "main.rs");
        profile.build_argv = vec![
            "compiler".into(),
            "/src/one.rs".into(),
            "/src/two.rs".into(),
        ];
        assert!(probe_source(&profile).is_err());
    }
    #[test]
    fn diagnostic_redaction_precedes_utf8_safe_bounding() {
        let token = "provider-token".to_owned();
        let value = bounded_redacted(&format!("{}{}", "é".repeat(2047), token), &[token]);
        assert!(!value.contains("provider"));
        assert!(value.len() <= 4096);
    }
}
