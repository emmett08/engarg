//! One lifecycle for retaining, observing and committing generated source packages.
use super::{
    contracts::{NeedContract, validate_candidate},
    generation::parse_candidate,
    package::{PackageStore, VerifiedPackage},
    profile::ToolchainProfile,
    runner::FixtureRunner,
    verification,
};
use crate::Error;
use eal_process::CancellationToken;
use std::{path::PathBuf, sync::Arc};

pub struct VerificationEvent<'a> {
    pub need_id: &'a str,
    pub attempt: &'a std::path::Path,
    pub passed: bool,
}
pub struct VerifiedBuild<'a> {
    pub store: &'a PackageStore,
    pub key: &'a str,
    pub profile: &'a ToolchainProfile,
    pub runner: Arc<dyn FixtureRunner>,
    pub cancellation: &'a CancellationToken,
}
impl VerifiedBuild<'_> {
    pub async fn commit<A>(
        &self,
        contract: NeedContract,
        response: &[u8],
        audit: A,
    ) -> Result<VerifiedPackage, Error>
    where
        A: Fn(VerificationEvent<'_>) -> Result<(), Error>,
    {
        let candidate = parse_candidate(response)?;
        validate_candidate(&contract, &candidate)?;
        let attempt = self.store.new_attempt(self.key)?;
        self.store.save_candidate(
            &attempt,
            &contract,
            &candidate,
            self.profile,
            self.runner.identity(),
        )?;
        let runner = Arc::clone(&self.runner);
        let task_contract = contract.clone();
        let task_candidate = candidate.clone();
        let task_attempt: PathBuf = attempt.clone();
        let cancellation = self.cancellation.clone();
        let receipt = tokio::task::spawn_blocking(move || {
            verification::verify(
                runner.as_ref(),
                &task_contract,
                &task_candidate,
                &task_attempt,
                &cancellation,
            )
        })
        .await
        .map_err(|_| Error::Provider("tool fixture worker failed".into()))??;
        if self.cancellation.is_cancelled() {
            return Err(Error::Provider("tool implementation cancelled".into()));
        }
        audit(VerificationEvent {
            need_id: &contract.need.id,
            attempt: &attempt,
            passed: receipt.passed,
        })?;
        if !receipt.passed {
            return Err(Error::InvalidOutput(format!(
                "generated collector failed contract fixtures; inspect {}",
                attempt.join("test-results.json").display()
            )));
        }
        self.store.commit_verified(
            self.key,
            &attempt,
            contract,
            candidate,
            self.profile.clone(),
            self.runner.identity().clone(),
            receipt,
        )
    }
}
