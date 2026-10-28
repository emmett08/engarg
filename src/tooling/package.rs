//! Private retained source, frozen fixtures and test receipts for generated collectors.
use super::{
    contracts::{
        CodeCandidate, NeedContract, contract_digest, digest_json, validate_candidate,
        validate_contract,
    },
    profile::ToolchainProfile,
    protocol::{REJECTION_CASES, rejected},
    runner::{DOCKER_ENVIRONMENT, RunnerIdentity},
    verification::{TEST_SCHEMA, VerificationReceipt},
};
use crate::{Error, acquisition::AcquisitionPlan, domain::EvidenceNeed};
use eal_collector_contracts::{AuthoringProfile, CollectorDescriptor, CommandTemplate};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const PACKAGE_SCHEMA: &str = "engarg.tool-package/1";
const FROZEN_SCHEMA: &str = "engarg.frozen-tool-contract/1";
const MAX_PACKAGE_JSON: usize = 8 * 1_048_576;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenContract {
    schema: String,
    contract: NeedContract,
    runner: RunnerIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    key: String,
    contract_sha256: String,
    candidate_sha256: String,
    profile_sha256: String,
    source_sha256: String,
    fixtures_sha256: String,
    receipt_sha256: String,
    runner: RunnerIdentity,
}

#[derive(Debug, Clone)]
pub struct PackageStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct VerifiedPackage {
    manifest_path: PathBuf,
    manifest: Manifest,
    contract: NeedContract,
    candidate: CodeCandidate,
    profile: ToolchainProfile,
    receipt: VerificationReceipt,
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn empty_digest() -> &'static str {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}

fn identity_valid(identity: &RunnerIdentity) -> bool {
    [
        &identity.profile_sha256,
        &identity.docker_sha256,
        &identity.runner_sha256,
        &identity.supervisor_sha256,
    ]
    .into_iter()
    .all(|value| hash(value))
        && identity.image_id.strip_prefix("sha256:").is_some_and(hash)
}

fn private_directory(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(invalid(
                "tool package directory must not be a link or special file",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)?,
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    write_new(path, &serde_json::to_vec_pretty(value)?)
}

pub(super) fn read_json<T: DeserializeOwned>(
    path: &Path,
    limit: usize,
    label: &str,
) -> Result<T, Error> {
    let bytes = crate::bounded_file::read(path, limit, label)?;
    let value = eal_collector_contracts::strict_json_exact(
        &bytes,
        eal_collector_contracts::JsonLimits {
            max_bytes: limit,
            max_depth: 128,
            max_nodes: 262_144,
        },
    )
    .map_err(Error::InvalidInput)?;
    Ok(serde_json::from_value(value)?)
}

fn nonce() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

impl PackageStore {
    pub fn new(path: &Path) -> Result<Self, Error> {
        private_directory(path)?;
        let root = path.canonicalize()?;
        for name in ["contracts", "attempts", "index"] {
            private_directory(&root.join(name))?;
        }
        Ok(Self { root })
    }

    pub fn key_for(
        need: &EvidenceNeed,
        request_context: &str,
        language: &str,
        identity: &RunnerIdentity,
    ) -> Result<String, Error> {
        if !identity_valid(identity) {
            return Err(invalid("invalid tool runner identity"));
        }
        super::contracts::validate_language(language)?;
        digest_json(&(need, request_context, language, identity))
    }

    pub fn frozen_contract(
        &self,
        key: &str,
        identity: &RunnerIdentity,
    ) -> Result<Option<NeedContract>, Error> {
        if !hash(key) {
            return Err(invalid("invalid tool package key"));
        }
        let path = self.root.join("contracts").join(format!("{key}.json"));
        if !path.try_exists()? {
            return Ok(None);
        }
        let frozen: FrozenContract = read_json(&path, MAX_PACKAGE_JSON, "frozen tool contract")?;
        validate_contract(&frozen.contract)?;
        if frozen.schema != FROZEN_SCHEMA
            || &frozen.runner != identity
            || Self::key_for(
                &frozen.contract.need,
                &frozen.contract.request_context,
                &frozen.contract.language,
                identity,
            )? != key
        {
            return Err(invalid(
                "frozen tool contract differs from the requested identity",
            ));
        }
        Ok(Some(frozen.contract))
    }

    pub fn freeze_contract(
        &self,
        key: &str,
        contract: &NeedContract,
        identity: &RunnerIdentity,
    ) -> Result<NeedContract, Error> {
        validate_contract(contract)?;
        if Self::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            identity,
        )? != key
        {
            return Err(invalid("tool contract does not belong to the package key"));
        }
        if let Some(frozen) = self.frozen_contract(key, identity)? {
            return Ok(frozen);
        }
        let frozen = FrozenContract {
            schema: FROZEN_SCHEMA.into(),
            contract: contract.clone(),
            runner: identity.clone(),
        };
        let path = self.root.join("contracts").join(format!("{key}.json"));
        match write_json(&path, &frozen) {
            Ok(()) => Ok(contract.clone()),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => self
                .frozen_contract(key, identity)?
                .ok_or_else(|| invalid("frozen contract disappeared")),
            Err(error) => Err(error),
        }
    }

    pub fn new_attempt(&self, key: &str) -> Result<PathBuf, Error> {
        if !hash(key) {
            return Err(invalid("invalid tool package key"));
        }
        for _ in 0..16 {
            let path = self
                .root
                .join("attempts")
                .join(format!("{key}-{}", nonce()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    private_directory(&path)?;
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(invalid("cannot allocate a unique tool attempt directory"))
    }

    fn admit_attempt(&self, attempt: &Path) -> Result<(), Error> {
        if attempt.parent() != Some(self.root.join("attempts").as_path())
            || attempt.canonicalize()? != attempt
            || !fs::symlink_metadata(attempt)?.is_dir()
        {
            return Err(invalid("tool attempt is outside its private store"));
        }
        Ok(())
    }

    pub fn save_candidate(
        &self,
        attempt: &Path,
        contract: &NeedContract,
        candidate: &CodeCandidate,
        profile: &ToolchainProfile,
        identity: &RunnerIdentity,
    ) -> Result<(), Error> {
        self.admit_attempt(attempt)?;
        validate_contract(contract)?;
        validate_candidate(contract, candidate)?;
        profile.validate()?;
        if profile.language != contract.language || digest_json(profile)? != identity.profile_sha256
        {
            return Err(invalid(
                "toolchain differs from the candidate execution identity",
            ));
        }
        write_json(&attempt.join("contract.json"), contract)?;
        write_json(&attempt.join("candidate.json"), candidate)?;
        write_json(&attempt.join("profile.json"), profile)?;
        write_json(&attempt.join("runner.json"), identity)?;
        let source = attempt.join("source");
        private_directory(&source)?;
        for file in &candidate.files {
            let path = source.join(&file.path);
            let parent = path
                .parent()
                .ok_or_else(|| invalid("source file lacks a parent"))?;
            private_directory(parent)?;
            write_new(&path, file.content.as_bytes())?;
        }
        Ok(())
    }

    pub fn lookup(
        &self,
        key: &str,
        identity: &RunnerIdentity,
    ) -> Result<Option<VerifiedPackage>, Error> {
        if !hash(key) {
            return Err(invalid("invalid tool package key"));
        }
        let index = self.root.join("index").join(format!("{key}.json"));
        if !index.try_exists()? {
            return Ok(None);
        }
        let name: String = read_json(&index, 4096, "tool package index")?;
        if !name.starts_with(&format!("{key}-"))
            || name.len() > 256
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(invalid("tool package index has an invalid attempt name"));
        }
        let package = load_verified(&self.root.join("attempts").join(name).join("manifest.json"))?;
        if package.manifest.key != key || package.identity() != identity {
            return Err(invalid(
                "verified tool package differs from the requested execution identity",
            ));
        }
        Ok(Some(package))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_verified(
        &self,
        key: &str,
        attempt: &Path,
        contract: NeedContract,
        candidate: CodeCandidate,
        profile: ToolchainProfile,
        identity: RunnerIdentity,
        receipt: VerificationReceipt,
    ) -> Result<VerifiedPackage, Error> {
        self.admit_attempt(attempt)?;
        if Self::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            &identity,
        )? != key
        {
            return Err(invalid("verified tool package has a different key"));
        }
        validate_receipt(&contract, &candidate, &profile, &identity, &receipt)?;
        // Retained files are checked independently of caller-owned objects.
        let retained_contract: NeedContract = read_json(
            &attempt.join("contract.json"),
            MAX_PACKAGE_JSON,
            "tool contract",
        )?;
        let retained_candidate: CodeCandidate = read_json(
            &attempt.join("candidate.json"),
            MAX_PACKAGE_JSON,
            "tool candidate",
        )?;
        let retained_profile: ToolchainProfile =
            read_json(&attempt.join("profile.json"), 65_536, "tool profile")?;
        let retained_identity: RunnerIdentity =
            read_json(&attempt.join("runner.json"), 65_536, "tool runner")?;
        let retained_receipt: VerificationReceipt = read_json(
            &attempt.join("test-results.json"),
            MAX_PACKAGE_JSON,
            "tool tests",
        )?;
        if digest_json(&retained_contract)? != digest_json(&contract)?
            || digest_json(&retained_candidate)? != digest_json(&candidate)?
            || digest_json(&retained_profile)? != digest_json(&profile)?
            || retained_identity != identity
            || digest_json(&retained_receipt)? != digest_json(&receipt)?
        {
            return Err(invalid("retained tool files changed before registration"));
        }
        validate_source_tree(attempt, &candidate)?;
        let manifest = Manifest {
            schema: PACKAGE_SCHEMA.into(),
            key: key.into(),
            contract_sha256: contract_digest(&contract)?,
            candidate_sha256: digest_json(&candidate)?,
            profile_sha256: digest_json(&profile)?,
            source_sha256: digest_json(&candidate.files)?,
            fixtures_sha256: digest_json(&contract.draft.fixtures)?,
            receipt_sha256: digest_json(&receipt)?,
            runner: identity,
        };
        let manifest_path = attempt.join("manifest.json");
        write_json(&manifest_path, &manifest)?;
        let package = load_verified(&manifest_path)?;
        let name = attempt
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| invalid("invalid tool attempt name"))?;
        let temporary = self
            .root
            .join("index")
            .join(format!("pending-{}.json", nonce()));
        write_json(&temporary, &name)?;
        fs::rename(
            &temporary,
            self.root.join("index").join(format!("{key}.json")),
        )?;
        Ok(package)
    }
}

fn validate_receipt(
    contract: &NeedContract,
    candidate: &CodeCandidate,
    profile: &ToolchainProfile,
    identity: &RunnerIdentity,
    receipt: &VerificationReceipt,
) -> Result<(), Error> {
    validate_contract(contract)?;
    validate_candidate(contract, candidate)?;
    profile.validate()?;
    if !identity_valid(identity)
        || profile.language != contract.language
        || digest_json(profile)? != identity.profile_sha256
        || receipt.schema != TEST_SCHEMA
        || !receipt.passed
        || receipt.language != contract.language
        || &receipt.runner != identity
        || receipt.contract_sha256 != contract_digest(contract)?
        || receipt.candidate_sha256 != digest_json(candidate)?
        || receipt.source_sha256 != digest_json(&candidate.files)?
        || receipt.fixtures_sha256 != digest_json(&contract.draft.fixtures)?
        || receipt.results.len() != contract.draft.fixtures.len() + REJECTION_CASES.len()
        || receipt.results.iter().any(|result| {
            !result.passed || !hash(&result.stdout_sha256) || !hash(&result.stderr_sha256)
        })
    {
        return Err(invalid(
            "tool registration requires a matching complete passing test receipt",
        ));
    }
    for (fixture, result) in contract.draft.fixtures.iter().zip(&receipt.results) {
        let kind = fixture.kind.label();
        if result.name != fixture.name
            || result.kind != kind
            || (kind == "acquisition_error"
                && !rejected(result.returncode, result.stdout_sha256 == empty_digest()))
            || (kind != "acquisition_error" && result.returncode != 0)
        {
            return Err(invalid(
                "tool test receipt omits or changes a frozen fixture",
            ));
        }
    }
    for (probe, result) in REJECTION_CASES
        .iter()
        .zip(&receipt.results[contract.draft.fixtures.len()..])
    {
        if result.name != probe.name()
            || result.kind != "protocol"
            || !rejected(result.returncode, result.stdout_sha256 == empty_digest())
        {
            return Err(invalid("tool test receipt omits a required protocol probe"));
        }
    }
    Ok(())
}

pub(super) fn validate_source_tree(attempt: &Path, candidate: &CodeCandidate) -> Result<(), Error> {
    let root = attempt.join("source");
    let metadata = fs::symlink_metadata(&root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("retained tool source requires a regular directory"));
    }
    let expected: BTreeSet<_> = candidate
        .files
        .iter()
        .map(|file| PathBuf::from(&file.path))
        .collect();
    let mut observed = BTreeSet::new();
    let mut pending = vec![root.clone()];
    let mut entries = 0;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            entries += 1;
            if entries > 512 {
                return Err(invalid("retained tool source tree exceeds its entry limit"));
            }
            let path = entry?.path();
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| invalid("invalid retained source path"))?;
            if relative.components().count() > 64 {
                return Err(invalid("retained source path is too deep"));
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("retained tool source contains a link"));
            }
            if metadata.is_dir() {
                if !expected.iter().any(|file| file.starts_with(relative)) {
                    return Err(invalid(
                        "retained tool source contains an unlisted directory",
                    ));
                }
                pending.push(path);
            } else if metadata.is_file() {
                observed.insert(relative.to_path_buf());
            } else {
                return Err(invalid("retained tool source contains a special file"));
            }
        }
    }
    if observed != expected {
        return Err(invalid("retained tool source file set changed"));
    }
    for file in &candidate.files {
        let bytes =
            crate::bounded_file::read(&root.join(&file.path), 131_072, "retained tool source")?;
        if bytes != file.content.as_bytes() {
            return Err(invalid("retained tool source content changed"));
        }
    }
    Ok(())
}

pub fn load_verified(path: &Path) -> Result<VerifiedPackage, Error> {
    if path.file_name().and_then(|value| value.to_str()) != Some("manifest.json") {
        return Err(invalid("verified tool package requires manifest.json"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("tool manifest lacks a parent"))?
        .canonicalize()?;
    let manifest_path = parent.join("manifest.json");
    let manifest: Manifest = read_json(&manifest_path, 65_536, "tool manifest")?;
    if manifest.schema != PACKAGE_SCHEMA || !hash(&manifest.key) {
        return Err(invalid("unsupported or invalid tool package manifest"));
    }
    let contract: NeedContract = read_json(
        &parent.join("contract.json"),
        MAX_PACKAGE_JSON,
        "tool contract",
    )?;
    let candidate: CodeCandidate = read_json(
        &parent.join("candidate.json"),
        MAX_PACKAGE_JSON,
        "tool candidate",
    )?;
    let profile: ToolchainProfile =
        read_json(&parent.join("profile.json"), 65_536, "tool profile")?;
    let identity: RunnerIdentity = read_json(&parent.join("runner.json"), 65_536, "tool runner")?;
    let receipt: VerificationReceipt = read_json(
        &parent.join("test-results.json"),
        MAX_PACKAGE_JSON,
        "tool tests",
    )?;
    validate_receipt(&contract, &candidate, &profile, &identity, &receipt)?;
    if manifest.key
        != PackageStore::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            &identity,
        )?
        || manifest.contract_sha256 != contract_digest(&contract)?
        || manifest.candidate_sha256 != digest_json(&candidate)?
        || manifest.profile_sha256 != digest_json(&profile)?
        || manifest.source_sha256 != digest_json(&candidate.files)?
        || manifest.fixtures_sha256 != digest_json(&contract.draft.fixtures)?
        || manifest.receipt_sha256 != digest_json(&receipt)?
        || manifest.runner != identity
    {
        return Err(invalid(
            "retained tool package no longer matches its verified manifest",
        ));
    }
    validate_source_tree(&parent, &candidate)?;
    Ok(VerifiedPackage {
        manifest_path,
        manifest,
        contract,
        candidate,
        profile,
        receipt,
    })
}

impl VerifiedPackage {
    pub fn contract(&self) -> &NeedContract {
        &self.contract
    }
    pub fn candidate(&self) -> &CodeCandidate {
        &self.candidate
    }
    pub fn profile(&self) -> &ToolchainProfile {
        &self.profile
    }
    pub fn identity(&self) -> &RunnerIdentity {
        &self.manifest.runner
    }
    pub fn receipt(&self) -> &VerificationReceipt {
        &self.receipt
    }
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    fn collector_id(&self) -> String {
        format!("generated_{}", &self.manifest.key[..32])
    }

    pub fn acquisition_plan(&self) -> AcquisitionPlan {
        AcquisitionPlan {
            input_preparation: None,
            need_id: self.contract.need.id.clone(),
            tool_id: self.collector_id(),
            arguments: self.contract.draft.arguments.clone(),
            unresolved_arguments: self.contract.draft.unresolved_arguments.clone(),
        }
    }

    pub fn descriptor(&self, executable: &Path) -> Result<CollectorDescriptor, Error> {
        // Re-read the retained files immediately before emitting a registration.
        if super::runner::executable_digest(executable)? != self.identity().runner_sha256 {
            return Err(invalid(
                "collector launcher differs from the verified runner executable",
            ));
        }
        let current = load_verified(&self.manifest_path)?;
        if digest_json(&current.manifest)? != digest_json(&self.manifest)? {
            return Err(invalid(
                "verified tool package changed before descriptor registration",
            ));
        }
        let literal = |path: &Path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("tool registration paths require valid UTF-8"))
        };
        let mut environment: BTreeSet<String> = DOCKER_ENVIRONMENT
            .iter()
            .map(|name| (*name).into())
            .collect();
        environment.extend(self.profile.inherit_env.iter().cloned());
        let descriptor = CollectorDescriptor {
            id: self.collector_id(),
            version: "1".into(),
            purpose: self.contract.draft.purpose.clone(),
            parameters: self.contract.draft.parameters.clone(),
            input_schema: self.contract.draft.input_schema.clone(),
            result_schema: self.contract.draft.result_schema.clone(),
            command: CommandTemplate {
                argv: vec![
                    literal(executable)?,
                    "eal".into(),
                    "tool-run".into(),
                    "--package".into(),
                    literal(&self.manifest_path)?,
                ],
                inherit_env: environment.into_iter().collect(),
                timeout_seconds: self.profile.timeout_seconds + 60,
                max_output_bytes: self.profile.output_bytes,
                parallel_safe: true,
            },
            acquisition_predicate: "collected == true".into(),
            constraints: Vec::new(),
        };
        descriptor
            .validate_for(AuthoringProfile::RuntimeExportable)
            .map_err(Error::InvalidInput)?;
        Ok(descriptor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooling::verification::tests::{candidate, contract, identity};

    fn profile() -> ToolchainProfile {
        ToolchainProfile {
            schema: super::super::profile::TOOLCHAIN_SCHEMA.into(),
            language: "python".into(),
            docker: "docker".into(),
            image: "python:3.13-slim".into(),
            build_argv: Vec::new(),
            run_argv: vec!["python3".into(), "/src/collector.py".into()],
            inherit_env: Vec::new(),
            runtime_network: "none".into(),
            timeout_seconds: 30,
            memory_mb: 128,
            output_bytes: 1_048_576,
        }
    }

    #[test]
    fn a_failed_coding_retry_cannot_replace_frozen_expectations() {
        let directory = tempfile::tempdir().unwrap();
        let store = PackageStore::new(directory.path()).unwrap();
        let original = contract();
        let runner = identity();
        let key = PackageStore::key_for(
            &original.need,
            &original.request_context,
            &original.language,
            &runner,
        )
        .unwrap();
        store.freeze_contract(&key, &original, &runner).unwrap();
        let mut weakened = original.clone();
        weakened.draft.fixtures[0].expected_value =
            original.draft.fixtures[1].expected_value.clone();
        weakened.draft.fixtures[1].expected_value =
            original.draft.fixtures[0].expected_value.clone();
        assert_ne!(
            contract_digest(&weakened).unwrap(),
            contract_digest(&original).unwrap()
        );
        let retained = store.freeze_contract(&key, &weakened, &runner).unwrap();
        assert_eq!(
            contract_digest(&retained).unwrap(),
            contract_digest(&original).unwrap()
        );
    }

    #[test]
    fn asserted_success_without_observed_test_cases_cannot_enter_the_reuse_index() {
        let directory = tempfile::tempdir().unwrap();
        let store = PackageStore::new(directory.path()).unwrap();
        let contract = contract();
        let candidate = candidate();
        let profile = profile();
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
        for asserted_pass in [false, true] {
            let attempt = store.new_attempt(&key).unwrap();
            store
                .save_candidate(&attempt, &contract, &candidate, &profile, &identity)
                .unwrap();
            let receipt = VerificationReceipt {
                schema: TEST_SCHEMA.into(),
                language: contract.language.clone(),
                contract_sha256: contract_digest(&contract).unwrap(),
                candidate_sha256: digest_json(&candidate).unwrap(),
                source_sha256: digest_json(&candidate.files).unwrap(),
                fixtures_sha256: digest_json(&contract.draft.fixtures).unwrap(),
                runner: identity.clone(),
                passed: asserted_pass,
                results: Vec::new(),
            };
            assert!(
                store
                    .commit_verified(
                        &key,
                        &attempt,
                        contract.clone(),
                        candidate.clone(),
                        profile.clone(),
                        identity.clone(),
                        receipt
                    )
                    .is_err()
            );
            assert!(store.lookup(&key, &identity).unwrap().is_none());
            assert!(!attempt.join("manifest.json").exists());
        }
    }

    #[test]
    fn prior_receipts_cannot_register_under_the_current_context_policy() {
        let directory = tempfile::tempdir().unwrap();
        let store = PackageStore::new(directory.path()).unwrap();
        let contract = contract();
        let candidate = candidate();
        let profile = profile();
        let mut identity = identity();
        identity.profile_sha256 = digest_json(&profile).unwrap();
        let key = PackageStore::key_for(
            &contract.need,
            &contract.request_context,
            &contract.language,
            &identity,
        )
        .unwrap();
        let attempt = store.new_attempt(&key).unwrap();
        store
            .save_candidate(&attempt, &contract, &candidate, &profile, &identity)
            .unwrap();
        let receipt = super::super::verification::tests::passing_receipt(identity.clone());
        assert!(receipt.passed);
        for schema in ["engarg.tool-tests/1", "engarg.tool-tests/2"] {
            let mut old = receipt.clone();
            old.schema = schema.into();
            std::fs::write(
                attempt.join("test-results.json"),
                serde_json::to_vec(&old).unwrap(),
            )
            .unwrap();
            assert!(
                store
                    .commit_verified(
                        &key,
                        &attempt,
                        contract.clone(),
                        candidate.clone(),
                        profile.clone(),
                        identity.clone(),
                        old
                    )
                    .is_err()
            );
            assert!(store.lookup(&key, &identity).unwrap().is_none());
            assert!(!attempt.join("manifest.json").exists());
        }
        std::fs::write(
            attempt.join("test-results.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        store
            .commit_verified(
                &key,
                &attempt,
                contract,
                candidate,
                profile,
                identity.clone(),
                receipt,
            )
            .unwrap();
        assert!(store.lookup(&key, &identity).unwrap().is_some());
    }
}
