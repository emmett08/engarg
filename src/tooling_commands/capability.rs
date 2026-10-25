//! CLI bridge for capability gaps; argument requirements and routes remain intact.
use super::{ToolOptions, termination_signal};
use engarg::{
    Error,
    provider::{Completion, CompletionRequest, LanguageModel},
    route_planning::{GapKind, ROUTE_PLAN_SCHEMA, RoutePlan},
    tooling::{
        capability::{
            CAPABILITY_TOOLING_SCHEMA, CapabilityGapBinding as GapBinding,
            CapabilityImplementationConfig, CapabilityToolingReport, ImplementationGapKind,
            ImplementedCapability, implement_capability,
        },
        completion::{CompletionBudget, CompletionLimits},
        contracts::digest_json,
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn append_audit(path: &Path, record: &Value) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(&mut file, record)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

fn retain<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

struct AuditModel<'a, M> {
    inner: &'a M,
    path: PathBuf,
    budget: CompletionBudget,
}
impl<M: LanguageModel> LanguageModel for AuditModel<'_, M> {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion, Error> {
        self.budget
            .complete(self.inner, request, |event| {
                let mut record = event.record();
                record["event"] = json!(format!(
                    "capability_{}",
                    record["event"].as_str().unwrap_or("unknown")
                ));
                append_audit(&self.path, &record)
            })
            .await
    }
}

struct AuditGuard {
    path: PathBuf,
    finished: bool,
}
impl Drop for AuditGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = append_audit(
                &self.path,
                &json!({
                    "event":"capability_cancelled","passed":false,
                    "reason":"capability_stage_dropped_before_completion_record"
                }),
            );
        }
    }
}

fn routes(original: &Value) -> Result<RoutePlan, Error> {
    engarg::report::validate_replanning(original, &std::env::current_exe()?)?;
    let engarg::report::EvidencePlanningReport::Graph(planning) =
        engarg::report::classify_evidence_planning(original)?
    else {
        return Err(invalid(
            "capability tooling requires typed evidence planning routes",
        ));
    };
    let routes = planning.routes;
    if routes.schema != ROUTE_PLAN_SCHEMA {
        return Err(invalid("unsupported capability route plan schema"));
    }
    routes.execution.validate().map_err(invalid)?;
    routes.configuration.validate()?;
    Ok(routes)
}

fn kind(gap: GapKind) -> Option<ImplementationGapKind> {
    match gap {
        GapKind::OperationImplementation => Some(ImplementationGapKind::Operation),
        GapKind::TransformationImplementation => Some(ImplementationGapKind::Transformation),
        _ => None,
    }
}

async fn implement<M: LanguageModel>(
    model: &AuditModel<'_, M>,
    original: &Value,
    routes: &RoutePlan,
    config: &CapabilityImplementationConfig,
) -> Result<CapabilityToolingReport, Error> {
    let mut generated = BTreeMap::<String, ImplementedCapability>::new();
    let mut bindings = Vec::new();
    let mut skipped = 0;
    let mut deferred = 0;
    for gap in &routes.gaps {
        let Some(kind) = kind(gap.kind) else {
            skipped += 1;
            continue;
        };
        let Some(request) = &gap.implementation else {
            deferred += 1;
            continue;
        };
        if routes.discovery.incomplete
            || routes
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::DiscoveryIncomplete)
        {
            deferred += 1;
            continue;
        }
        let identity = request.identity()?;
        if !generated.contains_key(&identity) {
            let mut selected = config.clone();
            selected.max_calls = config.max_calls.saturating_sub(model.budget.calls()?);
            append_audit(
                &model.path,
                &json!({"event":"capability_admission","capability_sha256":identity,"gap_kind":gap.kind,"remaining_calls":selected.max_calls}),
            )?;
            let result = implement_capability(request, kind, selected, model).await?;
            retain(
                &model
                    .path
                    .with_file_name(format!("capability-{identity}.json")),
                &result,
            )?;
            generated.insert(identity.clone(), result);
        }
        let implemented = generated
            .get(&identity)
            .ok_or_else(|| invalid("implemented capability disappeared"))?;
        bindings.push(GapBinding {
            acquisition_id: gap.acquisition_id.clone(),
            requirement_id: gap.requirement_id.clone(),
            capability_sha256: identity,
            collector_id: implemented.collector.id.clone(),
        });
    }
    let note = if generated.is_empty() {
        "No implementation was generated or reused. Original configuration, qualification, source availability and other route gaps remain unresolved."
    } else {
        "Retained source passed frozen synthetic function-contract tests. Measurement-method adequacy, source authority and availability require separate qualification; original requirements and route gaps remain unchanged."
    };
    let report = CapabilityToolingReport {
        schema: CAPABILITY_TOOLING_SCHEMA.into(),
        original_planning_sha256: digest_json(&original["evidence_planning"])?,
        language: config.language.clone(),
        capabilities: generated.into_values().collect(),
        bindings,
        model_calls: model.budget.calls()?,
        skipped_non_implementation_gaps: skipped,
        deferred_implementation_gaps: deferred,
        audit_path: model.path.clone(),
        note: note.into(),
    };
    if !report.capabilities.is_empty() {
        let present = std::env::vars_os()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>();
        let mut providers = Vec::new();
        let mut operations = BTreeMap::new();
        for package in &report.capabilities {
            let template = engarg::tooling::generated_registration::native_provider_template(
                package,
                &config.engarg_executable,
                &config.supervisor,
                &present,
            )?;
            let prepared = super::native_provider::prepare(
                &routes.configuration.collector,
                &config.supervisor,
                &template,
                &model.path.with_file_name(format!(
                    "provider-{}.request.json",
                    package.capability_sha256
                )),
            )
            .await?;
            operations.insert(
                package.collector.id.clone(),
                engarg::tooling::generated_registration::operation_contract(package)?,
            );
            providers.push(prepared);
        }
        retain(
            &model.path.with_file_name("generated-native-providers.json"),
            &json!({"schema":"eal.evidence-config/1","connections":[],"native_providers":providers,"local_catalogues":[],"approved_catalogues":[]}),
        )?;
        retain(
            &model.path.with_file_name("operation-contracts.json"),
            &operations,
        )?;
    }
    Ok(report)
}

/// The caller may have already admitted a legacy ToolingSession solely for profile,
/// Docker and private-directory preflight. This bridge creates no legacy overrides.
pub async fn run_capability_fallback<M: LanguageModel>(
    model: &M,
    original: &Value,
    options: &ToolOptions,
    max_input_bytes: usize,
    max_response_bytes: usize,
) -> Result<Value, Error> {
    if original
        .get("tooling")
        .is_some_and(|value| !value.is_null())
    {
        return Err(invalid("saved analysis already contains tooling metadata"));
    }
    let routes = routes(original)?;
    let config = options
        .capability_config(true, max_input_bytes, max_response_bytes)?
        .ok_or_else(|| invalid("capability implementation was not enabled"))?;
    let directory = options.output_dir()?;
    // Normally the existing ToolOptions admission created this private directory.
    // Standalone graph implementation may create it here without admitting a model.
    if !directory.exists() {
        if let Some(parent) = directory.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
    }
    if !fs::symlink_metadata(directory)?.is_dir() {
        return Err(invalid(
            "capability output directory must be a real directory",
        ));
    }
    let directory = directory.canonicalize()?;
    retain(&directory.join("argument.analysis.json"), original)?;
    let audit_path = directory.join("tooling.audit.jsonl");
    let mut guard = AuditGuard {
        path: audit_path.clone(),
        finished: false,
    };
    let model = AuditModel {
        inner: model,
        path: audit_path.clone(),
        budget: CompletionBudget::new(CompletionLimits {
            max_calls: config.max_calls,
            max_input_bytes: config.max_input_bytes,
            max_response_bytes: config.max_response_bytes,
        })?,
    };
    append_audit(
        &audit_path,
        &json!({"event":"capability_stage_started","max_calls":config.max_calls,"gap_count":routes.gaps.len()}),
    )?;
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(options.deadline()), implement(&model, original, &routes, &config)) => {
            result.map_err(|_| Error::Budget("capability implementation deadline exceeded".into()))
        }
        interrupted = tokio::signal::ctrl_c() => {
            interrupted?; Err(Error::Budget("capability implementation interrupted".into()))
        }
        terminated = termination_signal() => {
            terminated?; Err(Error::Budget("capability implementation terminated".into()))
        }
    }.and_then(|result| result);
    append_audit(
        &audit_path,
        &json!({"event":"capability_stage_finished","passed":result.is_ok(),"model_calls":model.budget.calls()?}),
    )?;
    guard.finished = true;
    let report = result?;
    retain(&directory.join("capability-tooling.json"), &report)?;
    let mut combined = original.clone();
    let object = combined
        .as_object_mut()
        .ok_or_else(|| invalid("saved analysis must be an object"))?;
    object.insert("tooling".into(), serde_json::to_value(report)?);
    Ok(combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoModelCalls;
    impl LanguageModel for NoModelCalls {
        async fn complete(&self, _request: CompletionRequest) -> Result<Completion, Error> {
            panic!("unresolved non-implementation or discovery gaps must not call a model")
        }
    }

    #[tokio::test]
    async fn graph_audit_omits_oversized_completion_before_retaining_it() {
        struct Oversized;
        impl LanguageModel for Oversized {
            async fn complete(&self, _: CompletionRequest) -> Result<Completion, Error> {
                Ok(Completion {
                    content: "oversized-private-code".repeat(100),
                    model: "fixture".into(),
                    input_tokens: None,
                    output_tokens: None,
                    request_id: None,
                    service_tier: None,
                    reasoning_effort: None,
                })
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audit.jsonl");
        let model = AuditModel {
            inner: &Oversized,
            path: path.clone(),
            budget: CompletionBudget::new(CompletionLimits {
                max_calls: 1,
                max_input_bytes: 1024,
                max_response_bytes: 8,
            })
            .unwrap(),
        };
        let request = CompletionRequest {
            role: engarg::provider::Role::Single,
            system: "bounded".into(),
            user: "operation".into(),
            schema: json!({}),
        };
        assert!(matches!(
            model.complete(request).await,
            Err(Error::Budget(_))
        ));
        let raw = fs::read_to_string(path).unwrap();
        assert!(!raw.contains("oversized-private-code"));
        let records = raw
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records[1]["event"], "capability_response_limit");
        assert_eq!(records[1]["response_content_omitted"], true);
        assert_eq!(model.budget.calls().unwrap(), 1);
    }

    fn original(incomplete: bool) -> Value {
        json!({
            "argument":{"sentinel":"Original acceptance wording stays exact"},
            "evidence_planning":{
                "schema":"engarg.support-planning/1","elaboration":{"schema":"engarg.support-elaboration/1","requirements":[],"acquisitions":[],"derivations":[],"evaluations":[],"revisions":[]},
                "routes":{
                    "schema":ROUTE_PLAN_SCHEMA,
                    "execution":{"schema":"eal.evidence-execution-plan/1","acquisitions":[],"derivations":[],"evaluations":[],"scheduling":{"maximum_concurrent_sources":4,"per_provider":{}}},
                    "routes":[],"projections":[],"gaps":[{
                        "acquisition_id":"read_source","requirement_id":null,"kind":"configuration",
                        "description":"Supply operator credentials","implementation":null
                    }],
                    "discovery":{"admitted_capabilities":[],"incomplete":incomplete,"gaps":[],"metrics":{}},
                    "configuration":{
                        "schema":"engarg.capability-planning-config/1","collector":"not-executed",
                        "connections":"not-read","store":"not-read",
                        "ranking":{"dimensions":["execution_requests"]},
                        "scheduling":{"maximum_concurrent_sources":4,"per_provider":{}},"bindings":[]
                    }
                }
            }
        })
    }

    fn options(directory: &Path) -> ToolOptions {
        let path = directory.join("profile.json");
        let profile = json!({
            "schema":"engarg.toolchain/1","language":"rust","docker":"/must-not-run/docker",
            "image":"operator-image","build_argv":[],"run_argv":["/build/collector"],
            "inherit_env":[],"runtime_network":"none","timeout_seconds":30,
            "memory_mb":128,"output_bytes":1_048_576
        });
        fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
        ToolOptions {
            tool_language: Some("rust".into()),
            toolchain: Some(path),
            tool_store: Some(directory.join("must-not-create-store")),
            tool_output_dir: Some(directory.join("output")),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn non_implementation_gaps_retain_original_without_admitting_docker_or_model() {
        let directory = tempfile::tempdir().unwrap();
        let original = original(false);
        let options = options(directory.path());
        let report =
            run_capability_fallback(&NoModelCalls, &original, &options, 1_048_576, 1_048_576)
                .await
                .unwrap();
        assert_eq!(report["evidence_planning"], original["evidence_planning"]);
        assert_eq!(report["argument"], original["argument"]);
        assert_eq!(report["tooling"]["capabilities"], json!([]));
        assert_eq!(report["tooling"]["model_calls"], 0);
        assert_eq!(report["tooling"]["skipped_non_implementation_gaps"], 1);
        assert!(
            report["tooling"]["note"]
                .as_str()
                .unwrap()
                .starts_with("No implementation")
        );
        assert!(!options.tool_store.as_ref().unwrap().exists());
        let retained: Value = serde_json::from_slice(
            &fs::read(options.output_dir().unwrap().join("argument.analysis.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(retained, original);
        assert!(
            options
                .output_dir()
                .unwrap()
                .join("capability-tooling.json")
                .is_file()
        );
        engarg::tooling::capability::revalidate_report(&report, &std::env::current_exe().unwrap())
            .unwrap();
        let mut tampered = report.clone();
        tampered["evidence_planning"]["elaboration"]["sentinel"] = json!("weakened");
        assert!(
            engarg::tooling::capability::revalidate_report(
                &tampered,
                &std::env::current_exe().unwrap()
            )
            .is_err()
        );
        let mut tampered = report.clone();
        tampered["tooling"]["bindings"] = json!([{
            "acquisition_id":"read_source","requirement_id":null,
            "capability_sha256":"a".repeat(64),"collector_id":"unverified"
        }]);
        assert!(
            engarg::tooling::capability::revalidate_report(
                &tampered,
                &std::env::current_exe().unwrap()
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn standalone_graph_implement_skips_legacy_docker_admission_for_configuration_gaps() {
        let directory = tempfile::tempdir().unwrap();
        let original = original(false);
        let options = options(directory.path());
        let input = directory.path().join("analysis.json");
        fs::write(&input, serde_json::to_vec(&original).unwrap()).unwrap();
        let args = vec![
            "engarg".to_owned(),
            "eal".into(),
            "implement".into(),
            "--input".into(),
            input.to_string_lossy().into_owned(),
            "--model".into(),
            "not-called".into(),
            "--endpoint".into(),
            "http://127.0.0.1:9".into(),
            "--tool-language".into(),
            "rust".into(),
            "--toolchain".into(),
            options
                .toolchain
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "--tool-store".into(),
            options
                .tool_store
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "--tool-output-dir".into(),
            options.output_dir().unwrap().to_string_lossy().into_owned(),
        ];
        let cli = <crate::Cli as clap::Parser>::try_parse_from(args).unwrap();
        let crate::Command::Eal { command } = cli.command else {
            panic!("expected EAL command");
        };
        let crate::eal_commands::EalCommand::Implement(args) = *command else {
            panic!("expected implement command");
        };
        super::super::implement(args).await.unwrap();
        let report: Value = serde_json::from_slice(
            &fs::read(
                options
                    .output_dir()
                    .unwrap()
                    .join("analysis.with-tools.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(report["evidence_planning"], original["evidence_planning"]);
        assert_eq!(report["tooling"]["model_calls"], 0);
        assert_eq!(report["tooling"]["capabilities"], json!([]));
        assert!(!options.tool_store.as_ref().unwrap().exists());
    }
    #[test]
    fn only_genuine_implementation_gap_kinds_are_eligible() {
        assert_eq!(
            kind(GapKind::OperationImplementation),
            Some(ImplementationGapKind::Operation)
        );
        assert_eq!(
            kind(GapKind::TransformationImplementation),
            Some(ImplementationGapKind::Transformation)
        );
        for gap in [
            GapKind::Configuration,
            GapKind::SourceUnavailable,
            GapKind::HistoricalUnavailable,
            GapKind::Qualification,
            GapKind::CapabilityMismatch,
            GapKind::Clarification,
            GapKind::Judgement,
            GapKind::DiscoveryIncomplete,
        ] {
            assert_eq!(kind(gap), None);
        }
    }
}
