//! Implement reusable operation contracts without importing an argument's conclusions.
//!
//! Frozen synthetic fixtures qualify the declared function on those cases only.
//! They never qualify a scientific method, source authority, or engineering claim.
#[cfg(test)]
use super::verification;
use super::{
    completion::{CompletionBudget, CompletionLimits},
    contracts::{
        CONTRACT_SCHEMA, ContractDraft, NeedContract, digest_json, validate_contract,
        validate_language,
    },
    generation::{code_request, draft_request, parse_draft},
    package::{PackageStore, VerifiedPackage},
    profile::ToolchainProfile,
    runner::{DockerRunner, FixtureRunner},
    verified_build::VerifiedBuild,
};
use crate::{
    Error,
    domain::{EvidenceNeed, Request},
    provider::LanguageModel,
};
use eal_collector_contracts::{
    CollectorDescriptor,
    capability::{
        CapabilityCosts, CapabilityDescriptor, CapabilityLimits, ConfigurationKind,
        ConfigurationPrerequisite, ProviderIdentity, SupportContract, TransportKind,
        canonical_identity, schema_identity,
    },
};
use eal_process::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

pub const CAPABILITY_IMPLEMENTATION_SCHEMA: &str = "engarg.capability-implementation/1";
/// Record metadata version 2 leaves runtime parameter admission to the frozen
/// input schema. The version-1 request/package identity above remains stable.
pub const CAPABILITY_IMPLEMENTATION_RECORD_SCHEMA: &str = "engarg.capability-implementation/2";
pub use super::capability_report::{
    CAPABILITY_TOOLING_SCHEMA, CapabilityGapBinding, CapabilityToolingReport, revalidate_report,
};
const FUNCTION_QUALIFICATION_SCHEMA: &str = "engarg.tested-function-contract/1";
const LIMITATIONS: [&str; 3] = [
    "Synthetic fixtures test only the declared function contract on the retained cases; they are not evidence for an engineering claim.",
    "Source authority, scientific method adequacy, population coverage and historical availability require separate qualification or review.",
    "Runtime parameters, credentials and source records must be supplied independently; generation does not establish their availability.",
];

/// Admission is deliberately narrower than all the reasons a route can be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationGapKind {
    Operation,
    Transformation,
    Configuration,
    Credentials,
    SourceUnavailable,
    HistoricalUnavailable,
    Judgement,
    Qualification,
    Clarification,
}

impl ImplementationGapKind {
    fn admit(self) -> Result<(), Error> {
        if matches!(self, Self::Operation | Self::Transformation) {
            Ok(())
        } else {
            Err(invalid(
                "only operation or transformation gaps admit implementation",
            ))
        }
    }
}

/// A reusable operation specification. There are no claim, need or criterion fields.
///
/// `support_contract` describes the parameterised operation, rather than a consumer's
/// claim. Exact consumer selectors belong in runtime input. Declared limits describe
/// operation behaviour; executable resource bounds come from the operator profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityImplementationRequest {
    pub support_contract: SupportContract,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub input_schema: Value,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_exact")]
    pub output_schema: Value,
    pub operation_purpose: String,
    #[serde(deserialize_with = "eal_collector_contracts::deserialize_value_map_exact")]
    pub declared_limits: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct CapabilityImplementationConfig {
    pub language: String,
    pub profile: ToolchainProfile,
    pub store: PathBuf,
    pub max_calls: u32,
    pub max_input_bytes: usize,
    pub max_response_bytes: usize,
    pub supervisor: PathBuf,
    pub engarg_executable: PathBuf,
}

/// A retained test qualification, kept separate from measurement semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionContractQualification {
    pub schema: String,
    pub contract_sha256: String,
    pub fixtures_sha256: String,
    pub receipt: PathBuf,
    pub receipt_sha256: String,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementedCapability {
    pub schema: String,
    pub capability_sha256: String,
    pub collector: CollectorDescriptor,
    pub capability: CapabilityDescriptor,
    pub package_manifest: PathBuf,
    pub manifest_sha256: String,
    pub qualification: FunctionContractQualification,
    pub reused: bool,
    pub model_calls: u32,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

impl CapabilityImplementationRequest {
    /// Stable across consumer claims, need identifiers and criterion wording.
    pub fn identity(&self) -> Result<String, Error> {
        canonical_identity(&json!({
            "schema": CAPABILITY_IMPLEMENTATION_SCHEMA,
            "request": self
        }))
        .map_err(invalid)
    }

    fn parameters(&self) -> Result<Vec<String>, Error> {
        self.input_schema["properties"]
            .as_object()
            .map(|properties| properties.keys().cloned().collect())
            .ok_or_else(|| invalid("implementation input schema requires object properties"))
    }

    fn need(&self) -> Result<EvidenceNeed, Error> {
        Ok(EvidenceNeed {
            id: format!("capability_{}", &self.identity()?[..32]),
            target_id: "capability".into(),
            question: self.operation_purpose.clone(),
            method: "Implement the supplied parameterised operation contract".into(),
            measurement: "Return the declared output for supplied operation parameters".into(),
            comparator: "Frozen synthetic input/output examples".into(),
            acceptance_criterion:
                "Conform to the declared function contract on retained test cases".into(),
            if_failed: "Do not register the implementation".into(),
            enables_claim: "capability".into(),
            priority: 1,
            cost: "Bounded by the separately admitted implementation budget".into(),
        })
    }

    fn context(&self) -> Result<String, Error> {
        // Canonical serialisation also keeps cache identity independent of object order.
        let value = eal_collector_contracts::strict_json_exact(
            &serde_json::to_vec(self)?,
            eal_collector_contracts::JsonLimits {
                max_bytes: super::contracts::MAX_JSON_BYTES,
                ..Default::default()
            },
        )
        .map_err(invalid)?;
        Ok(canonical_json(value).to_string())
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.support_contract.validate().map_err(invalid)?;
        if self.operation_purpose.trim().is_empty()
            || self.operation_purpose.len() > 8192
            || self.operation_purpose.chars().any(char::is_control)
            || self.declared_limits.len() > 128
            || self
                .declared_limits
                .keys()
                .any(|key| key.trim().is_empty() || key.len() > 1024)
            || serde_json::to_vec(self)?.len() > super::contracts::MAX_JSON_BYTES
        {
            return Err(invalid(
                "implementation requires a bounded operation purpose and limits",
            ));
        }
        // Validate the fixed interface before either an external runner or model is admitted.
        let parameters = self.parameters()?;
        let probe = NeedContract {
            schema: CONTRACT_SCHEMA.into(),
            language: "interface".into(),
            need: self.need()?,
            request_context: self.context()?,
            toolchain_description: "Interface admission; no implementation has been generated"
                .into(),
            draft: ContractDraft {
                purpose: self.operation_purpose.clone(),
                parameters: parameters.clone(),
                input_schema: self.input_schema.clone(),
                result_schema: self.output_schema.clone(),
                arguments: Vec::new(),
                unresolved_arguments: parameters,
                fixtures: Vec::new(),
            },
        };
        // Schema/interface validation is shared with frozen collector contracts. Fixtures
        // are validated after their dedicated drafting call, never invented at admission.
        validate_interface(&probe)?;
        if let Some(expected) = &self.support_contract.expected_output_schema_sha256
            && schema_identity(&self.output_schema).map_err(invalid)? != *expected
        {
            return Err(invalid(
                "implementation output schema differs from its support contract",
            ));
        }
        Ok(())
    }
}

fn canonical_json(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical_json(value)))
                    .collect(),
            )
        }
        Value::Array(array) => Value::Array(array.into_iter().map(canonical_json).collect()),
        other => other,
    }
}

fn validate_interface(contract: &NeedContract) -> Result<(), Error> {
    // Use the authoritative descriptor validator directly for the interface, including
    // bounded local schemas, parameter correspondence and result transport predicate.
    use eal_collector_contracts::{AuthoringProfile, CommandTemplate};
    let descriptor = CollectorDescriptor {
        id: "capability_interface".into(),
        version: "1".into(),
        purpose: contract.draft.purpose.clone(),
        parameters: contract.draft.parameters.clone(),
        input_schema: contract.draft.input_schema.clone(),
        result_schema: contract.draft.result_schema.clone(),
        command: CommandTemplate {
            argv: vec!["engarg".into()],
            inherit_env: Vec::new(),
            timeout_seconds: 1,
            max_output_bytes: 1024,
            parallel_safe: false,
        },
        acquisition_predicate: "collected == true".into(),
        constraints: Vec::new(),
    };
    descriptor
        .validate_for(AuthoringProfile::RuntimeExportable)
        .map_err(invalid)?;
    // The legacy collector contract additionally requires the successful-acquisition bit.
    if contract.draft.result_schema["type"] != "object"
        || contract.draft.result_schema["properties"]["collected"]["type"] != "boolean"
        || !contract.draft.result_schema["required"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value == "collected"))
    {
        return Err(invalid(
            "implementation output schema requires a boolean collected property",
        ));
    }
    Ok(())
}

impl CapabilityImplementationConfig {
    fn validate(&self) -> Result<(), Error> {
        validate_language(&self.language)?;
        self.profile.validate()?;
        if self.language != self.profile.language {
            return Err(invalid(
                "implementation language differs from the operator profile",
            ));
        }
        if self.max_calls > 256 || self.max_input_bytes == 0 || self.max_response_bytes == 0 {
            return Err(invalid("invalid capability implementation budget"));
        }
        let current = std::env::current_exe()?.canonicalize()?;
        if self.engarg_executable.canonicalize()? != current {
            return Err(invalid(
                "capability registration requires the running EngArg executable",
            ));
        }
        Ok(())
    }
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Generate and observe contract tests, or reuse a rehashed verified package without
/// making model calls. Rejected gap classes fail before Docker or API admission.
pub async fn implement_capability<M: LanguageModel>(
    request: &CapabilityImplementationRequest,
    gap_kind: ImplementationGapKind,
    config: CapabilityImplementationConfig,
    model: &M,
) -> Result<ImplementedCapability, Error> {
    gap_kind.admit()?;
    request.validate()?;
    config.validate()?;
    let runner = Arc::new(DockerRunner::new(
        config.profile.clone(),
        config.supervisor.clone(),
    )?);
    implement_with_runner(request, gap_kind, config, model, runner).await
}

async fn implement_with_runner<M: LanguageModel>(
    request: &CapabilityImplementationRequest,
    gap_kind: ImplementationGapKind,
    config: CapabilityImplementationConfig,
    model: &M,
    runner: Arc<dyn FixtureRunner>,
) -> Result<ImplementedCapability, Error> {
    gap_kind.admit()?;
    request.validate()?;
    config.validate()?;
    let cancellation = CancellationToken::default();
    let _guard = CancelOnDrop(cancellation.clone());
    let store = PackageStore::new(&config.store)?;
    let identity = runner.identity().clone();
    let need = request.need()?;
    let context = request.context()?;
    let key = PackageStore::key_for(&need, &context, &config.language, &identity)?;
    if let Some(package) = store.lookup(&key, &identity)? {
        ensure_fixed_contract(request, package.contract())?;
        return registration(
            request,
            &config,
            &package,
            true,
            0,
            CAPABILITY_IMPLEMENTATION_RECORD_SCHEMA,
        );
    }
    let frozen = store.frozen_contract(&key, &identity)?;
    let required_calls = if frozen.is_some() { 1 } else { 2 };
    if config.max_calls < required_calls {
        return Err(Error::Budget(format!(
            "capability implementation requires {required_calls} model calls; configured budget is {}",
            config.max_calls
        )));
    }
    let calls = CompletionBudget::new(CompletionLimits {
        max_calls: config.max_calls,
        max_input_bytes: config.max_input_bytes,
        max_response_bytes: config.max_response_bytes,
    })?;
    let contract = if let Some(contract) = frozen {
        ensure_fixed_contract(request, &contract)?;
        contract
    } else {
        let description = config.profile.description()?;
        let operation_request = Request {
            claim: request.operation_purpose.clone(),
            context: context.clone(),
            evidence: Vec::new(),
        };
        let mut draft = draft_request(&need, &operation_request, &config.language, &description);
        draft.system.push_str(concat!(
            " This is a reusable operation contract, independent of all consumer claims. ",
            "Copy operation_purpose, input_schema and output_schema exactly from request.context into purpose, input_schema_json and result_schema_json. ",
            "Do not select claim acceptance thresholds or consumer identifiers. All input parameters remain unresolved; arguments must be empty. ",
            "Respect every declared operation limit. Draft synthetic function-contract fixtures, not scientific qualification or evidence."
        ));
        let response = calls.complete(model, draft, |_| Ok(())).await?;
        let contract = NeedContract {
            schema: CONTRACT_SCHEMA.into(),
            language: config.language.clone(),
            need,
            request_context: context,
            toolchain_description: description,
            draft: parse_draft(response.content.as_bytes())?,
        };
        ensure_fixed_contract(request, &contract)?;
        validate_contract(&contract)?;
        store.freeze_contract(&key, &contract, &identity)?
    };
    let response = calls
        .complete(model, code_request(&contract), |_| Ok(()))
        .await?;
    let package = VerifiedBuild {
        store: &store,
        key: &key,
        profile: &config.profile,
        runner,
        cancellation: &cancellation,
    }
    .commit(contract, response.content.as_bytes(), |_| Ok(()))
    .await?;
    registration(
        request,
        &config,
        &package,
        false,
        calls.calls()?,
        CAPABILITY_IMPLEMENTATION_RECORD_SCHEMA,
    )
}

fn ensure_fixed_contract(
    request: &CapabilityImplementationRequest,
    contract: &NeedContract,
) -> Result<(), Error> {
    let parameters = request.parameters()?;
    let mut unresolved = contract.draft.unresolved_arguments.clone();
    unresolved.sort();
    let mut actual_parameters = contract.draft.parameters.clone();
    actual_parameters.sort();
    if contract.draft.purpose != request.operation_purpose
        || digest_json(&contract.draft.input_schema)? != digest_json(&request.input_schema)?
        || digest_json(&contract.draft.result_schema)? != digest_json(&request.output_schema)?
        || !contract.draft.arguments.is_empty()
        || unresolved != parameters
        || actual_parameters != parameters
        || contract.request_context != request.context()?
        || digest_json(&contract.need)? != digest_json(&request.need()?)?
    {
        return Err(invalid(
            "generated contract changed the fixed parameterised capability interface",
        ));
    }
    Ok(())
}

fn file_digest(path: &std::path::Path) -> Result<String, Error> {
    let bytes = crate::bounded_file::read(path, 8 * 1_048_576, "capability retained record")?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn registration(
    request: &CapabilityImplementationRequest,
    config: &CapabilityImplementationConfig,
    package: &VerifiedPackage,
    reused: bool,
    calls: u32,
    record_schema: &str,
) -> Result<ImplementedCapability, Error> {
    // descriptor() reopens the package and rehashes retained source and observed tests.
    let collector = package.descriptor(&config.engarg_executable)?;
    let manifest = package.manifest_path().to_path_buf();
    let receipt = manifest
        .parent()
        .ok_or_else(|| invalid("package manifest has no directory"))?
        .join("test-results.json");
    let mut configuration_requirements = Vec::new();
    // Reconstruct saved version-1 records exactly. New records do not duplicate
    // schema-required input fields as separate operator confirmations.
    if record_schema == CAPABILITY_IMPLEMENTATION_SCHEMA {
        let required_parameters = collector.required_parameters().map_err(invalid)?;
        configuration_requirements = collector
            .parameters
            .iter()
            .map(|parameter| ConfigurationPrerequisite {
                id: parameter.clone(),
                kind: ConfigurationKind::OperatorValue,
                description: format!(
                    "Supply runtime parameter {parameter} according to the fixed input schema"
                ),
                required: required_parameters.contains(parameter),
            })
            .collect();
    }
    configuration_requirements.extend(config.profile.inherit_env.iter().map(|name| {
        ConfigurationPrerequisite {
            id: format!("credential_{name}"),
            kind: ConfigurationKind::CredentialReference,
            description: format!(
                "Supply the operator-admitted environment reference {name} at execution time"
            ),
            required: true,
        }
    }));
    let capability = CapabilityDescriptor {
        id: collector.id.clone(),
        version: collector.version.clone(),
        description: request.operation_purpose.clone(),
        provider: ProviderIdentity {
            id: collector.id.clone(),
            version: collector.version.clone(),
            transport: TransportKind::Command,
            connection: None,
            tool: None,
            authentication_scope: if config.profile.inherit_env.is_empty() {
                "no_inherited_credentials".into()
            } else {
                "operator_supplied_environment_references".into()
            },
            configuration_sha256: canonical_identity(
                &json!({"command":collector.command,"runner":package.identity()}),
            )
            .map_err(invalid)?,
        },
        input_schema: request.input_schema.clone(),
        input_schema_sha256: schema_identity(&request.input_schema).map_err(invalid)?,
        output_schema: request.output_schema.clone(),
        output_schema_sha256: schema_identity(&request.output_schema).map_err(invalid)?,
        // Function tests cannot supply operator-qualified measurement semantics.
        semantics: None,
        configuration_requirements,
        limits: CapabilityLimits {
            timeout_seconds: collector.command.timeout_seconds,
            output_bytes: collector.command.max_output_bytes as u64,
            concurrency: 1,
            requests_per_minute: None,
        },
        costs: CapabilityCosts::unknown(
            "Retained fixture timings do not estimate live source execution costs",
        ),
    };
    capability.validate().map_err(invalid)?;
    Ok(ImplementedCapability {
        schema: record_schema.into(),
        capability_sha256: request.identity()?,
        collector,
        capability,
        package_manifest: manifest.clone(),
        manifest_sha256: file_digest(&manifest)?,
        qualification: FunctionContractQualification {
            schema: FUNCTION_QUALIFICATION_SCHEMA.into(),
            contract_sha256: package.receipt().contract_sha256.clone(),
            fixtures_sha256: package.receipt().fixtures_sha256.clone(),
            receipt_sha256: file_digest(&receipt)?,
            receipt,
            limitations: LIMITATIONS.into_iter().map(str::to_owned).collect(),
        },
        reused,
        model_calls: calls,
    })
}

/// Reopen every retained record and source file before accepting saved registration
/// metadata. Interface qualification cannot be widened by editing a report.
pub fn revalidate_implemented_capability(
    record: &ImplementedCapability,
    executable: &std::path::Path,
) -> Result<ImplementedCapability, Error> {
    if !matches!(
        record.schema.as_str(),
        CAPABILITY_IMPLEMENTATION_SCHEMA | CAPABILITY_IMPLEMENTATION_RECORD_SCHEMA
    ) || file_digest(&record.package_manifest)? != record.manifest_sha256
    {
        return Err(invalid("saved capability package identity changed"));
    }
    let package = super::package::load_verified(&record.package_manifest)?;
    let request: CapabilityImplementationRequest =
        serde_json::from_str(&package.contract().request_context)?;
    request.validate()?;
    ensure_fixed_contract(&request, package.contract())?;
    let config = CapabilityImplementationConfig {
        language: package.contract().language.clone(),
        profile: package.profile().clone(),
        store: PathBuf::new(),
        max_calls: 0,
        max_input_bytes: 1,
        max_response_bytes: 1,
        supervisor: PathBuf::new(),
        engarg_executable: executable.to_owned(),
    };
    let expected = registration(
        &request,
        &config,
        &package,
        record.reused,
        record.model_calls,
        &record.schema,
    )?;
    if digest_json(&expected)? != digest_json(record)? {
        return Err(invalid(
            "saved capability registration differs from its verified package",
        ));
    }
    Ok(expected)
}

#[cfg(test)]
#[path = "capability_tests.rs"]
mod tests;
