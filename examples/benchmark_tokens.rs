//! Whole-CLI fixed-output token-volume benchmark. No paid inference is performed.
//!
//! Run `cargo run --locked --release --example benchmark_tokens -- --help`.

#[path = "benchmark_support/replay.rs"]
mod replay;

use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use tiktoken_rs::CoreBPE;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Parser)]
#[command(about = "Replay complete CLI runs and count named reference text tokens")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    Capture(Capture),
    Compare {
        baseline: PathBuf,
        candidate: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Document {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Args)]
struct Capture {
    #[arg(long)]
    binary: PathBuf,
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    #[arg(long)]
    label: String,
    #[arg(long, default_value = "unknown")]
    build_profile: String,
    #[arg(long)]
    output: PathBuf,
    /// Private retained RunReport. Only digests and volume metrics are exported.
    #[arg(long)]
    replay_report: Option<PathBuf>,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u16).range(1..))]
    repetitions: u16,
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "claim_only,supplied,objections,repair"
    )]
    workloads: Vec<String>,
    /// Measure optional tool planning separately from the legacy case contract.
    #[arg(long)]
    tool_planning: bool,
    /// Use when the executable predates the --no-tool-planning option.
    #[arg(long)]
    legacy_binary: bool,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn json_digest(value: &Value) -> String {
    digest(serde_json::to_vec(value).expect("JSON value serialises"))
}

fn write_result(path: &Path, value: &Value) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{}", serde_json::to_string_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err("git metadata command failed".into());
    }
    Ok(output.stdout)
}

fn source_identity(repo: &Path) -> Result<Value> {
    fn visit(repo: &Path, path: &Path, files: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(repo, &path, files)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.insert(
                    path.strip_prefix(repo)?.to_string_lossy().into(),
                    digest(fs::read(path)?),
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(repo, &repo.join("src"), &mut files)?;
    for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        files.insert(name.into(), digest(fs::read(repo.join(name))?));
    }
    Ok(serde_json::to_value(files)?)
}

fn tokenizer_identity() -> Value {
    json!({"package":"tiktoken-rs", "package_version":"0.12.1", "encoding":"o200k_base", "scope":"Named reference encoding of recorded text; model-specific framing, caching, hidden reasoning and billing are excluded."})
}

fn measure(text: &str, encoding: &CoreBPE) -> Value {
    json!({"utf8_bytes":text.len(), "reference_tokens":encoding.encode_ordinary(text).len()})
}

/// Reconstruct the expanded representation independently of engine grouping.
/// The corresponding system paragraph is identified by its fixed section label.
fn expand_target_contexts(system: &str, user: &str) -> Result<(String, String)> {
    let mut data: Value = serde_json::from_str(user)?;
    let Some(table) = data
        .as_object_mut()
        .and_then(|data| data.remove("retained_target_contexts"))
    else {
        return Ok((system.into(), user.into()));
    };
    for objection in data["retained_objections"]
        .as_array_mut()
        .ok_or("retained objections absent")?
    {
        let Some(identity) = objection.get_mut("identity").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(reference) = identity.remove("target_context_ref") {
            let context = table
                .get(reference.as_str().ok_or("invalid context reference")?)
                .ok_or("unresolved context reference")?;
            identity.insert("target_context".into(), context.clone());
        }
    }
    let marker = "\n\nTARGET CONTEXT REFERENCES: ";
    let start = system
        .find(marker)
        .ok_or("target context instruction absent")?;
    let end = system[start + marker.len()..]
        .find("\n\n")
        .map_or(system.len(), |offset| start + marker.len() + offset);
    let mut expanded_system = system.to_owned();
    expanded_system.replace_range(start..end, "");
    Ok((expanded_system, serde_json::to_string(&data)?))
}

#[cfg(test)]
mod context_measurement_tests {
    use super::*;

    #[test]
    fn expansion_preserves_scope_target_future_fields_and_following_instructions() {
        let number: Value = serde_json::from_str("1.234567890123456789e-1000").unwrap();
        let context = json!({"scope":{"configuration":"Frozen exact scope."},
            "target":{"rule":"Exact warrant."},"related_targets":{"G1":{"statement":"Exact premise."}},"future":number});
        let compact = json!({"request":{"retained_target_contexts":"literal untrusted data"},
            "retained_target_contexts":{"tc1":context},
            "retained_objections":[{"identity":{"target_id":"W1","target_context_ref":"tc1"}}]});
        let system = "Core\n\nTARGET CONTEXT REFERENCES: Resolve exact contexts.\n\nCONSTRUCTION LIMIT: Two layers.";
        let (system, user) =
            expand_target_contexts(system, &serde_json::to_string(&compact).unwrap()).unwrap();
        assert_eq!(system, "Core\n\nCONSTRUCTION LIMIT: Two layers.");
        let expanded: Value = serde_json::from_str(&user).unwrap();
        assert_eq!(
            expanded["retained_objections"][0]["identity"]["target_context"],
            context
        );
        assert_eq!(expanded["request"], compact["request"]);
        assert!(expanded.get("retained_target_contexts").is_none());
    }

    #[test]
    fn an_uncompacted_prompt_is_measured_without_reformatting() {
        let user = "{\n  \"request\": {\"retained_target_contexts\": \"source\"}\n}";
        assert_eq!(
            expand_target_contexts("Core", user).unwrap(),
            ("Core".into(), user.into())
        );
    }

    #[test]
    fn missing_authoritative_context_is_a_measurement_error() {
        let user = json!({"retained_target_contexts":{}, "retained_objections":[{"identity":{"target_context_ref":"tc1"}}]});
        assert!(
            expand_target_contexts(
                "Core\n\nTARGET CONTEXT REFERENCES: Resolve exact contexts.",
                &user.to_string()
            )
            .is_err()
        );
    }
}

fn metrics(report: &Value, wire_bytes: usize, encoding: &CoreBPE) -> Result<Value> {
    let mut records = Vec::new();
    let mut totals = BTreeMap::<&str, (usize, usize)>::new();
    let mut expanded_input = (0usize, 0usize);
    for call in report["transcript"]
        .as_array()
        .ok_or("report transcript absent")?
    {
        let schema = serde_json::to_string(&call["prompt"]["schema"])?;
        let parts = [
            (
                "instructions",
                call["prompt"]["system"]
                    .as_str()
                    .ok_or("instructions absent")?,
            ),
            (
                "user",
                call["prompt"]["user"]
                    .as_str()
                    .ok_or("user context absent")?,
            ),
            ("schema", schema.as_str()),
            (
                "output",
                call["response"]["content"]
                    .as_str()
                    .ok_or("response absent")?,
            ),
        ];
        let mut components = serde_json::Map::new();
        for (name, text) in parts {
            let tokens = encoding.encode_ordinary(text).len();
            components.insert(
                name.into(),
                json!({"utf8_bytes":text.len(), "reference_tokens":tokens}),
            );
            let sum = totals.entry(name).or_default();
            sum.0 += text.len();
            sum.1 += tokens;
        }
        let (expanded_system, expanded_user) = expand_target_contexts(parts[0].1, parts[1].1)?;
        let expanded = [expanded_system.as_str(), expanded_user.as_str(), parts[2].1]
            .iter()
            .fold((0, 0), |(bytes, tokens), text| {
                (
                    bytes + text.len(),
                    tokens + encoding.encode_ordinary(text).len(),
                )
            });
        expanded_input.0 += expanded.0;
        expanded_input.1 += expanded.1;
        records.push(json!({"role":call["role"], "components":components, "response_sha256":digest(parts[3].1),"expanded_context_input":{"utf8_bytes":expanded.0,"reference_tokens":expanded.1}}));
    }
    let input = ["instructions", "user", "schema"]
        .iter()
        .fold((0, 0), |sum, key| {
            (sum.0 + totals[key].0, sum.1 + totals[key].1)
        });
    totals.insert("input", input);
    totals.insert(
        "input_plus_output",
        (input.0 + totals["output"].0, input.1 + totals["output"].1),
    );
    let sums: BTreeMap<_, _> = totals
        .into_iter()
        .map(|(key, (bytes, tokens))| (key, json!({"utf8_bytes":bytes, "reference_tokens":tokens})))
        .collect();
    Ok(
        json!({"call_count":records.len(), "argument_sha256":json_digest(&report["argument"]), "request_sha256":json_digest(&report["request"]), "records":records, "totals":sums, "expanded_context_input":{"utf8_bytes":expanded_input.0,"reference_tokens":expanded_input.1}, "wire_request_utf8_bytes":wire_bytes, "provider_usage":{"input_tokens":null,"output_tokens":null,"reason":"No new provider inference. Historical replay usage is excluded from candidate billed-use claims."}}),
    )
}

fn capture(args: Capture, encoding: &CoreBPE) -> Result<()> {
    if args.tool_planning && args.legacy_binary {
        return Err(
            "--tool-planning requires an executable supporting the acquisition envelope".into(),
        );
    }
    let repo = args.repo.canonicalize()?;
    let binary = args.binary.canonicalize()?;
    let mut workloads = args.workloads.clone();
    if args.replay_report.is_some() && !workloads.iter().any(|w| w == "retained_report") {
        workloads.push("retained_report".into());
    }
    let mut results = serde_json::Map::new();
    for workload in workloads {
        let fixture = if workload == "retained_report" {
            replay::Fixture::retained(
                args.replay_report
                    .as_deref()
                    .ok_or("retained_report requires --replay-report")?,
            )?
        } else {
            replay::Fixture::public(&repo, &workload)?
        };
        let mut rows = Vec::new();
        for _ in 0..args.repetitions {
            let run = replay::run(&binary, &fixture, args.tool_planning, args.legacy_binary)?;
            let mut row = metrics(&run.report, run.wire_bytes, encoding)?;
            row["elapsed_seconds"] = json!(run.elapsed);
            row["accepted"] = json!(true);
            fixture.annotate(&mut row);
            rows.push(row);
        }
        results.insert(workload, json!(rows));
    }
    let workload_source: BTreeMap<_, _> = [
        "case.json",
        "claim-only-case.json",
        "request.json",
        "claim-only-request.json",
    ]
    .into_iter()
    .map(|name| Ok((name, digest(fs::read(repo.join("examples").join(name))?))))
    .collect::<Result<_>>()?;
    let data = json!({
        "benchmark":"engarg.fixed-output-context-volume.v1", "label":args.label,
        "build_profile":args.build_profile, "platform":format!("{} {}",std::env::consts::OS,std::env::consts::ARCH),
        "repository_revision":String::from_utf8(git(&repo,&["rev-parse","HEAD"])?)?.trim(),
        "working_tree_diff_sha256":digest(git(&repo,&["diff","--binary","HEAD"])?),
        "source_tree_sha256":json_digest(&source_identity(&repo)?),
        "benchmark_source_sha256":digest([include_bytes!("benchmark_tokens.rs").as_slice(), include_bytes!("benchmark_support/replay.rs").as_slice()].concat()),
        "binary":binary, "binary_sha256":digest(fs::read(&binary)?),
        "tokenizer":tokenizer_identity(), "repetitions":args.repetitions,
        "tool_planning":args.tool_planning, "workload_source":workload_source,
        "limitations":["Fixed outputs verify structural acceptance, retained dissent and the complete call path.","Reference text tokens exclude proprietary framing, caching, hidden reasoning and billing.","Local replay latency does not measure live provider latency or argument quality."],
        "workloads":results
    });
    write_result(&args.output, &data)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"output":args.output,"workloads":data["workloads"].as_object().unwrap().iter().map(|(name,rows)|(name,json!({"calls":rows[0]["call_count"],"input_reference_tokens":rows[0]["totals"]["input"]["reference_tokens"],"all_accepted":true}))).collect::<BTreeMap<_,_>>()})
        )?
    );
    Ok(())
}

fn compare(baseline: &Path, candidate: &Path, output: Option<&Path>) -> Result<()> {
    let base: Value = serde_json::from_slice(&fs::read(baseline)?)?;
    let new: Value = serde_json::from_slice(&fs::read(candidate)?)?;
    // Language implementations may differ while naming the same reference encoding.
    if base["tokenizer"]["encoding"] != new["tokenizer"]["encoding"]
        || base["workload_source"] != new["workload_source"]
    {
        return Err("comparison requires identical encoding and immutable workload source".into());
    }
    let mut workloads = serde_json::Map::new();
    for (name, rows) in base["workloads"]
        .as_object()
        .ok_or("baseline workloads absent")?
    {
        let old = rows.as_array().ok_or("baseline repetitions absent")?;
        let next = new["workloads"][name]
            .as_array()
            .ok_or("candidate workload absent")?;
        if old.len() != next.len() || old.is_empty() {
            return Err("repetition count differs".into());
        }
        for (b, c) in old.iter().zip(next) {
            for key in [
                "argument_sha256",
                "request_sha256",
                "call_count",
                "source_report_sha256",
                "source_response_sequence_sha256",
                "source_run_config_sha256",
            ] {
                if b[key] != c[key] {
                    return Err(format!("{name} changed {key}").into());
                }
            }
            if b["accepted"] != true || c["accepted"] != true {
                return Err("unaccepted replay".into());
            }
            let sequences = |row: &Value| {
                row["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| json!([r["role"], r["response_sha256"]]))
                    .collect::<Vec<_>>()
            };
            if sequences(b) != sequences(c) {
                return Err("frozen completion sequence changed".into());
            }
        }
        let mut components = serde_json::Map::new();
        for (component, units) in old[0]["totals"].as_object().ok_or("totals absent")? {
            let mut changes = serde_json::Map::new();
            for unit in ["utf8_bytes", "reference_tokens"] {
                let b = units[unit].as_f64().ok_or("measurement absent")?;
                let c = next[0]["totals"][component][unit]
                    .as_f64()
                    .ok_or("measurement absent")?;
                if old
                    .iter()
                    .any(|r| r["totals"][component][unit] != units[unit])
                    || next
                        .iter()
                        .any(|r| r["totals"][component][unit] != next[0]["totals"][component][unit])
                {
                    return Err("deterministic replay volume varied".into());
                }
                changes.insert(unit.into(),json!({"baseline":b,"candidate":c,"absolute_reduction":b-c,"percent_reduction":if b==0.0 {None}else{Some(100.0*(b-c)/b)},"baseline_range":[b,b],"candidate_range":[c,c]}));
            }
            components.insert(component.clone(), json!(changes));
        }
        workloads.insert(name.clone(),json!({"calls":old[0]["call_count"],"argument_and_request_identical":true,"components":components}));
    }
    if workloads.len()
        != new["workloads"]
            .as_object()
            .ok_or("candidate workloads absent")?
            .len()
    {
        return Err("workload set differs".into());
    }
    let result = json!({"baseline":base["label"],"candidate":new["label"],"tokenizer":new["tokenizer"],"workloads":workloads});
    if let Some(path) = output {
        write_result(path, &result)?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"output":path,"workloads":result["workloads"].as_object().unwrap().iter().map(|(name,row)|(name,row["components"]["input"]["reference_tokens"].clone())).collect::<BTreeMap<_,_>>()})
            )?
        );
    } else {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}

fn document(binary: &Path, input: &Path, output: &Path, encoding: &CoreBPE) -> Result<()> {
    let original = fs::read(input)?;
    let render = |brief: bool| -> Result<String> {
        let mut command = Command::new(binary);
        command
            .args(["render", "--input"])
            .arg(input)
            .args(["--format", "markdown"]);
        if brief {
            command.arg("--no-audit-appendix");
        }
        let result = command.output()?;
        if !result.status.success() {
            return Err("render failed".into());
        }
        Ok(String::from_utf8(result.stdout)?)
    };
    let full = render(false)?;
    let brief = render(true)?;
    if full
        .rsplit_once("\n## Complete audit record\n\n")
        .ok_or("audit marker absent")?
        .0
        .trim_end()
        != brief.trim_end()
        || fs::read(input)? != original
    {
        return Err("argument content or source changed".into());
    }
    let result = json!({"benchmark":"engarg.audit-appendix-context.v1","binary_sha256":digest(fs::read(binary)?),"source_report_sha256":digest(original),"tokenizer":tokenizer_identity(),"canonical_source_unchanged":true,"all_argument_records_and_registers_identical":true,"full":measure(&full,encoding),"without_audit_appendix":measure(&brief,encoding),"limitation":"Downstream document context only. Construction inference is unchanged."});
    write_result(output, &result)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn main() -> Result<()> {
    let encoding = tiktoken_rs::o200k_base()?;
    match Cli::parse().action {
        Action::Capture(args) => capture(args, &encoding),
        Action::Compare {
            baseline,
            candidate,
            output,
        } => compare(&baseline, &candidate, output.as_deref()),
        Action::Document {
            binary,
            input,
            output,
        } => document(&binary, &input, &output, &encoding),
    }
}
