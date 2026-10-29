#![cfg(unix)]
//! Real supervised assistant exchanges, bounds and cancellation ownership.
use engarg::{
    Error,
    provider::{
        AssistantKind, CommandConfig, CommandModel, CompletionRequest, LanguageModel, Role,
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn script(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("assistant.py");
    fs::write(&path, format!("#!/usr/bin/python3\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn config(executable: &Path, kind: AssistantKind) -> CommandConfig {
    CommandConfig {
        kind,
        argv: vec![executable.to_str().unwrap().into()],
        inherit_env: vec![],
        model: "configured".into(),
        role_models: HashMap::from([(Role::Sceptic, "role-model".into())]),
        timeout: Duration::from_secs(5),
        max_input_bytes: 10_000,
        max_output_bytes: 10_000,
        max_output_tokens: 123,
        max_concurrent_requests: 1,
        supervisor: env!("CARGO_BIN_EXE_engarg-process-supervisor").into(),
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        role: Role::Sceptic,
        system: "Use only the supplied evidence.".into(),
        user: "Untrusted literal $(command) with 'quotes'\nand newline.".into(),
        schema: json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}

const ECHO: &str = r#"
import json, os, sys
received = json.load(sys.stdin)
content = json.dumps({"received":received,"argv":sys.argv[1:],"cwd":os.getcwd(),"files":os.listdir('.'),"environment":sorted(os.environ)})
print(json.dumps({"content":content,"model":"actual","input_tokens":7,"output_tokens":9,"request_id":"r1"}))
"#;

#[tokio::test]
async fn generic_process_receives_exact_schema_role_model_and_selected_environment() {
    let root = tempfile::tempdir().unwrap();
    let executable = script(root.path(), ECHO);
    let mut config = config(&executable, AssistantKind::Command);
    config.argv.push("literal $(argument)".into());
    config.inherit_env.push("PATH".into());
    let completion = CommandModel::new(config)
        .unwrap()
        .complete(request())
        .await
        .unwrap();
    assert_eq!(completion.model, "actual");
    assert_eq!(completion.input_tokens, Some(7));
    assert_eq!(completion.output_tokens, Some(9));
    assert_eq!(completion.request_id.as_deref(), Some("r1"));
    let content: Value = serde_json::from_str(&completion.content).unwrap();
    assert_eq!(
        content["received"]["schema_version"],
        "engarg.completion.v1"
    );
    assert_eq!(content["received"]["model"], "role-model");
    assert_eq!(content["received"]["max_output_tokens"], 123);
    assert_eq!(
        content["received"]["request"],
        serde_json::to_value(request()).unwrap()
    );
    assert_eq!(content["argv"], json!(["literal $(argument)"]));
    assert_eq!(content["files"], json!([]));
    let names = content["environment"].as_array().unwrap();
    assert!(names.contains(&json!("PATH")));
    assert!(!names.contains(&json!("HOME")));
    assert!(!names.contains(&json!("OPENAI_API_KEY")));
    assert!(!Path::new(content["cwd"].as_str().unwrap()).exists());
}

#[tokio::test]
async fn native_processes_receive_documented_flags_and_return_truthful_completion_metadata() {
    for kind in [AssistantKind::Cursor, AssistantKind::Augment] {
        let root = tempfile::tempdir().unwrap();
        let body = r#"
import json, sys
prompt = json.load(sys.stdin)
content = json.dumps({"argv":sys.argv[1:],"prompt":prompt})
if '--output-format' in sys.argv:
    print(json.dumps({"type":"result","subtype":"success","is_error":False,"result":content,"request_id":"native-r1"}))
else:
    print(content)
"#;
        let executable = script(root.path(), body);
        let completion = CommandModel::new(config(&executable, kind))
            .unwrap()
            .complete(request())
            .await
            .unwrap();
        let content: Value = serde_json::from_str(&completion.content).unwrap();
        let argv = content["argv"].as_array().unwrap();
        assert!(argv.contains(&json!("--print")));
        assert!(argv.contains(&json!("--model")));
        assert!(argv.contains(&json!("role-model")));
        assert_eq!(content["prompt"]["system"], request().system);
        assert_eq!(content["prompt"]["user"], request().user);
        assert_eq!(content["prompt"]["output_schema"], request().schema);
        assert_eq!(completion.input_tokens, None);
        assert_eq!(completion.output_tokens, None);
        match kind {
            AssistantKind::Cursor => {
                assert!(argv.contains(&json!("ask")));
                assert!(argv.contains(&json!("json")));
                assert_eq!(completion.model, "assistant:cursor:role-model");
                assert_eq!(completion.request_id.as_deref(), Some("native-r1"));
            }
            AssistantKind::Augment => {
                assert!(argv.contains(&json!("--quiet")));
                assert_eq!(completion.model, "assistant:augment:role-model");
                assert_eq!(completion.request_id, None);
            }
            AssistantKind::Command => unreachable!(),
        }
    }
}

#[tokio::test]
async fn failed_processes_redact_child_output_and_never_retry() {
    let root = tempfile::tempdir().unwrap();
    let attempts = root.path().join("attempts");
    let executable = script(
        root.path(),
        r#"
import sys
with open(sys.argv[1], 'a') as file:
    file.write('attempt\n')
print('private stdout')
print('private stderr', file=sys.stderr)
sys.exit(7)
"#,
    );
    let mut config = config(&executable, AssistantKind::Command);
    config.argv.push(attempts.to_str().unwrap().into());
    let error = CommandModel::new(config)
        .unwrap()
        .complete(request())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("exited unsuccessfully"));
    assert!(error.contains("no automatic retry"));
    assert!(!error.contains("private"));
    assert_eq!(fs::read_to_string(attempts).unwrap(), "attempt\n");
}

#[tokio::test]
async fn stdout_and_stderr_have_independent_hard_limits() {
    for stream in ["stdout", "stderr"] {
        let root = tempfile::tempdir().unwrap();
        let executable = script(
            root.path(),
            &format!("import sys\nsys.{stream}.write('x' * 100000)\nsys.{stream}.flush()"),
        );
        let mut config = config(&executable, AssistantKind::Command);
        config.max_output_bytes = 128;
        let result = CommandModel::new(config).unwrap().complete(request()).await;
        assert!(
            matches!(result, Err(Error::Budget(_))),
            "{stream}: {result:?}"
        );
    }
}

#[tokio::test]
async fn subprocess_concurrency_obeys_the_configured_admission_limit() {
    let root = tempfile::tempdir().unwrap();
    let journal = root.path().join("journal");
    let executable = script(
        root.path(),
        r#"
import json, sys, time
with open(sys.argv[1], 'a') as file:
    file.write('start\n')
time.sleep(0.15)
with open(sys.argv[1], 'a') as file:
    file.write('finish\n')
print(json.dumps({"content":"{}","model":"m","input_tokens":None,"output_tokens":None,"request_id":None}))
"#,
    );
    let mut config = config(&executable, AssistantKind::Command);
    config.argv.push(journal.to_str().unwrap().into());
    let model = CommandModel::new(config).unwrap();
    let (first, second) = tokio::join!(model.complete(request()), model.complete(request()));
    first.unwrap();
    second.unwrap();
    assert_eq!(
        fs::read_to_string(journal).unwrap(),
        "start\nfinish\nstart\nfinish\n"
    );
}

async fn wait_for_file(path: &Path) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn deadline_and_caller_cancellation_stop_process_descendants() {
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let ready = root.path().join("ready");
        let escaped = root.path().join("escaped");
        let executable = script(
            root.path(),
            r#"
import json, os, subprocess, sys, time
child = "import signal,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); open(sys.argv[1], 'w').write('ready'); time.sleep(0.8); open(sys.argv[2], 'w').write('escaped')"
subprocess.Popen(['/usr/bin/python3', '-c', child, sys.argv[1], sys.argv[2]])
time.sleep(10)
"#,
        );
        let mut config = config(&executable, AssistantKind::Command);
        config.argv.extend([
            ready.to_str().unwrap().into(),
            escaped.to_str().unwrap().into(),
        ]);
        config.timeout = if cancelled {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(400)
        };
        let model = Arc::new(CommandModel::new(config).unwrap());
        let worker = tokio::spawn(async move { model.complete(request()).await });
        wait_for_file(&ready).await;
        if cancelled {
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
        } else {
            let error = worker.await.unwrap().unwrap_err().to_string();
            assert!(error.contains("deadline"));
            assert!(error.contains("no automatic retry"));
        }
        tokio::time::sleep(Duration::from_millis(950)).await;
        assert!(
            !escaped.exists(),
            "a descendant survived assistant cancellation"
        );
    }
}

#[tokio::test]
async fn cancelled_process_keeps_its_admission_slot_until_descendants_are_gone() {
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("child-pid");
    let executable = script(
        root.path(),
        r#"
import json, os, subprocess, sys, time
request = json.load(sys.stdin)
if request['request']['user'] == 'first':
    child = "import os,signal,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); open(sys.argv[1], 'w').write(str(os.getpid())); time.sleep(10)"
    subprocess.Popen(['/usr/bin/python3', '-c', child, sys.argv[1]])
    time.sleep(10)
else:
    with open(sys.argv[1]) as file:
        pid = int(file.read())
    try:
        os.kill(pid, 0)
        previous_alive = True
    except ProcessLookupError:
        previous_alive = False
    print(json.dumps({'content':json.dumps({'previous_alive':previous_alive}),'model':'m','input_tokens':None,'output_tokens':None,'request_id':None}))
"#,
    );
    let mut config = config(&executable, AssistantKind::Command);
    config.argv.push(ready.to_str().unwrap().into());
    let model = Arc::new(CommandModel::new(config).unwrap());
    let first_model = model.clone();
    let first = tokio::spawn(async move {
        let mut input = request();
        input.user = "first".into();
        first_model.complete(input).await
    });
    wait_for_file(&ready).await;
    let second = tokio::spawn(async move { model.complete(request()).await });
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let completion = second.await.unwrap().unwrap();
    let content: Value = serde_json::from_str(&completion.content).unwrap();
    assert_eq!(content["previous_alive"], false);
}
