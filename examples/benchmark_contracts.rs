//! Measure complete generated-tool contract admission without models or execution.
use engarg::{
    acquisition::ToolArgument,
    domain::ArgumentCase,
    tooling::{
        contracts::{
            CONTRACT_SCHEMA, CodeCandidate, ContractDraft, ContractFixture, FixtureKind,
            NeedContract, SourceFile, digest_json, validate_contract,
        },
        runner::{ExecutionMode, FixtureRunner, RunOutcome, RunnerIdentity},
        verification::verify,
    },
};
use serde_json::{Map, Value, json};
use std::{hint::black_box, path::Path, time::Instant};

fn contract(width: usize, fixture_count: usize) -> NeedContract {
    let case: ArgumentCase = serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    let parameters = (0..width)
        .map(|index| format!("sample_{index:03}"))
        .collect::<Vec<_>>();
    let properties = parameters
        .iter()
        .map(|name| {
            (
                name.clone(),
                json!({"type":"integer","minimum":0,"maximum":1000}),
            )
        })
        .collect::<Map<_, _>>();
    let input = parameters
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), json!(index)))
        .collect::<Map<_, _>>();
    let fixtures = (0..fixture_count)
        .map(|index| {
            let kind = if index == fixture_count - 1 {
                FixtureKind::AcquisitionError
            } else if index % 2 == 0 {
                FixtureKind::Positive
            } else {
                FixtureKind::Negative
            };
            let mut fixture_input = input.clone();
            fixture_input.insert("sample_000".into(), json!(index));
            ContractFixture {
                name: format!("fixture_{index:03}"),
                kind,
                input: if kind == FixtureKind::AcquisitionError {
                    json!({})
                } else {
                    Value::Object(fixture_input)
                },
                expected_value: match kind {
                    FixtureKind::Positive => Some(json!({"collected":true,"present":true})),
                    FixtureKind::Negative => Some(json!({"collected":true,"present":false})),
                    FixtureKind::AcquisitionError => None,
                },
                files: vec![],
            }
        })
        .collect();
    NeedContract {
        schema: CONTRACT_SCHEMA.into(),
        language: "python".into(),
        need: case.evidence_plan[0].clone(),
        request_context: "{}".into(),
        toolchain_description: "Synthetic operator-selected interpreter".into(),
        draft: ContractDraft {
            purpose:
                "Evaluate parity of sample_000 under bounded synthetic instrument configuration"
                    .into(),
            parameters: parameters.clone(),
            input_schema: json!({"type":"object","additionalProperties":false,
                "properties":properties,"required":parameters}),
            result_schema: json!({"type":"object","additionalProperties":false,
                "properties":{"collected":{"type":"boolean"},"present":{"type":"boolean"}},
                "required":["collected","present"]}),
            arguments: parameters
                .iter()
                .enumerate()
                .map(|(index, name)| ToolArgument {
                    name: name.clone(),
                    value: index.to_string(),
                })
                .collect(),
            unresolved_arguments: vec![],
            fixtures,
        },
    }
}

fn rejected_variants(original: &NeedContract) -> Vec<String> {
    (0..4)
        .map(|variant| {
            let mut rejected = original.clone();
            match variant {
                0 => rejected.draft.input_schema["additionalProperties"] = json!(true),
                1 => {
                    rejected.draft.result_schema["properties"]["collected"]["type"] =
                        json!("invalid-type")
                }
                2 => rejected.draft.input_schema["$ref"] = json!("https://invalid.example/schema"),
                _ => {
                    rejected.draft.fixtures[0].expected_value =
                        Some(json!({"collected":true,"present":"invalid-boolean"}))
                }
            }
            validate_contract(&rejected)
                .expect_err("adverse contract must remain rejected")
                .to_string()
        })
        .collect()
}

struct SyntheticRunner<'a> {
    contract: &'a NeedContract,
    identity: RunnerIdentity,
}

impl FixtureRunner for SyntheticRunner<'_> {
    fn identity(&self) -> &RunnerIdentity {
        &self.identity
    }

    fn execute(
        &self,
        _: &Path,
        _: &[SourceFile],
        _: &[SourceFile],
        request_bytes: &[u8],
        _: ExecutionMode,
        _: &eal_process::CancellationToken,
    ) -> Result<RunOutcome, engarg::Error> {
        let failed = || RunOutcome {
            stdout: vec![],
            stderr: vec![],
            returncode: 2,
            execution_error: None,
            elapsed_ms: 0,
        };
        let parsed = eal_collector_contracts::strict_json_exact(request_bytes, Default::default())
            .ok()
            .and_then(|value| {
                serde_json::from_value::<eal_collector_contracts::AcquisitionRequest>(value).ok()
            });
        let Some(request) = parsed.filter(|request| request.validate().is_ok()) else {
            return Ok(failed());
        };
        if request
            .input
            .as_object()
            .is_none_or(|input| input.len() != self.contract.draft.parameters.len())
        {
            return Ok(failed());
        }
        let Some(sample) = request.input["sample_000"]
            .as_u64()
            .filter(|sample| *sample <= 1000)
        else {
            return Ok(failed());
        };
        let value = json!({"collected":true,"present":sample.is_multiple_of(2)});
        Ok(RunOutcome {
            stdout: serde_json::to_vec(
                &json!({"value":value,"context":request.context,"request":request.acquisition()}),
            )?,
            stderr: vec![],
            returncode: 0,
            execution_error: None,
            elapsed_ms: 0,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 4 || !matches!(arguments[0].as_str(), "admit" | "verify") {
        return Err(
            "usage: benchmark_contracts admit|verify ITERATIONS PARAMETERS FIXTURES".into(),
        );
    }
    let mode = &arguments[0];
    let iterations: usize = arguments[1].parse()?;
    let width: usize = arguments[2].parse()?;
    let fixtures: usize = arguments[3].parse()?;
    if !(1..=1000).contains(&iterations)
        || !(1..=64).contains(&width)
        || !(3..=64).contains(&fixtures)
    {
        return Err("requires 1..1000 iterations, 1..64 parameters and 3..64 fixtures".into());
    }
    let contract = contract(width, fixtures);
    validate_contract(&contract)?;
    let rejection_messages = rejected_variants(&contract);
    let runner = SyntheticRunner {
        contract: &contract,
        identity: RunnerIdentity {
            profile_sha256: "0".repeat(64),
            image_id: format!("sha256:{}", "0".repeat(64)),
            docker_sha256: "1".repeat(64),
            runner_sha256: "2".repeat(64),
            supervisor_sha256: "3".repeat(64),
        },
    };
    let candidate = CodeCandidate {
        language: "python".into(),
        files: vec![SourceFile {
            path: "collector.py".into(),
            content: "# Synthetic verification source".into(),
        }],
    };
    let directory = tempfile::tempdir()?;
    let cancellation = eal_process::CancellationToken::default();
    let mut last_receipt = None;
    let started = Instant::now();
    for iteration in 0..iterations {
        if mode == "admit" {
            validate_contract(black_box(&contract))?;
        } else {
            let attempt = directory.path().join(iteration.to_string());
            std::fs::create_dir(&attempt)?;
            let receipt = verify(
                &runner,
                black_box(&contract),
                &candidate,
                &attempt,
                &cancellation,
            )?;
            assert!(receipt.passed, "complete fixture verification must pass");
            last_receipt = Some(receipt);
        }
    }
    let elapsed_ns = started.elapsed().as_nanos();
    let peak_rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        });
    println!(
        "{}",
        json!({
            "schema":"engarg.contract-admission-profile/1",
            "workload":mode,
            "scope":if mode == "admit" {
                "Complete validate_contract boundary, including schema admission, operator input and frozen fixture checks; excludes fixture construction, processes, providers and execution"
            } else {
                "Complete verify boundary with deterministic in-process fixture runner, protocol rejection checks and durable receipt; excludes fixture construction, real subprocesses, containers and providers"
            },
            "iterations":iterations,"parameters":width,"fixtures":fixtures,
            "elapsed_ns":elapsed_ns,"contract_sha256":digest_json(&contract)?,
        "peak_rss_kib":peak_rss_kib,
            "rejected_variants":rejection_messages,
            "receipt_sha256":last_receipt.as_ref().map(digest_json).transpose()?,
            "validated":true,
        })
    );
    Ok(())
}
