//! Authoring adapter to EAL's single bounded isolated execution lifecycle.
use super::{contracts::SourceFile, profile::ToolchainProfile};
use crate::Error;
use eal_process::CancellationToken;
pub use eal_tool_runtime::runner::{DOCKER_ENVIRONMENT, ExecutionMode, RunOutcome, RunnerIdentity};
use std::path::{Path, PathBuf};

pub trait FixtureRunner: Send + Sync {
    fn identity(&self) -> &RunnerIdentity;
    fn execute(
        &self,
        package_root: &Path,
        source_files: &[SourceFile],
        fixture_files: &[SourceFile],
        request_bytes: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error>;
}
pub struct DockerRunner(eal_tool_runtime::runner::DockerRunner);
impl DockerRunner {
    pub fn new(profile: ToolchainProfile, supervisor: PathBuf) -> Result<Self, Error> {
        Ok(Self(eal_tool_runtime::runner::DockerRunner::new(
            profile, supervisor,
        )?))
    }
    pub fn profile(&self) -> &ToolchainProfile {
        self.0.profile()
    }
    pub fn identity(&self) -> &RunnerIdentity {
        self.0.identity()
    }
}
impl FixtureRunner for DockerRunner {
    fn identity(&self) -> &RunnerIdentity {
        self.0.identity()
    }
    fn execute(
        &self,
        package_root: &Path,
        source_files: &[SourceFile],
        fixture_files: &[SourceFile],
        request_bytes: &[u8],
        mode: ExecutionMode,
        cancellation: &CancellationToken,
    ) -> Result<RunOutcome, Error> {
        use eal_tool_runtime::runner::FixtureRunner;
        Ok(self.0.execute(
            package_root,
            source_files,
            fixture_files,
            request_bytes,
            mode,
            cancellation,
        )?)
    }
}
pub fn executable_digest(path: &Path) -> Result<String, Error> {
    Ok(eal_tool_runtime::runner::executable_digest(path)?)
}
