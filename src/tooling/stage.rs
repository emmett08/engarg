//! Generate collectors without changing argument authorship or registering untested code.
use super::{
    contracts::{
        CONTRACT_SCHEMA, CodeCandidate, NeedContract, digest_json, validate_candidate,
        validate_contract, validate_language,
    },
    generation::{code_request, draft_request, parse_candidate, parse_draft},
    package::{PackageStore, VerifiedPackage, load_verified},
    profile::ToolchainProfile,
    runner::{DockerRunner, RunnerIdentity},
    saved::{self, SavedInput},
    verification,
};
use crate::{
    Error,
    acquisition::AcquisitionPlan,
    collector_catalogue::CollectorCatalogue,
    domain::EvidenceNeed,
    provider::{Completion, CompletionRequest, LanguageModel},
};
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const TOOLING_SCHEMA: &str = "engarg.tooling/1";
#[derive(Debug, Clone)]
pub struct ToolingConfig {
    pub language: String,
    pub profile: ToolchainProfile,
    pub store: PathBuf,
    pub output_dir: PathBuf,
    pub max_calls: u32,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub supervisor: PathBuf,
    pub engarg_executable: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPackageRecord {
    pub need_id: String,
    pub manifest: PathBuf,
    pub manifest_sha256: String,
    pub reused: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolingReport {
    pub schema: String,
    pub language: String,
    pub scope_sha256: String,
    pub original_plans: Vec<AcquisitionPlan>,
    pub original_catalogue: CollectorCatalogue,
    pub catalogue: CollectorCatalogue,
    pub overrides: Vec<AcquisitionPlan>,
    pub packages: Vec<ToolPackageRecord>,
    pub model_calls: u32,
    pub audit_path: PathBuf,
}
pub struct Registration {
    pub catalogue: CollectorCatalogue,
    pub overrides: Vec<AcquisitionPlan>,
    pub original_plans: Vec<AcquisitionPlan>,
}
pub struct ToolingSession {
    config: ToolingConfig,
    runner: Arc<DockerRunner>,
    store: PackageStore,
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn empty_catalogue() -> CollectorCatalogue {
    CollectorCatalogue(eal_collector_contracts::Catalogue {
        schema: eal_collector_contracts::CATALOGUE_SCHEMA.into(),
        acquisition_contract: eal_collector_contracts::ACQUISITION_CONTRACT.into(),
        collectors: Vec::new(),
    })
}
fn scope(input: &SavedInput) -> Result<String, Error> {
    digest_json(&(
        &input.request,
        &input.argument,
        &input.plans,
        &input.catalogue,
    ))
}
fn missing_needs(input: &SavedInput) -> Vec<EvidenceNeed> {
    let plans: BTreeMap<_, _> = input
        .plans
        .iter()
        .map(|plan| (plan.need_id.as_str(), plan))
        .collect();
    input
        .argument
        .evidence_plan
        .iter()
        .filter(|need| {
            plans
                .get(need.id.as_str())
                .is_none_or(|plan| plan.tool_id == "custom")
        })
        .cloned()
        .collect()
}
fn manifest_digest(path: &Path) -> Result<String, Error> {
    let bytes = crate::bounded_file::read(path, 8_388_608, "tool package manifest")?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn same<T: Serialize>(left: &T, right: &T) -> Result<bool, Error> {
    Ok(digest_json(left)? == digest_json(right)?)
}
fn combine(
    original: &CollectorCatalogue,
    generated: &CollectorCatalogue,
) -> Result<CollectorCatalogue, Error> {
    let mut combined = original.clone();
    for descriptor in &generated.collectors {
        if let Some(existing) = combined
            .collectors
            .iter()
            .find(|item| item.id == descriptor.id)
        {
            if !same(existing, descriptor)? {
                return Err(invalid("conflicting generated collector identifier"));
            }
        } else {
            combined.collectors.push(descriptor.clone());
        }
    }
    combined.validate()?;
    Ok(combined)
}
fn write_new<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
impl ToolingSession {
    pub fn admit(mut config: ToolingConfig) -> Result<Self, Error> {
        validate_language(&config.language)?;
        config.profile.validate()?;
        if config.profile.language != config.language {
            return Err(invalid(
                "tool language differs from the operator toolchain profile",
            ));
        }
        if !(1..=256).contains(&config.max_calls)
            || config.max_input_bytes == 0
            || config.max_response_bytes == 0
        {
            return Err(invalid("invalid tooling call or byte budget"));
        }
        let current = std::env::current_exe()?.canonicalize()?;
        config.engarg_executable = config.engarg_executable.canonicalize()?;
        if current != config.engarg_executable {
            return Err(invalid(
                "tool registration executable must be the running EngArg executable",
            ));
        }
        let runner = Arc::new(DockerRunner::new(
            config.profile.clone(),
            config.supervisor.clone(),
        )?);
        let store = PackageStore::new(&config.store)?;
        let parent = config
            .output_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&config.output_dir).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                invalid("tool output directory must be new")
            } else {
                Error::Io(error)
            }
        })?;
        config.output_dir = config.output_dir.canonicalize()?;
        write_new(
            &config.output_dir.join("tooling-admission.json"),
            &json!({
                "schema":"engarg.tooling-admission/1", "language":config.language, "runner":runner.identity()
            }),
        )?;
        Ok(Self {
            config,
            runner,
            store,
        })
    }
    fn audit(&self, record: &Value) -> Result<(), Error> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(self.config.output_dir.join("tooling.audit.jsonl"))?;
        serde_json::to_writer(&mut file, record)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        Ok(())
    }
    async fn complete<M: LanguageModel>(
        &self,
        model: &M,
        phase: &str,
        need: &str,
        request: CompletionRequest,
        calls: &mut u32,
    ) -> Result<Completion, Error> {
        if *calls >= self.config.max_calls {
            return Err(Error::Budget("tooling model call budget exhausted".into()));
        }
        if serde_json::to_vec(&request)?.len() > self.config.max_input_bytes {
            return Err(Error::Budget(
                "tooling model request exceeds its input byte budget".into(),
            ));
        }
        self.audit(&json!({"event":"request","phase":phase,"need_id":need,"request":request}))?;
        *calls += 1;
        let started = Instant::now();
        let completion = match model.complete(request).await {
            Ok(completion) => completion,
            Err(error) => {
                self.audit(&json!({"event":"provider_failure","phase":phase,"need_id":need,"elapsed_ms":started.elapsed().as_millis()}))?;
                return Err(error);
            }
        };
        if completion.content.len() > self.config.max_response_bytes {
            self.audit(&json!({"event":"response_limit","phase":phase,"need_id":need,"response_bytes":completion.content.len()}))?;
            return Err(Error::Budget(
                "tooling model response exceeds its byte budget".into(),
            ));
        }
        self.audit(&json!({"event":"completion","phase":phase,"need_id":need,"completion":completion,"elapsed_ms":started.elapsed().as_millis()}))?;
        Ok(completion)
    }
    pub async fn implement<M: LanguageModel>(
        &self,
        model: &M,
        report: &Value,
    ) -> Result<ToolingReport, Error> {
        let cancellation = CancellationToken::default();
        let _guard = CancelOnDrop(cancellation.clone());
        let result = self.implement_inner(model, report, &cancellation).await;
        self.audit(&json!({"event":"finished","passed":result.is_ok()}))?;
        result
    }

    async fn implement_inner<M: LanguageModel>(
        &self,
        model: &M,
        report: &Value,
        cancellation: &CancellationToken,
    ) -> Result<ToolingReport, Error> {
        let input = saved::parse(report)?;
        write_new(
            &self.config.output_dir.join("argument.analysis.json"),
            report,
        )?;
        let request_context = serde_json::to_string(&input.request)?;
        let identity = self.runner.identity().clone();
        enum Work {
            Reuse(Box<VerifiedPackage>),
            Frozen(String, Box<NeedContract>),
            Fresh(String, Box<EvidenceNeed>),
        }
        let mut work = Vec::new();
        let mut required_calls = 0u32;
        for need in missing_needs(&input) {
            let key =
                PackageStore::key_for(&need, &request_context, &self.config.language, &identity)?;
            if let Some(package) = self.store.lookup(&key, &identity)? {
                work.push(Work::Reuse(Box::new(package)));
            } else if let Some(contract) = self.store.frozen_contract(&key, &identity)? {
                required_calls += 1;
                work.push(Work::Frozen(key, Box::new(contract)));
            } else {
                required_calls += 2;
                work.push(Work::Fresh(key, Box::new(need)));
            }
        }
        if required_calls > self.config.max_calls {
            return Err(Error::Budget(format!(
                "tooling requires {required_calls} model calls; configured budget is {}",
                self.config.max_calls
            )));
        }
        self.audit(&json!({"event":"budget","required_calls":required_calls,"max_calls":self.config.max_calls}))?;
        let description = self.config.profile.description()?;
        let mut generated = empty_catalogue();
        let mut overrides = Vec::new();
        let mut packages = Vec::new();
        let mut calls = 0;
        for item in work {
            if cancellation.is_cancelled() {
                return Err(Error::Provider("tool implementation cancelled".into()));
            }
            let (package, reused) = match item {
                Work::Reuse(package) => (*package, true),
                Work::Frozen(key, contract) => (
                    self.code_and_verify(
                        model,
                        &key,
                        *contract,
                        &identity,
                        &mut calls,
                        cancellation,
                    )
                    .await?,
                    false,
                ),
                Work::Fresh(key, need) => {
                    let need = *need;
                    let response = self
                        .complete(
                            model,
                            "contract",
                            &need.id,
                            draft_request(
                                &need,
                                &input.request,
                                &self.config.language,
                                &description,
                            ),
                            &mut calls,
                        )
                        .await?;
                    let draft = parse_draft(response.content.as_bytes())?;
                    let contract = NeedContract {
                        schema: CONTRACT_SCHEMA.into(),
                        language: self.config.language.clone(),
                        need,
                        request_context: request_context.clone(),
                        toolchain_description: description.clone(),
                        draft,
                    };
                    validate_contract(&contract)?;
                    let contract = self.store.freeze_contract(&key, &contract, &identity)?;
                    (
                        self.code_and_verify(
                            model,
                            &key,
                            contract,
                            &identity,
                            &mut calls,
                            cancellation,
                        )
                        .await?,
                        false,
                    )
                }
            };
            let descriptor = package.descriptor(&self.config.engarg_executable)?;
            let plan = package.acquisition_plan();
            packages.push(ToolPackageRecord {
                need_id: plan.need_id.clone(),
                manifest: package.manifest_path().to_path_buf(),
                manifest_sha256: manifest_digest(package.manifest_path())?,
                reused,
            });
            generated.collectors.push(descriptor);
            overrides.push(plan);
        }
        generated.validate()?;
        let combined = combine(&input.catalogue, &generated)?;
        let mut effective = input.plans.clone();
        for replacement in &overrides {
            effective.retain(|plan| plan.need_id != replacement.need_id);
            effective.push(replacement.clone());
        }
        let errors = crate::acquisition::validate_with_catalogue(
            &input.argument,
            &effective,
            Some(&combined),
        );
        if !errors.is_empty() {
            return Err(invalid(&format!(
                "generated acquisition bindings are invalid: {}",
                errors.join("; ")
            )));
        }
        let metadata = ToolingReport {
            schema: TOOLING_SCHEMA.into(),
            language: self.config.language.clone(),
            scope_sha256: scope(&input)?,
            original_plans: input.plans,
            original_catalogue: input.catalogue,
            catalogue: generated,
            overrides,
            packages,
            model_calls: calls,
            audit_path: self.config.output_dir.join("tooling.audit.jsonl"),
        };
        write_new(&self.config.output_dir.join("collectors.json"), &combined)?;
        write_new(&self.config.output_dir.join("tooling.json"), &metadata)?;
        Ok(metadata)
    }
    async fn code_and_verify<M: LanguageModel>(
        &self,
        model: &M,
        key: &str,
        contract: NeedContract,
        identity: &RunnerIdentity,
        calls: &mut u32,
        cancellation: &CancellationToken,
    ) -> Result<VerifiedPackage, Error> {
        let attempt = self.store.new_attempt(key)?;
        let response = self
            .complete(
                model,
                "code",
                &contract.need.id,
                code_request(&contract),
                calls,
            )
            .await?;
        let candidate: CodeCandidate = parse_candidate(response.content.as_bytes())?;
        validate_candidate(&contract, &candidate)?;
        self.store.save_candidate(
            &attempt,
            &contract,
            &candidate,
            &self.config.profile,
            identity,
        )?;
        let runner = Arc::clone(&self.runner);
        let task_contract = contract.clone();
        let task_candidate = candidate.clone();
        let task_attempt = attempt.clone();
        let task_cancel = cancellation.clone();
        let receipt = tokio::task::spawn_blocking(move || {
            verification::verify(
                runner.as_ref(),
                &task_contract,
                &task_candidate,
                &task_attempt,
                &task_cancel,
            )
        })
        .await
        .map_err(|_| Error::Provider("tool fixture worker failed".into()))??;
        if cancellation.is_cancelled() {
            return Err(Error::Provider("tool implementation cancelled".into()));
        }
        self.audit(&json!({"event":"verification","need_id":contract.need.id,"passed":receipt.passed,"attempt":attempt}))?;
        if !receipt.passed {
            return Err(Error::InvalidOutput(format!(
                "generated collector failed contract fixtures; inspect {}",
                attempt.join("test-results.json").display()
            )));
        }
        self.store.commit_verified(
            key,
            &attempt,
            contract,
            candidate,
            self.config.profile.clone(),
            identity.clone(),
            receipt,
        )
    }
}
impl ToolingReport {
    pub fn with_tooling(&self, original: &Value) -> Result<Value, Error> {
        let mut combined = original.clone();
        combined
            .as_object_mut()
            .ok_or_else(|| invalid("saved analysis must be an object"))?
            .insert("tooling".into(), serde_json::to_value(self)?);
        registration(&combined)?;
        Ok(combined)
    }
}
pub fn registration(report: &Value) -> Result<Option<Registration>, Error> {
    let Some(value) = report.get("tooling").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let metadata: ToolingReport = serde_json::from_value(value.clone())?;
    if metadata.schema != TOOLING_SCHEMA {
        return Err(invalid("unsupported tooling metadata schema"));
    }
    validate_language(&metadata.language)?;
    let input = saved::parse(report)?;
    if metadata.scope_sha256 != scope(&input)?
        || !same(&metadata.original_plans, &input.plans)?
        || !same(&metadata.original_catalogue, &input.catalogue)?
    {
        return Err(invalid(
            "tooling metadata does not match the original analysis scope",
        ));
    }
    let expected: BTreeSet<_> = missing_needs(&input)
        .into_iter()
        .map(|need| need.id)
        .collect();
    let mut seen = BTreeSet::new();
    let mut derived = empty_catalogue();
    let mut overrides = Vec::new();
    let executable = std::env::current_exe()?.canonicalize()?;
    let executable_sha256 = super::runner::executable_digest(&executable)?;
    let request_context = serde_json::to_string(&input.request)?;
    for record in &metadata.packages {
        if !seen.insert(record.need_id.clone()) || !expected.contains(&record.need_id) {
            return Err(invalid(
                "tooling repeats or adds an unexpected evidence need",
            ));
        }
        if manifest_digest(&record.manifest)? != record.manifest_sha256 {
            return Err(invalid("verified tool package manifest changed"));
        }
        let package = load_verified(&record.manifest)?;
        let need = input
            .argument
            .evidence_plan
            .iter()
            .find(|need| need.id == record.need_id)
            .ok_or_else(|| invalid("generated tool names an unknown evidence need"))?;
        if !same(&package.contract().need, need)?
            || package.contract().request_context != request_context
            || package.contract().language != metadata.language
            || package.identity().runner_sha256 != executable_sha256
        {
            return Err(invalid(
                "verified tool package differs from its evidence need, scope, language or launcher",
            ));
        }
        derived.collectors.push(package.descriptor(&executable)?);
        overrides.push(package.acquisition_plan());
    }
    if seen != expected
        || !same(&derived, &metadata.catalogue)?
        || !same(&overrides, &metadata.overrides)?
    {
        return Err(invalid(
            "tooling registration is incomplete or differs from verified packages",
        ));
    }
    let catalogue = combine(&input.catalogue, &derived)?;
    let mut effective = input.plans.clone();
    for replacement in &overrides {
        effective.retain(|plan| plan.need_id != replacement.need_id);
        effective.push(replacement.clone());
    }
    let errors =
        crate::acquisition::validate_with_catalogue(&input.argument, &effective, Some(&catalogue));
    if !errors.is_empty() {
        return Err(invalid("verified tooling acquisition bindings are invalid"));
    }
    Ok(Some(Registration {
        catalogue,
        overrides,
        original_plans: input.plans,
    }))
}
