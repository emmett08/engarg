//! Reverify retained source against its frozen contract without asking a model.
use super::{
    contracts::{CodeCandidate, NeedContract, contract_digest, digest_json, validate_candidate},
    generation::parse_candidate,
    package::{PackageStore, read_json, validate_source_tree},
    profile::ToolchainProfile,
    runner::{DockerRunner, FixtureRunner, RunnerIdentity},
    verified_build::VerifiedBuild,
};
use crate::Error;
use eal_process::CancellationToken;
use serde::Serialize;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_RETAINED_BYTES: usize = 8 * 1_048_576;

struct RetainedAttempt {
    path: PathBuf,
    contract: NeedContract,
    candidate: CodeCandidate,
    profile: ToolchainProfile,
    identity: RunnerIdentity,
}

impl RetainedAttempt {
    fn load(path: &Path) -> Result<Self, Error> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("replay requires a retained attempt directory"));
        }
        let path = path.canonicalize()?;
        let attempts = path
            .parent()
            .ok_or_else(|| invalid("attempt has no store"))?;
        if attempts.file_name().and_then(|p| p.to_str()) != Some("attempts") {
            return Err(invalid("replay requires an attempt inside a tool store"));
        }
        let store = PackageStore::new(
            attempts
                .parent()
                .ok_or_else(|| invalid("attempt has no store"))?,
        )?;
        let contract: NeedContract = read_json(
            &path.join("contract.json"),
            MAX_RETAINED_BYTES,
            "replay contract",
        )?;
        let candidate: CodeCandidate = read_json(
            &path.join("candidate.json"),
            MAX_RETAINED_BYTES,
            "replay candidate",
        )?;
        let profile = ToolchainProfile::load(&path.join("profile.json"))?;
        let identity: RunnerIdentity =
            read_json(&path.join("runner.json"), 65_536, "replay runner")?;
        let key = PackageStore::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            &identity,
        )?;
        if !path
            .file_name()
            .and_then(|p| p.to_str())
            .is_some_and(|name| name.starts_with(&format!("{key}-")))
        {
            return Err(invalid(
                "retained attempt differs from its original package key",
            ));
        }
        let frozen = store
            .frozen_contract(&key, &identity)?
            .ok_or_else(|| invalid("replay requires the original frozen contract"))?;
        if contract_digest(&frozen)? != contract_digest(&contract)?
            || digest_json(&profile)? != identity.profile_sha256
            || profile.language != contract.language
        {
            return Err(invalid(
                "replay contract or toolchain changed after freezing",
            ));
        }
        validate_candidate(&contract, &candidate)?;
        validate_source_tree(&path, &candidate)?;
        let response = crate::bounded_file::read(
            &path.join("candidate-response.json"),
            MAX_RETAINED_BYTES,
            "replay response",
        )?;
        if digest_json(&parse_candidate(&response)?)? != digest_json(&candidate)? {
            return Err(invalid(
                "retained response differs from the replay candidate",
            ));
        }
        Ok(Self {
            path,
            contract,
            candidate,
            profile,
            identity,
        })
    }
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

#[derive(Debug, Serialize)]
pub struct ReplayReceipt {
    pub schema: &'static str,
    pub original_attempt: PathBuf,
    pub manifest: PathBuf,
    pub contract_sha256: String,
    pub candidate_sha256: String,
    pub original_runner: RunnerIdentity,
    pub runner: RunnerIdentity,
    pub model_calls: u32,
    pub passed: bool,
}

/// An explicit replay admits the current executable while keeping the image,
/// profile, source and frozen fixtures unchanged. The old attempt is preserved.
pub async fn replay_attempt(
    attempt: &Path,
    store: &Path,
    supervisor: &Path,
    cancellation: &CancellationToken,
) -> Result<ReplayReceipt, Error> {
    let retained = RetainedAttempt::load(attempt)?;
    let runner = Arc::new(DockerRunner::new(
        retained.profile.clone(),
        supervisor.to_owned(),
    )?);
    replay_with_runner(retained, store, runner, cancellation).await
}

async fn replay_with_runner(
    retained: RetainedAttempt,
    store: &Path,
    runner: Arc<dyn FixtureRunner>,
    cancellation: &CancellationToken,
) -> Result<ReplayReceipt, Error> {
    if runner.identity().profile_sha256 != retained.identity.profile_sha256
        || runner.identity().image_id != retained.identity.image_id
    {
        return Err(invalid(
            "replay requires the original profile and immutable image identity",
        ));
    }
    let store = PackageStore::new(store)?;
    let key = PackageStore::key_for(
        &retained.contract.need,
        &retained.contract.request_context,
        &retained.contract.language,
        runner.identity(),
    )?;
    let contract = store.freeze_contract(&key, &retained.contract, runner.identity())?;
    if contract_digest(&contract)? != contract_digest(&retained.contract)? {
        return Err(invalid(
            "current package key already freezes a different contract",
        ));
    }
    let origin = serde_json::json!({
        "schema":"engarg.tool-replay-origin/1", "original_attempt":retained.path,
        "original_runner":retained.identity, "runner":runner.identity(),
        "contract_sha256":contract_digest(&contract)?,
        "candidate_sha256":digest_json(&retained.candidate)?, "model_calls":0
    });
    let package = VerifiedBuild {
        store: &store,
        key: &key,
        profile: &retained.profile,
        runner: Arc::clone(&runner),
        cancellation,
    }
    .commit(
        contract,
        &serde_json::to_vec(&retained.candidate)?,
        |event| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(event.attempt.join("replay-origin.json"))?;
            serde_json::to_writer_pretty(&mut file, &origin)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            Ok(())
        },
    )
    .await?;
    Ok(ReplayReceipt {
        schema: "engarg.tool-replay/1",
        original_attempt: retained.path,
        manifest: package.manifest_path().to_owned(),
        contract_sha256: contract_digest(package.contract())?,
        candidate_sha256: digest_json(package.candidate())?,
        original_runner: retained.identity,
        runner: runner.identity().clone(),
        model_calls: 0,
        passed: true,
    })
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
