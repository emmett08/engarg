//! Launch only retained code whose contract fixtures and execution identity still match.
use super::{
    contracts::digest_json,
    package::load_verified,
    runner::{DockerRunner, ExecutionMode, FixtureRunner},
    verification::validate_observation,
};
use crate::Error;
use eal_collector_contracts::{
    AcquisitionRequest, JsonLimits, SensitiveJsonLimits, contains_sensitive_json, strict_json_exact,
};
use eal_process::CancellationToken;
use serde_json::Value;
use std::path::Path;

pub fn execute_registered(
    package: &Path,
    request_bytes: &[u8],
    supervisor: &Path,
) -> Result<Value, Error> {
    execute_registered_with_cancellation(
        package,
        request_bytes,
        supervisor,
        &CancellationToken::default(),
    )
}

pub fn execute_registered_with_cancellation(
    package: &Path,
    request_bytes: &[u8],
    supervisor: &Path,
    cancellation: &CancellationToken,
) -> Result<Value, Error> {
    let verified = load_verified(package)?;
    let value = strict_json_exact(
        request_bytes,
        JsonLimits {
            max_bytes: 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    let request: AcquisitionRequest = serde_json::from_value(value)?;
    request.validate().map_err(Error::InvalidInput)?;
    if request.tool_version != "1"
        || !eal_collector_contracts::identifier(&request.tool)
        || !eal_collector_contracts::identifier(&request.evidence_id)
        || !eal_collector_contracts::environment_name(&request.environment)
    {
        return Err(Error::InvalidInput(
            "generated collector request identity is invalid".into(),
        ));
    }
    // Registry aliases belong to the operator. Preserve the exact alias in the observation.
    let validator = jsonschema::validator_for(&verified.contract().draft.input_schema)
        .map_err(|_| Error::InvalidInput("verified collector input schema is invalid".into()))?;
    if !validator.is_valid(&request.input) {
        return Err(Error::InvalidInput(
            "collector input differs from its verified contract".into(),
        ));
    }
    let runner = DockerRunner::new(verified.profile().clone(), supervisor.to_path_buf())?;
    if digest_json(runner.identity())? != digest_json(verified.identity())? {
        return Err(Error::InvalidInput(
            "verified collector execution identity changed; verify it again".into(),
        ));
    }
    if cancellation.is_cancelled() {
        return Err(Error::Provider("collector execution cancelled".into()));
    }
    let outcome = runner
        .execute(
            verified
                .manifest_path()
                .parent()
                .ok_or_else(|| Error::InvalidInput("tool package has no directory".into()))?,
            &verified.candidate().files,
            &[],
            request_bytes,
            ExecutionMode::Runtime,
            cancellation,
        )
        .map_err(|_| Error::Provider("verified collector execution failed".into()))?;
    if cancellation.is_cancelled() || outcome.execution_error.is_some() || outcome.returncode != 0 {
        return Err(Error::Provider(
            "verified collector did not produce a successful acquisition".into(),
        ));
    }
    let envelope =
        validate_observation(verified.contract(), &request, &outcome.stdout).map_err(|_| {
            Error::InvalidOutput("collector returned an invalid or mismatched observation".into())
        })?;
    let value = serde_json::to_value(envelope)?;
    let secrets: Vec<String> = verified
        .profile()
        .inherit_env
        .iter()
        .map(|name| match std::env::var(name) {
            Ok(value) => Ok(value),
            Err(std::env::VarError::NotPresent) => Ok(String::new()),
            Err(std::env::VarError::NotUnicode(_)) => Err(Error::InvalidInput(
                "collector environment value is not UTF-8".into(),
            )),
        })
        .collect::<Result<_, _>>()?;
    let sensitive: Vec<_> = secrets
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    if contains_sensitive_json(&value, &sensitive, SensitiveJsonLimits::default())
        .map_err(|error| Error::InvalidInput(error.to_string()))?
    {
        return Err(Error::Provider(
            "collector output contains a forwarded credential".into(),
        ));
    }
    Ok(value)
}
