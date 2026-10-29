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
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

fn script(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("assistant.py");
    fs::write(&path, format!("#!/usr/bin/python3\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn shell_script(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("assistant.sh");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

fn held_protocol_failure_supervisor(directory: &Path) -> PathBuf {
    let quoted = directory.to_str().unwrap().replace('\'', "'\"'\"'");
    shell_script(
        directory,
        &format!(
            r#"
directory='{quoted}'
if [ -e "$directory/first-started" ]; then
    printf second > "$directory/second-started"
    printf invalid-supervisor-frame
else
    printf first > "$directory/first-started"
    pwd > "$directory/cwd"
    printf invalid-supervisor-frame
    printf ready > "$directory/ready"
    remaining=500
    while [ ! -e "$directory/release" ] && [ "$remaining" -gt 0 ]; do
        /bin/sleep 0.01
        remaining=$((remaining - 1))
    done
fi
"#
        ),
    )
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

fn descendant_lifetime(directory: &Path) -> (PathBuf, fs::File) {
    let fifo = directory.join("descendant-lifetime");
    // mkfifoat is absent from rustix's Apple API. The POSIX utility creates
    // this owned fixture before the model's logical deadline starts.
    assert!(
        std::process::Command::new("mkfifo")
            .args(["-m", "600"])
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let reader = rustix::fs::open(
        &fifo,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    (fifo, fs::File::from(reader))
}

async fn wait_for_descendant_shutdown(reader: &mut fs::File) -> bool {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut byte = [0u8; 1];
        loop {
            match reader.read(&mut byte) {
                Ok(0) => break true,
                Ok(_) => break false,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

fn fixture_process_snapshot(directory: &Path, fixture_pids: &[u32]) -> String {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid,ppid,pgid,state,command"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let rows = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((
                fields.next()?.parse::<u32>().ok()?,
                fields.next()?.parse::<u32>().ok()?,
                line,
            ))
        })
        .collect::<Vec<_>>();
    let mut selected = fixture_pids.iter().copied().collect::<HashSet<_>>();
    // Include the supervising process once, then only descendants of the
    // selected fixture tree. Do not include unrelated host process commands.
    if let Some((_, parent, _)) = rows.iter().find(|(pid, _, _)| *pid == fixture_pids[0]) {
        selected.insert(*parent);
    }
    let root = directory.to_str().unwrap();
    loop {
        let previous = selected.len();
        for (pid, parent, line) in &rows {
            if selected.contains(parent) || line.contains(root) {
                selected.insert(*pid);
            }
        }
        if previous == selected.len() {
            break;
        }
    }
    std::iter::once("PID PPID PGID STATE COMMAND")
        .chain(
            rows.iter()
                .filter_map(|(pid, _, line)| selected.contains(pid).then_some(*line)),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn deadline_and_caller_cancellation_stop_process_descendants() {
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let ready = root.path().join("ready");
        let escaped = root.path().join("escaped");
        let effect_gate = root.path().join("allow-effect");
        let stop_gate = root.path().join("stop-fixture");
        let options = root.path().join("shell-options");
        let parent_pid = root.path().join("parent-pid");
        let descendant_pid = root.path().join("descendant-pid");
        let (lifetime, mut reader) = descendant_lifetime(root.path());
        // The deadline constrains the owned process tree. A shell fixture
        // avoids measuring two cold Python interpreter startups on macOS.
        let executable = shell_script(
            root.path(),
            r#"
set -o > "$6"
printf '%s\n' "$$" > "$7"
(
    trap '' TERM
    exec 3>"$3"
    printf ready > "$1"
    remaining=10
    while [ ! -e "$4" ] && [ ! -e "$5" ] && [ "$remaining" -gt 0 ]; do
        /bin/sleep 1
        remaining=$((remaining - 1))
    done
    if [ -e "$4" ]; then
        printf escaped > "$2"
    fi
) &
printf '%s\n' "$!" > "$8"
wait
"#,
        );
        let mut config = config(&executable, AssistantKind::Command);
        config.argv.extend([
            ready.to_str().unwrap().into(),
            escaped.to_str().unwrap().into(),
            lifetime.to_str().unwrap().into(),
            effect_gate.to_str().unwrap().into(),
            stop_gate.to_str().unwrap().into(),
            options.to_str().unwrap().into(),
            parent_pid.to_str().unwrap().into(),
            descendant_pid.to_str().unwrap().into(),
        ]);
        config.timeout = if cancelled {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(400)
        };
        let model = Arc::new(CommandModel::new(config).unwrap());
        let started = Instant::now();
        let worker = tokio::spawn(async move { model.complete(request()).await });
        wait_for_file(&ready).await;
        wait_for_file(&descendant_pid).await;
        let fixture_pids = [&parent_pid, &descendant_pid].map(|path| {
            fs::read_to_string(path)
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap()
        });
        let before_cancellation = fixture_process_snapshot(root.path(), &fixture_pids);
        let before_cancellation_ms = started.elapsed().as_millis();
        let shell_options = fs::read_to_string(options).unwrap();
        match reader.read(&mut [0u8; 1]) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            // Opening the writer precedes ready. The deadline may correctly
            // close it before a descheduled observer resumes; an explicit
            // cancellation still requires a live writer before aborting.
            Ok(0) if !cancelled => {}
            result => panic!("unexpected descendant lifetime before cancellation: {result:?}"),
        }
        if cancelled {
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
        } else {
            let error = worker.await.unwrap().unwrap_err().to_string();
            assert!(error.contains("deadline"));
            assert!(error.contains("no automatic retry"));
        }
        let caller_returned_ms = started.elapsed().as_millis();
        // The caller's deadline does not acknowledge cleanup. The child holds
        // its only FIFO writer throughout execution; EOF establishes that its
        // execution capability has ended, including when an orphan PID lingers.
        let stopped_on_cancellation = wait_for_descendant_shutdown(&mut reader).await;
        let shutdown_observed_ms = started.elapsed().as_millis();
        let after_cancellation = if stopped_on_cancellation {
            String::new()
        } else {
            fixture_process_snapshot(root.path(), &fixture_pids)
        };
        // Clean up a failing bounded fixture without allowing its effect.
        fs::write(&stop_gate, "stop").unwrap();
        let fixture_cleaned_up = wait_for_descendant_shutdown(&mut reader).await;
        assert!(
            fixture_cleaned_up,
            "the bounded descendant fixture did not stop"
        );
        assert!(
            stopped_on_cancellation,
            "a descendant retained its execution capability: cancelled={cancelled}, before_cancellation_ms={before_cancellation_ms}, caller_returned_ms={caller_returned_ms}, shutdown_observed_ms={shutdown_observed_ms}\nshell options:\n{shell_options}\nbefore cancellation:\n{before_cancellation}\nafter cancellation:\n{after_cancellation}"
        );
        fs::write(effect_gate, "allow effect").unwrap();
        assert!(
            !escaped.exists(),
            "a descendant produced an effect after cleanup acknowledgement"
        );
    }
}

#[tokio::test]
async fn cancelled_process_keeps_its_admission_slot_until_descendants_stop() {
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("child-ready");
    let executable = script(
        root.path(),
        r#"
import fcntl, json, os, signal, sys, time
request = json.load(sys.stdin)
lease_path = sys.argv[1] + '.lease'
if request['request']['user'] == 'first':
    if os.fork() == 0:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        with open(lease_path, 'a+') as lease:
            fcntl.flock(lease, fcntl.LOCK_EX)
            with open(sys.argv[1], 'w') as ready:
                ready.write('locked')
            time.sleep(10)
        os._exit(0)
    time.sleep(10)
else:
    with open(lease_path, 'a+') as lease:
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            previous_executing = False
        except BlockingIOError:
            previous_executing = True
    print(json.dumps({'content':json.dumps({'previous_executing':previous_executing}),'model':'m','input_tokens':None,'output_tokens':None,'request_id':None}))
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
    // A dead orphan can retain its PID as a zombie on macOS. The kernel
    // releases this child's continuously held file lock when it terminates,
    // so it measures execution ownership instead of PID-table occupancy.
    assert_eq!(content["previous_executing"], false);
}

#[tokio::test]
async fn protocol_failure_retains_directory_and_admission_until_supervisor_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("ready");
    let second_started = root.path().join("second-started");
    let release = root.path().join("release");
    let executable = script(root.path(), ECHO);
    let mut config = config(&executable, AssistantKind::Command);
    config.supervisor = held_protocol_failure_supervisor(root.path());
    let model = Arc::new(CommandModel::new(config).unwrap());
    let first_model = model.clone();
    let first = tokio::spawn(async move { first_model.complete(request()).await });
    wait_for_file(&ready).await;
    let cwd = fs::read_to_string(root.path().join("cwd")).unwrap();
    let second = tokio::spawn(async move { model.complete(request()).await });
    // The malformed frame makes the protocol reader return immediately. The
    // fake supervisor deliberately retains its inherited lifetime endpoint.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let retained_before_shutdown =
        !first.is_finished() && !second_started.exists() && Path::new(cwd.trim()).is_dir();
    // Release the bounded fixture before asserting, including on failure.
    fs::write(release, "release").unwrap();
    assert!(first.await.unwrap().is_err());
    assert!(second.await.unwrap().is_err());
    assert!(retained_before_shutdown);
    assert!(second_started.exists());
    assert!(!Path::new(cwd.trim()).exists());
}

#[tokio::test]
async fn runtime_shutdown_keeps_its_deadline_while_worker_retains_cleanup_ownership() {
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("ready");
    let second_started = root.path().join("second-started");
    let release = root.path().join("release");
    let executable = script(root.path(), ECHO);
    let mut config = config(&executable, AssistantKind::Command);
    config.supervisor = held_protocol_failure_supervisor(root.path());
    config.timeout = Duration::from_millis(400);
    let model = Arc::new(CommandModel::new(config).unwrap());
    let first_model = model.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    let runtime_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(first_model.complete(request()))
            .unwrap_err()
            .to_string();
        drop(runtime);
        sender.send(error).unwrap();
    });
    wait_for_file(&ready).await;
    let cwd = fs::read_to_string(root.path().join("cwd")).unwrap();
    let runtime_exited = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match receiver.try_recv() {
                Ok(error) => break Some(error),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
    })
    .await;
    let exited_before_shutdown =
        matches!(&runtime_exited, Ok(Some(error)) if error.contains("deadline"));
    let second = tokio::spawn(async move { model.complete(request()).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let ownership_retained = !second_started.exists() && Path::new(cwd.trim()).is_dir();
    // Even a failing implementation is allowed to unwind its bounded fixture
    // before the test joins the separate runtime thread or asserts.
    fs::write(release, "release").unwrap();
    assert!(second.await.unwrap().is_err());
    runtime_thread.join().unwrap();
    assert!(
        exited_before_shutdown,
        "runtime waited for supervisor cleanup"
    );
    assert!(ownership_retained);
    assert!(second_started.exists());
    assert!(!Path::new(cwd.trim()).exists());
}
