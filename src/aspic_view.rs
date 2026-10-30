//! Offline ASPIC snapshots through the paired EAL application's pure commands.
//!
//! EAL owns assessment, translation, solving and checked export. Retained
//! observations and operator-binding digests remain supplied provenance; this
//! adapter neither acquires evidence nor authenticates a current tools registry.
mod verification;
pub use verification::attach_verification;

use crate::Error;
use eal_core::ExecutionLimits;
use eal_process::{CommandRequest, ExecutionError, StreamLimits};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

const MAX_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

/// The operator selects the paired native core and one deadline for both steps.
pub struct Config {
    pub executable: PathBuf,
    pub methods: String,
    pub timeout: Duration,
    pub limits: ExecutionLimits,
}

#[derive(Serialize)]
struct LimitDocument<'a> {
    limits: &'a ExecutionLimits,
}

async fn command(
    config: &Config,
    workspace: &std::path::Path,
    arguments: &[String],
    remaining: Duration,
) -> Result<Value, Error> {
    let selected = if config.executable.is_absolute() || config.executable.components().count() > 1
    {
        config.executable.canonicalize()?
    } else {
        config.executable.clone()
    };
    let executable = selected
        .to_str()
        .filter(|path| !path.is_empty())
        .ok_or_else(|| invalid("eal-core must be a nonempty UTF-8 executable path"))?;
    let mut argv = vec![
        executable.into(),
        "--workspace".into(),
        workspace
            .to_str()
            .ok_or_else(|| invalid("temporary workspace must be UTF-8"))?
            .into(),
        "--limits".into(),
        "limits.toml".into(),
        "--methods".into(),
        config.methods.clone(),
    ];
    argv.extend_from_slice(arguments);
    // Only PATH is needed to resolve the operator's default executable name.
    // No provider credentials, collector configuration or ambient EAL options
    // are forwarded to these pure native operations.
    let environment: BTreeMap<_, _> = std::env::var("PATH")
        .ok()
        .map(|value| ("PATH".into(), value))
        .into_iter()
        .collect();
    let outcome = eal_adapters::command::run_with_stream_limits(
        CommandRequest {
            argv,
            environment,
            cwd: workspace.to_path_buf(),
            input: Vec::new(),
            timeout: remaining,
            max_output_bytes: MAX_OUTPUT + MAX_STDERR,
        },
        StreamLimits {
            max_stdout_bytes: MAX_OUTPUT,
            max_stderr_bytes: MAX_STDERR,
        },
    )
    .await?;
    if let Some(error) = outcome.execution_error {
        return Err(match error {
            ExecutionError::Timeout => Error::Budget("ASPIC export deadline exhausted; native process terminated".into()),
            ExecutionError::OutputLimit => Error::Budget("ASPIC native output exceeded its byte limit".into()),
            _ => Error::ToolExecution("bounded eal-core execution failed; check the paired native executable and supervisor".into()),
        });
    }
    let parsed = eal_collector_contracts::strict_json_exact(
        &outcome.stdout,
        eal_collector_contracts::JsonLimits {
            max_bytes: MAX_OUTPUT,
            max_depth: 128,
            ..Default::default()
        },
    );
    if outcome.returncode != 0 {
        if let Ok(value) = parsed
            && value["complete"] == false
        {
            return Err(Error::Budget(format!(
                "incomplete ASPIC snapshot: {}",
                value["reasons"]
            )));
        }
        return Err(invalid(format!(
            "eal-core {} failed: {}",
            arguments[0],
            String::from_utf8_lossy(&outcome.stderr).trim()
        )));
    }
    parsed.map_err(|_| Error::ToolExecution("eal-core returned invalid or oversized JSON".into()))
}

/// Compile a self-contained EAL/4 programme using a retained run's exact
/// observations, context, assessment time and method profile. Imported modules
/// require the EAL application with its original source workspace/resolver.
pub async fn export(
    source: &str,
    execution: &Value,
    claim: Option<&str>,
    config: &Config,
) -> Result<Value, Error> {
    let started = std::time::Instant::now();
    config.limits.validate().map_err(invalid)?;
    if config.timeout.is_zero() || config.timeout > Duration::from_secs(3600) {
        return Err(invalid(
            "ASPIC export timeout must be positive and at most 3600 seconds",
        ));
    }
    if !matches!(
        config.methods.as_str(),
        "builtin" | "engineering" | "aspic" | "all"
    ) {
        return Err(invalid("unknown EAL method profile"));
    }
    let run = execution.get("result")
        .filter(|_| execution["is_error"] == false)
        .filter(|run| run["schema"] == "EAL/execution/1")
        .ok_or_else(|| invalid("export-aspic requires a successful eal-host run execution envelope, including collection and assessment; an assessment alone is insufficient"))?;
    let collection = &run["collection"];
    let recorded = &run["assessment"];
    if !collection["records"].is_object()
        || !collection["context"].is_object()
        || recorded["valid"] != true
        || recorded["complete"] != true
    {
        return Err(invalid(
            "export-aspic requires retained records, context and a valid, complete EAL assessment",
        ));
    }
    let source_digest = run["source_digest"]
        .as_str()
        .filter(|digest| !digest.is_empty())
        .ok_or_else(|| invalid("execution source_digest is missing"))?;
    if collection["source_digest"] != source_digest
        || recorded["source_digest"] != source_digest
        || collection["collection_id"]
            .as_str()
            .is_none_or(str::is_empty)
        || recorded["collection_id"] != collection["collection_id"]
    {
        return Err(invalid(
            "execution, collection and assessment identities disagree",
        ));
    }
    let now = recorded["assessed_at"]
        .as_str()
        .filter(|time| !time.is_empty())
        .ok_or_else(|| invalid("retained assessment time is missing"))?;
    let goal = claim
        .or_else(|| run["claim"].as_str())
        .filter(|claim| !claim.trim().is_empty())
        .ok_or_else(|| invalid("execution claim is missing; supply an exact EAL --claim"))?;
    let mut expected = recorded.clone();
    let expected_object = expected
        .as_object_mut()
        .ok_or_else(|| invalid("retained assessment must be an object"))?;
    expected_object.remove("assessment_id");
    expected_object.remove("collection_id");
    let expected_digest =
        eal_core::canonical::canonical_digest(&expected, &config.limits).map_err(invalid)?;
    let workspace = tempfile::tempdir()?;
    std::fs::write(workspace.path().join("case.eal"), source)?;
    std::fs::write(
        workspace.path().join("context.json"),
        serde_json::to_vec(&collection["context"])?,
    )?;
    std::fs::write(
        workspace.path().join("records.json"),
        serde_json::to_vec(&collection["records"])?,
    )?;
    std::fs::write(
        workspace.path().join("limits.toml"),
        toml::to_string(&LimitDocument {
            limits: &config.limits,
        })
        .map_err(|error| invalid(error.to_string()))?,
    )?;
    let remaining = || {
        config
            .timeout
            .checked_sub(started.elapsed())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| Error::Budget("ASPIC export deadline exhausted".into()))
    };
    let compiled = command(
        config,
        workspace.path(),
        &[
            "compile-aspic".into(),
            "case.eal".into(),
            "--claim".into(),
            goal.into(),
            "--now".into(),
            now.into(),
            "--context".into(),
            "@context.json".into(),
            "--records".into(),
            "records.json".into(),
        ],
        remaining()?,
    )
    .await?;
    // EAL's compiled source map records the digest of its recomputed assessment.
    // Storage IDs are the only fields outside the pure assessment contract.
    if compiled["source_map"]["assessment_digest"] != expected_digest {
        return Err(invalid(
            "source, observations, context, assessment time or method profile does not reproduce the retained EAL assessment",
        ));
    }
    std::fs::write(
        workspace.path().join("compiled.json"),
        serde_json::to_vec(&compiled)?,
    )?;
    let view = command(
        config,
        workspace.path(),
        &["export-aspic".into(), "compiled.json".into()],
        remaining()?,
    )
    .await?;
    if view["schema"] != "aspic-view/3" {
        return Err(Error::ToolExecution(
            "eal-core did not return an aspic-view/3 export".into(),
        ));
    }
    Ok(view)
}
