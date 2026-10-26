//! Supervised, bounded subprocess calls selected by the ticket operator.
use engarg::Error;
use serde_json::Value;
use std::{collections::BTreeMap, io::Write, path::Path, time::Duration};

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.into())
}
fn argv(raw: &str) -> Result<Vec<String>, Error> {
    let items: Vec<String> = serde_json::from_str(raw)
        .map_err(|_| invalid("command arguments require a JSON string array"))?;
    if items.is_empty()
        || items.len() > 128
        || items
            .iter()
            .any(|value| value.is_empty() || value.len() > 16_384 || value.contains('\0'))
    {
        return Err(invalid("command requires 1–128 nonempty bounded arguments"));
    }
    Ok(items)
}
pub(super) fn implementation_arguments(raw: &str) -> Result<Vec<String>, Error> {
    let items = argv(raw)?;
    let allowed = [
        "--model",
        "--service-tier",
        "--reasoning-effort",
        "--api",
        "--endpoint",
        "--api-key-env",
        "--max-input-bytes",
        "--max-response-bytes",
        "--max-output-tokens",
        "--http-retries",
        "--retry-max-delay-secs",
        "--request-timeout-secs",
        "--tool-language",
        "--toolchain",
        "--tool-store",
        "--tool-max-calls",
        "--tool-timeout-secs",
    ];
    let mut seen = std::collections::BTreeSet::new();
    let mut remaining = items.iter();
    while let Some(option) = remaining.next() {
        if !allowed.contains(&option.as_str()) || !seen.insert(option) {
            return Err(invalid(
                "implementation arguments contain an unsupported or repeated option",
            ));
        }
        if remaining.next().is_none_or(|value| value.starts_with("--")) {
            return Err(invalid("implementation options require an explicit value"));
        }
    }
    let mut normalized = items;
    for pair in normalized.as_chunks_mut::<2>().0 {
        if ["--toolchain", "--tool-store"].contains(&pair[0].as_str()) {
            pair[1] = std::path::absolute(&pair[1])?
                .to_string_lossy()
                .into_owned();
        }
    }
    Ok(normalized)
}
struct Cancel(eal_process::CancellationToken);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
async fn execute(
    directory: &Path,
    name: &str,
    argv: Vec<String>,
    input: Vec<u8>,
    timeout: u64,
) -> Result<Vec<u8>, Error> {
    let environment = std::env::vars_os()
        .map(|(key, value)| {
            Ok((
                key.into_string()
                    .map_err(|_| invalid("command environment name requires UTF-8"))?,
                value
                    .into_string()
                    .map_err(|_| invalid("command environment value requires UTF-8"))?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let cancellation = eal_process::CancellationToken::default();
    let _guard = Cancel(cancellation.clone());
    let command = eal_process::CommandRequest {
        argv,
        environment,
        cwd: directory.to_owned(),
        input,
        timeout: Duration::from_secs(timeout),
        max_output_bytes: 5 * 1_048_576,
    };
    let supervisor = super::super::supervisor()?.canonicalize()?;
    let outcome = tokio::task::spawn_blocking(move || {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            command,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: 4 * 1_048_576,
                max_stderr_bytes: 1_048_576,
            },
        )
    })
    .await
    .map_err(|_| invalid("integration command worker failed"))?
    .map_err(|_| invalid("integration command supervisor failed"))?;
    let stdout = directory.join(format!("{name}.stdout"));
    let stderr = directory.join(format!("{name}.stderr"));
    let mut file = crate::private_file(&stdout)?;
    file.write_all(&outcome.stdout)?;
    file.sync_all()?;
    let mut file = crate::private_file(&stderr)?;
    file.write_all(&outcome.stderr)?;
    file.sync_all()?;
    if outcome.returncode != 0 || outcome.execution_error.is_some() {
        return Err(invalid(
            "integration command failed; inspect the retained private command output",
        ));
    }
    Ok(outcome.stdout)
}
pub(super) async fn engarg(
    directory: &Path,
    name: &str,
    arguments: Vec<String>,
    timeout: u64,
) -> Result<(), Error> {
    let mut argv = vec![
        std::env::current_exe()?
            .canonicalize()?
            .to_string_lossy()
            .into_owned(),
    ];
    argv.extend(arguments);
    execute(directory, name, argv, Vec::new(), timeout)
        .await
        .map(|_| ())
}
pub(super) async fn qualification(
    directory: &Path,
    raw: &str,
    request: &Value,
    timeout: u64,
) -> Result<Value, Error> {
    let arguments = argv(raw)?;
    if !Path::new(&arguments[0]).is_absolute() {
        return Err(invalid(
            "qualification policy requires an absolute operator executable",
        ));
    }
    let mut input = serde_json::to_vec(request)?;
    input.push(b'\n');
    if input.len() > 16 * 1_048_576 {
        return Err(invalid("qualification request exceeds its byte bound"));
    }
    let output = execute(directory, "qualification", arguments, input, timeout).await?;
    eal_collector_contracts::strict_json_exact(
        &output,
        eal_collector_contracts::JsonLimits {
            max_bytes: 4 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn implementation_cannot_replace_stage_inputs_or_destinations() {
        assert!(implementation_arguments(r#"["--model","fixture","--tool-language","rust","--toolchain","/operator/profile.json"]"#).is_ok());
        for input in [
            r#"["--input","/other"]"#,
            r#"["--tool-output-dir","/other"]"#,
            r#"["--model","a","--model","b"]"#,
            r#"["--model"]"#,
            r#"["sh","-c","anything"]"#,
        ] {
            assert!(implementation_arguments(input).is_err(), "{input}");
        }
    }
}
