//! Pure collector-side native configuration preparation, without source calls.
use engarg::{Error, tooling::contracts::digest_json};
use serde_json::Value;
use std::{collections::BTreeMap, io::Write, path::Path, time::Duration};

pub async fn prepare(
    collector: &Path,
    supervisor: &Path,
    input: &Value,
    path: &Path,
) -> Result<Value, Error> {
    let mut file = crate::private_file(path)?;
    serde_json::to_writer_pretty(&mut file, input)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    let cancellation = eal_process::CancellationToken::default();
    struct Cancel(eal_process::CancellationToken);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let _guard = Cancel(cancellation.clone());
    let command = eal_process::CommandRequest {
        argv: vec![
            collector.to_string_lossy().into_owned(),
            "prepare-native-provider".into(),
            "--input".into(),
            path.to_string_lossy().into_owned(),
        ],
        environment: BTreeMap::new(),
        cwd: path.parent().unwrap_or(Path::new(".")).to_owned(),
        input: Vec::new(),
        timeout: Duration::from_secs(30),
        max_output_bytes: 4 * 1_048_576,
    };
    let supervisor = supervisor.to_owned();
    let outcome = tokio::task::spawn_blocking(move || {
        eal_process::Supervisor::new(supervisor).run_with_stream_limits_and_cancellation(
            command,
            None,
            &cancellation,
            eal_process::StreamLimits {
                max_stdout_bytes: 4 * 1_048_576,
                max_stderr_bytes: 65536,
            },
        )
    })
    .await
    .map_err(|_| Error::Provider("native registration preparation worker failed".into()))?
    .map_err(|_| Error::Provider("native registration preparation could not start".into()))?;
    let response = path.with_extension("response.json");
    let mut retained = crate::private_file(&response)?;
    retained.write_all(&outcome.stdout)?;
    retained.sync_all()?;
    if outcome.returncode != 0 || outcome.execution_error.is_some() {
        return Err(Error::InvalidInput(format!(
            "collector rejected native registration; complete bounded response retained at {}. Use a matching collector supporting prepare-native-provider and valid operator qualification",
            response.display()
        )));
    }
    let prepared = eal_collector_contracts::strict_json_exact(
        &outcome.stdout,
        eal_collector_contracts::JsonLimits {
            max_bytes: 4 * 1_048_576,
            ..Default::default()
        },
    )
    .map_err(Error::InvalidInput)?;
    let mut expected = input.clone();
    let mut actual = prepared.clone();
    expected["capability"]["provider"]["configuration_sha256"] = Value::Null;
    actual["capability"]["provider"]["configuration_sha256"] = Value::Null;
    if digest_json(&expected)? != digest_json(&actual)? {
        return Err(Error::InvalidOutput(
            "native preparation changed fields other than the configuration identity".into(),
        ));
    }
    Ok(prepared)
}
