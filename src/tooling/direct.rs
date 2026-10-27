//! Construct missing acquisition operations and publish their direct EAL bindings.
use super::{
    capability::{
        CapabilityImplementationConfig, ImplementationGapKind, ImplementedCapability,
        implement_capability,
    },
    completion::{CompletionBudget, CompletionLimits},
    contracts::digest_json,
};
use crate::{
    Error,
    direct_planning::DirectPlanner,
    engine::{RouteResolver, RunReport},
    provider::{Completion, CompletionRequest, LanguageModel},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const DIRECT_TOOLING_SCHEMA: &str = "engarg.direct-tooling/1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectOperationImplementation {
    pub acquisition_id: String,
    pub operation_sha256: String,
    pub implementation: ImplementedCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredOperation {
    pub acquisition_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectToolingReport {
    pub schema: String,
    pub input_report_sha256: String,
    pub retained_input: PathBuf,
    pub tools: PathBuf,
    pub tools_sha256: String,
    pub operations: Vec<DirectOperationImplementation>,
    pub deferred: Vec<DeferredOperation>,
    pub model_calls: u32,
    pub audit_path: PathBuf,
}

pub struct DirectImplementationConfig {
    pub implementation: CapabilityImplementationConfig,
    pub output_dir: PathBuf,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn retain(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
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

fn audit(path: &Path, value: &Value) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

struct AuditedModel<'a, M> {
    inner: &'a M,
    path: PathBuf,
    budget: CompletionBudget,
}
impl<M: LanguageModel> LanguageModel for AuditedModel<'_, M> {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.budget
            .complete(self.inner, request, |event| {
                audit(&self.path, &event.record())
            })
            .await
    }
}
struct AttemptGuard {
    path: PathBuf,
    finished: bool,
}
impl Drop for AttemptGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = audit(
                &self.path,
                &json!({"event":"implementation_cancelled","passed":false}),
            );
        }
    }
}

/// Derive the reusable, unit-aware function contract from authored operation semantics.
pub fn operation_request(
    acquisition: &crate::support_requirements::AcquisitionBlueprint,
    elaboration: &crate::support_requirements::SupportElaboration,
) -> Result<super::capability::CapabilityImplementationRequest, Error> {
    crate::route_planning::implementation::request(acquisition, elaboration)
}

/// Resolve the supplied authority before admitting generation. Every missing
/// operation requires its own authored parameterised specification; an unknown
/// source, credential or judgement is never replaced by invented observations.
pub async fn implement<M: LanguageModel>(
    model: &M,
    original: &Value,
    tools: &Path,
    config: DirectImplementationConfig,
) -> Result<RunReport, Error> {
    let mut report = crate::report::read_direct(original)?;
    let elaboration = report
        .support_elaboration
        .clone()
        .ok_or_else(|| invalid("tool implementation requires support elaboration"))?;
    let planner = DirectPlanner::from_tools(tools)?;
    let plan = planner.resolve(&report.argument, &elaboration).await?;
    let mut requests = Vec::new();
    let mut deferred = Vec::new();
    for id in &plan.unbound_operations {
        let acquisition = elaboration
            .acquisitions
            .iter()
            .find(|a| &a.id == id)
            .ok_or_else(|| invalid("unbound operation is absent from its authored elaboration"))?;
        if acquisition.operation_spec.is_none() {
            deferred.push(DeferredOperation {
                acquisition_id: id.clone(),
                reason: "The operation needs an explicit parameterised specification before implementation".into(),
            });
        } else {
            requests.push((id.clone(), operation_request(acquisition, &elaboration)?));
        }
    }
    let directory = std::path::absolute(&config.output_dir)?;
    if directory.symlink_metadata().is_ok() {
        return Err(invalid(
            "tool implementation requires a new output directory",
        ));
    }
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let directory = directory.canonicalize()?;
    let retained_input = directory.join("argument.analysis.json");
    retain(&retained_input, original)?;
    let audit_path = directory.join("tooling.audit.jsonl");
    let mut guard = AttemptGuard {
        path: audit_path.clone(),
        finished: false,
    };
    let model = AuditedModel {
        inner: model,
        path: audit_path.clone(),
        budget: CompletionBudget::new(CompletionLimits {
            max_calls: config.implementation.max_calls,
            max_input_bytes: config.implementation.max_input_bytes,
            max_response_bytes: config.implementation.max_response_bytes,
        })?,
    };
    audit(
        &audit_path,
        &json!({"event":"implementation_started","operations":requests.len(),"deferred":deferred.len()}),
    )?;
    let result = async {
        let mut generated = BTreeMap::<String, ImplementedCapability>::new();
        let mut operations = Vec::new();
        let mut exports = Vec::new();
        for (id, request) in requests {
            let identity = request.identity()?;
            if !generated.contains_key(&identity) {
                let mut selected = config.implementation.clone();
                selected.max_calls = selected.max_calls.saturating_sub(model.budget.calls()?);
                let implementation = implement_capability(
                    &request,
                    ImplementationGapKind::Operation,
                    selected,
                    &model,
                )
                .await?;
                generated.insert(identity.clone(), implementation);
            }
            let implementation = generated
                .get(&identity)
                .expect("generated operation retained")
                .clone();
            let exported = super::export::export_verified(
                &implementation.package_manifest,
                &directory.join("exports").join(&id),
                Some(&id),
            )?;
            exports.push((id.clone(), exported.tools));
            operations.push(DirectOperationImplementation {
                acquisition_id: id,
                operation_sha256: identity,
                implementation,
            });
        }
        let bound_tools =
            crate::package::merge_tool_exports(tools, &exports, &directory.join("bindings"))?;
        let completed = DirectPlanner::from_tools(&bound_tools)?
            .resolve(&report.argument, &elaboration)
            .await?;
        let metadata = DirectToolingReport {
            schema: DIRECT_TOOLING_SCHEMA.into(),
            input_report_sha256: digest_json(original)?,
            retained_input,
            tools_sha256: crate::bounded_file::sha256(
                &bound_tools,
                1_048_576,
                "generated tools manifest",
            )?
            .sha256,
            tools: bound_tools,
            operations,
            deferred,
            model_calls: model.budget.calls()?,
            audit_path: audit_path.clone(),
        };
        retain(&directory.join("implementation.json"), &metadata)?;
        report.tooling = Some(metadata);
        report.direct_bindings = Some(completed);
        crate::report::read_direct(&serde_json::to_value(&report)?)?;
        Ok::<_, Error>(report)
    }
    .await;
    audit(
        &audit_path,
        &json!({"event":"implementation_finished","passed":result.is_ok(),"model_calls":model.budget.calls()?}),
    )?;
    guard.finished = true;
    result
}

/// Revalidate current implementation receipts when auditing retained discovery
/// history. This deliberately reads typed fields without recursively validating
/// the report's own provenance chain.
pub fn revalidate_report(raw: &Value, executable: &Path) -> Result<(), Error> {
    let Some(value) = raw.get("tooling").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let metadata: DirectToolingReport = serde_json::from_value(value.clone())?;
    if metadata.schema != DIRECT_TOOLING_SCHEMA
        || metadata.operations.len() > 256
        || metadata.deferred.len() > 256
    {
        return Err(invalid(
            "unsupported or oversized direct implementation metadata",
        ));
    }
    let retained = crate::bounded_file::read(
        &metadata.retained_input,
        16 * 1_048_576,
        "implementation input",
    )?;
    let original = eal_collector_contracts::strict_json_exact(
        &retained,
        eal_collector_contracts::JsonLimits {
            max_bytes: 16 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    if digest_json(&original)? != metadata.input_report_sha256
        || original["argument"] != raw["argument"]
        || original["request"] != raw["request"]
    {
        return Err(invalid(
            "implementation input differs from its retained source or current argument",
        ));
    }
    let original: RunReport = serde_json::from_value(original)?;
    let support = original
        .support_elaboration
        .as_ref()
        .or_else(|| {
            original
                .evidence_planning
                .as_ref()
                .map(|planning| &planning.elaboration)
        })
        .ok_or_else(|| invalid("implementation input lacks support elaboration"))?;
    if crate::bounded_file::sha256(&metadata.tools, 1_048_576, "implementation tools")?.sha256
        != metadata.tools_sha256
    {
        return Err(invalid(
            "implemented tool bindings changed after publication",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut packages = BTreeMap::<String, &ImplementedCapability>::new();
    for operation in &metadata.operations {
        if !ids.insert(operation.acquisition_id.as_str()) {
            return Err(invalid("implementation metadata repeats an acquisition"));
        }
        let acquisition = support
            .acquisitions
            .iter()
            .find(|acquisition| acquisition.id == operation.acquisition_id)
            .ok_or_else(|| invalid("implemented operation is absent from the authored contract"))?;
        let identity = operation_request(acquisition, support)?.identity()?;
        let implementation = &operation.implementation;
        if operation.operation_sha256 != identity
            || implementation.capability_sha256 != identity
            || (implementation.reused && implementation.model_calls != 0)
            || (!implementation.reused && !(1..=256).contains(&implementation.model_calls))
        {
            return Err(invalid(
                "implementation changed its authored operation identity or call accounting",
            ));
        }
        if let Some(previous) = packages.insert(identity, implementation) {
            if digest_json(previous)? != digest_json(implementation)? {
                return Err(invalid(
                    "shared operation has conflicting implementation receipts",
                ));
            }
        } else {
            super::capability::revalidate_implemented_capability(implementation, executable)?;
        }
    }
    for deferred in &metadata.deferred {
        if !ids.insert(deferred.acquisition_id.as_str())
            || deferred.reason.is_empty()
            || !support.acquisitions.iter().any(|operation| {
                operation.id == deferred.acquisition_id && operation.operation_spec.is_none()
            })
        {
            return Err(invalid(
                "deferred operation metadata differs from its missing semantic specification",
            ));
        }
    }
    let calls = packages
        .values()
        .try_fold(0u32, |total, implementation| {
            total.checked_add(implementation.model_calls)
        })
        .ok_or_else(|| invalid("implementation call count overflow"))?;
    if calls != metadata.model_calls {
        return Err(invalid(
            "implementation call accounting differs from its verified packages",
        ));
    }
    Ok(())
}
