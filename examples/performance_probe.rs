//! Reproducible local probes without provider calls or external evidence.
use std::{path::PathBuf, time::Instant};

use engarg::{
    authoring_checkpoint::{Checkpoint, Store, digest},
    domain::Request,
    engine::{CallRecord, Mode, RunConfig},
    provider::{Completion, CompletionRequest, Role},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("checkpoint") if args.len() == 3 => checkpoint(args[1].parse()?, args[2].parse()?),
        Some("digest") if args.len() == 3 => {
            let path = PathBuf::from(&args[1]);
            let iterations: usize = args[2].parse()?;
            if !(1..=100).contains(&iterations) { return Err("iterations must be 1–100".into()); }
            let started = Instant::now();
            let mut result = String::new();
            for _ in 0..iterations {
                result = engarg::tooling::runner::executable_digest(&path)?;
            }
            println!("{}", json!({"workload":"executable_digest","iterations":iterations,
                "input_bytes":std::fs::metadata(&path)?.len(),"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,
                "sha256":result}));
            Ok(())
        }
        _ => Err("usage: performance_probe checkpoint CALLS RESPONSE_BYTES | digest EXECUTABLE ITERATIONS".into()),
    }
}

fn checkpoint(calls: usize, response_bytes: usize) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=24).contains(&calls) || !(1024..=4 * 1_048_576).contains(&response_bytes) {
        return Err("calls must be 1–24 and response bytes 1024–4194304".into());
    }
    if calls.saturating_mul(response_bytes) > 12 * 1_048_576 {
        return Err("probe payload must fit within 12 MiB for record admission".into());
    }
    let request = Request {
        claim: "Synthetic checkpoint throughput probe".into(),
        context: String::new(),
        evidence: vec![],
    };
    let source = json!({"request":request}).to_string();
    let response = json!({"synthetic_padding":"x".repeat(response_bytes)}).to_string();
    let call = CallRecord {
        role: Role::Single,
        prompt: CompletionRequest {
            role: Role::Single,
            system: "Synthetic performance probe".into(),
            user: source,
            schema: json!({}),
        },
        response_content_bytes: response.len(),
        response_content_sha256: digest(&response),
        response_content_omitted: false,
        response: Completion {
            content: response,
            model: "synthetic-local".into(),
            input_tokens: None,
            output_tokens: None,
            request_id: None,
            service_tier: None,
            reasoning_effort: None,
        },
        elapsed_ms: 0,
    };
    let config = RunConfig {
        mode: Mode::Single,
        max_calls: calls as u32,
        max_response_bytes: 4 * 1_048_576 + 1024,
        ..RunConfig::default()
    };
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("checkpoint.json");
    let started = Instant::now();
    let store = Store::create(
        &path,
        Checkpoint::new(request, config, json!({}), "f".repeat(64)),
    )?;
    for _ in 0..calls {
        store.completion(&call)?;
    }
    let write_ms = started.elapsed().as_secs_f64() * 1000.0;
    let write_peak_rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
            })
        });
    let saved = Checkpoint::read(&path)?;
    assert_eq!(saved.transcript.len(), calls);
    assert!(
        saved
            .transcript
            .iter()
            .all(|item| item.response.content == call.response.content)
    );
    println!(
        "{}",
        json!({"workload":"checkpoint","calls":calls,"response_bytes":response_bytes,
        "write_ms":write_ms,"write_peak_rss_kib":write_peak_rss_kib,"end_to_end_ms":started.elapsed().as_secs_f64()*1000.0,
        "checkpoint_bytes":std::fs::metadata(&path)?.len(),"validated":true})
    );
    Ok(())
}
