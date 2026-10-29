//! Installed assistant protocols reach the normal authoring and admission path.
#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[path = "common/argument_assertions.rs"]
mod argument_assertions;
#[path = "common/live_output.rs"]
mod live_output;

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engarg"));
    for name in [
        "ENGARG_PROVIDER",
        "ENGARG_MODEL",
        "ENGARG_SERVICE_TIER",
        "ENGARG_REASONING_EFFORT",
        "ENGARG_API_KEY",
        "OPENAI_API_KEY",
        "EAL_HOST",
    ] {
        command.env_remove(name);
    }
    command
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Assistant {
    directory: tempfile::TempDir,
    config: PathBuf,
    calls: PathBuf,
    report: PathBuf,
}

impl Assistant {
    fn new(kind: &str, argument: &Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("assistant.py");
        let response = directory.path().join("argument.json");
        let config = directory.path().join("assistant.toml");
        let calls = directory.path().join("calls.jsonl");
        let report = directory.path().join("analysis.json");
        fs::write(&response, serde_json::to_vec(argument).unwrap()).unwrap();
        fs::write(
            &script,
            r#"import json, os, pathlib, sys
kind, argument_file, calls_file = sys.argv[1:4]
prompt = sys.stdin.read()
with open(calls_file, 'a') as log:
    log.write(json.dumps({'argv': sys.argv[4:], 'stdin': prompt, 'cwd': os.getcwd(),
                         'secret_inherited': 'ASSISTANT_CLI_PRIVATE_VALUE' in os.environ}) + '\n')
argument = json.loads(pathlib.Path(argument_file).read_text())
content = json.dumps(argument)
if kind == 'cursor':
    print(json.dumps({'type': 'result', 'subtype': 'success', 'is_error': False,
                      'result': content, 'session_id': 'synthetic-session'}))
elif kind == 'augment':
    print(content)
else:
    print(json.dumps({'content': content, 'model': 'adapter-reported-model',
                      'input_tokens': None, 'output_tokens': None, 'request_id': None,
                      'service_tier': None, 'reasoning_effort': None}))
"#,
        )
        .unwrap();
        let settings = json!({
            "argv": ["python3", script, kind, response, calls],
            "inherit_env": ["PATH"]
        });
        fs::write(&config, toml::to_string(&settings).unwrap()).unwrap();
        Self {
            directory,
            config,
            calls,
            report,
        }
    }

    fn analyse(&self, provider: &str) -> Command {
        let mut command = cli();
        command
            .env("ASSISTANT_CLI_PRIVATE_VALUE", "private-test-value")
            .args(["analyse", "--input"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/claim-only-request.json"))
            .args(["--provider", provider, "--assistant-config"])
            .arg(&self.config)
            .args([
                "--model",
                "selected-model",
                "--role-model",
                "single=selected-role-model",
                "--mode",
                "single",
                "--no-tool-planning",
                "--max-calls",
                "1",
                "--format",
                "json",
                "--request-timeout-secs",
                "10",
                "--timeout-secs",
                "20",
                "--output",
            ])
            .arg(&self.report);
        command
    }

    fn records(&self) -> Vec<Value> {
        fs::read_to_string(&self.calls)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn argument() -> Value {
    live_output::generation_value(&live_output::value(include_str!(
        "../examples/claim-only-case.json"
    )))
}

#[test]
fn historical_http_provider_configuration_round_trips_without_assistant_metadata() {
    let original = json!({
        "endpoint": "https://example.test/v1/responses",
        "api": "Responses",
        "default_model": "recorded-model",
        "requested_service_tier": null,
        "requested_reasoning_effort": null,
        "role_models": {"sceptic": "recorded-reviewer"},
        "max_output_tokens": 10000,
        "request_timeout_secs": 120,
        "timeout_secs": 480
    });
    let admitted: engarg::engine::ProviderRunConfig =
        serde_json::from_value(original.clone()).unwrap();
    assert!(admitted.assistant.is_none());
    assert_eq!(serde_json::to_value(admitted).unwrap(), original);
}

#[test]
fn generic_assistant_authors_without_an_api_key_and_retains_private_config_identity() {
    let assistant = Assistant::new("command", &argument());
    let output = assistant.analyse("command").output().unwrap();
    succeeded(&output);
    let report: Value = serde_json::from_slice(&fs::read(&assistant.report).unwrap()).unwrap();
    let records = assistant.records();
    assert_eq!(records.len(), 1);
    let prompt: Value = serde_json::from_str(records[0]["stdin"].as_str().unwrap()).unwrap();
    assert_eq!(prompt["schema_version"], "engarg.completion.v1");
    assert_eq!(prompt["model"], "selected-role-model");
    assert_eq!(prompt["request"]["role"], "single");
    assert!(prompt["request"]["schema"]["properties"]["original_claim"].is_object());
    assert!(!prompt["request"]["system"].as_str().unwrap().is_empty());
    assert!(records[0]["argv"].as_array().unwrap().is_empty());
    assert_eq!(records[0]["secret_inherited"], false);
    assert_ne!(
        records[0]["cwd"].as_str().unwrap(),
        assistant.directory.path().to_str().unwrap()
    );
    argument_assertions::assert_authored_argument(&report, &argument());
    assert_eq!(
        report["provider_config"]["assistant"]["provider"],
        "command"
    );
    assert_eq!(
        report["provider_config"]["assistant"]["configuration_sha256"],
        engarg::authoring_checkpoint::digest(fs::read(&assistant.config).unwrap())
    );
    assert_eq!(report["provider_config"]["endpoint"], "command:command");
    assert_eq!(report["provider_config"]["api"], "AssistantCommand");
    assert_eq!(
        report["transcript"][0]["response"]["model"],
        "adapter-reported-model"
    );
    for key in [
        "input_tokens",
        "output_tokens",
        "service_tier",
        "reasoning_effort",
    ] {
        assert!(report["transcript"][0]["response"][key].is_null());
    }
    let metadata = report["provider_config"].to_string();
    assert!(!metadata.contains("assistant.py"));
    assert!(!metadata.contains("private-test-value"));
    assert!(!metadata.contains("inherit_env"));
    engarg::report::read(&report).unwrap();
}

#[test]
fn native_assistants_receive_the_schema_and_preserve_unknown_usage() {
    for (provider, expected_flags) in [
        (
            "cursor",
            vec![
                "--print",
                "--mode",
                "ask",
                "--output-format",
                "json",
                "--model",
                "selected-role-model",
            ],
        ),
        (
            "augment",
            vec!["--print", "--quiet", "--model", "selected-role-model"],
        ),
    ] {
        let assistant = Assistant::new(provider, &argument());
        let output = assistant.analyse(provider).output().unwrap();
        succeeded(&output);
        let records = assistant.records();
        assert_eq!(records.len(), 1);
        let argv = records[0]["argv"].as_array().unwrap();
        assert_eq!(argv.len(), expected_flags.len() + 1);
        for (actual, expected) in argv.iter().zip(expected_flags) {
            assert_eq!(actual, expected);
        }
        assert!(argv.last().unwrap().as_str().unwrap().contains("stdin"));
        let prompt: Value = serde_json::from_str(records[0]["stdin"].as_str().unwrap()).unwrap();
        assert_eq!(prompt["role"], "single");
        assert!(prompt["output_schema"]["properties"]["original_claim"].is_object());
        assert!(!prompt["system"].as_str().unwrap().is_empty());
        assert!(!prompt["user"].as_str().unwrap().is_empty());
        let report: Value = serde_json::from_slice(&fs::read(&assistant.report).unwrap()).unwrap();
        argument_assertions::assert_authored_argument(&report, &argument());
        assert_eq!(report["provider_config"]["assistant"]["provider"], provider);
        assert_eq!(
            report["transcript"][0]["response"]["model"],
            format!("assistant:{provider}:selected-role-model")
        );
        for key in [
            "input_tokens",
            "output_tokens",
            "service_tier",
            "reasoning_effort",
        ] {
            assert!(report["transcript"][0]["response"][key].is_null());
        }
    }
}

#[test]
fn malformed_authored_argument_is_retained_in_audit_without_report_or_retry() {
    let assistant = Assistant::new("command", &json!({}));
    let audit = assistant.directory.path().join("audit.jsonl");
    let output = assistant
        .analyse("command")
        .arg("--audit")
        .arg(&audit)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid model output"));
    assert!(!assistant.report.exists());
    assert_eq!(assistant.records().len(), 1);
    let events: Vec<Value> = fs::read_to_string(audit)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events.iter().any(|event| {
        event["event"] == "completion" && event["record"]["response"]["content"] == "{}"
    }));
}

#[test]
fn every_model_backed_recovery_and_implementation_command_accepts_assistant_flags() {
    for command in [
        vec!["authoring", "resume"],
        vec!["eal", "repair"],
        vec!["eal", "implement"],
    ] {
        let output = cli()
            .args(command)
            .args([
                "--provider",
                "command",
                "--assistant-config",
                "not-opened.toml",
                "--help",
            ])
            .output()
            .unwrap();
        succeeded(&output);
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--provider"));
        assert!(help.contains("--assistant-config"));
        assert!(help.contains("http, cursor, augment, command"));
    }
}

#[test]
fn assistant_admission_rejects_mixed_http_settings_and_invalid_private_config() {
    let assistant = Assistant::new("cursor", &argument());
    for options in [
        vec!["--api", "chat"],
        vec!["--endpoint", "https://example.invalid/v1/responses"],
        vec!["--service-tier", "priority"],
        vec!["--reasoning-effort", "high"],
    ] {
        let output = assistant.analyse("cursor").args(options).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("do not accept HTTP"));
        assert!(!assistant.calls.exists());
        assert!(!assistant.report.exists());
    }
    fs::write(
        &assistant.config,
        "argv = [\"private-argument-value\"]\ninherit_env = []\nunrecognised = true\n",
    )
    .unwrap();
    let output = assistant.analyse("cursor").output().unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("invalid assistant configuration TOML"));
    assert!(!error.contains("private-argument-value"));
    assert!(!assistant.calls.exists());
}
