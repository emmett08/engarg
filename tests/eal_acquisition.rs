//! Paired acquisition exports keep raw collection separate from independent review.

use std::{
    collections::BTreeSet,
    io::Write,
    process::{Command, Output, Stdio},
};

use engarg::{
    acquisition::{AcquisitionPlan, ToolArgument},
    domain::{ArgumentCase, Request},
    eal_acquisition::{export_with_plan, plan_from_bundle},
    eal_export::{EalExport, export},
};
use serde_json::{Value, json};

fn fixtures() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}

fn custom_plans(case: &ArgumentCase) -> Vec<AcquisitionPlan> {
    case.evidence_plan
        .iter()
        .map(|need| AcquisitionPlan {
            need_id: need.id.clone(),
            tool_id: "custom".into(),
            arguments: Vec::new(),
            unresolved_arguments: vec!["adapter".into()],
        })
        .collect()
}

fn declaration<'a>(bundle: &'a EalExport, id: &str, role: &str) -> &'a str {
    &bundle.source_map[id]
        .declarations
        .iter()
        .find(|entry| entry.role == role)
        .unwrap_or_else(|| panic!("missing {role} for {id}"))
        .name
}

fn application<'a>(bundle: &'a EalExport, id: &str, role: &str) -> &'a str {
    let declaration = declaration(bundle, id, role);
    bundle
        .source
        .lines()
        .find(|line| {
            line.starts_with(&format!("argument {declaration} = "))
                || line.starts_with(&format!("objection {declaration} = "))
        })
        .unwrap()
}

#[test]
fn every_need_has_a_fail_closed_stub_without_changing_bundle_fields() {
    let (request, case) = fixtures();
    let legacy = export(&request, &case).unwrap();
    let artifacts = export_with_plan(&request, &case, &[]).unwrap();
    let bundle = &artifacts.bundle;
    let legacy_keys: BTreeSet<_> = serde_json::to_value(&legacy)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let keys: BTreeSet<_> = serde_json::to_value(bundle)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, legacy_keys);
    assert_eq!(bundle.obligations.len(), legacy.obligations.len());
    assert_eq!(
        artifacts.tools_toml.matches("kind = \"command\"").count(),
        case.evidence_plan.len() + 1
    );
    assert!(!artifacts.tools_toml.contains("kind = \"json_file\""));
    assert!(
        artifacts
            .tools_toml
            .contains("/REPLACE_WITH_CUSTOM_COLLECTOR")
    );
    assert!(!bundle.source.contains("\"collected\":true"));
    assert!(!bundle.source.contains("\"accepted\":true"));
    assert_eq!(
        bundle.source.matches("require collected == true").count(),
        case.evidence_plan.len()
    );
    for need in &case.evidence_plan {
        let tool = declaration(bundle, &need.id, "planned_acquisition_tool");
        assert!(artifacts.tools_toml.contains(&format!("[tools.{tool}]")));
        assert!(bundle.source.contains(&need.question));
        assert!(bundle.source.contains(&need.acceptance_criterion));
    }
    assert_eq!(bundle.context.as_object().unwrap().len(), 1);
    assert!(plan_from_bundle(&legacy).unwrap().is_none());
    let plans = plan_from_bundle(bundle).unwrap().unwrap();
    let reproduced = export_with_plan(&request, &case, &plans).unwrap();
    assert_eq!(
        serde_json::to_value(bundle).unwrap(),
        serde_json::to_value(reproduced.bundle).unwrap()
    );
    assert_eq!(artifacts.tools_toml, reproduced.tools_toml);
}

#[test]
fn resolved_known_inputs_use_fixed_adapters_and_unknown_inputs_remain_stubs() {
    let (request, case) = fixtures();
    let mut plans = custom_plans(&case);
    plans[0] = AcquisitionPlan {
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: "kubectl_get".into(),
        arguments: vec![
            ToolArgument {
                name: "context".into(),
                value: "prod-a".into(),
            },
            ToolArgument {
                name: "namespace".into(),
                value: "platform".into(),
            },
            ToolArgument {
                name: "resource".into(),
                value: "applications.argoproj.io".into(),
            },
            ToolArgument {
                name: "name".into(),
                value: "pilot-sync-operator".into(),
            },
        ],
        unresolved_arguments: Vec::new(),
    };
    let ready = export_with_plan(&request, &case, &plans).unwrap();
    assert!(
        ready
            .registry
            .tools
            .values()
            .any(|binding| binding.argv == ["engarg", "eal", "observe", "--tool", "kubectl_get"])
    );
    assert!(ready.tools_toml.contains("timeout_seconds = 65"));
    assert!(ready.tools_toml.contains("max_output_bytes = 4194304"));
    assert!(ready.bundle.source.contains(
        "\"arguments\":{\"context\":\"prod-a\",\"name\":\"pilot-sync-operator\",\"namespace\":\"platform\",\"resource\":\"applications.argoproj.io\"}"
    ));
    plans[0]
        .arguments
        .retain(|argument| argument.name != "context");
    plans[0].unresolved_arguments.push("context".into());
    let unresolved = export_with_plan(&request, &case, &plans).unwrap();
    assert!(
        unresolved
            .registry
            .tools
            .values()
            .any(|binding| binding.argv == ["engarg", "eal", "observe", "--tool", "kubectl_get"])
    );
    assert!(!unresolved.tools_toml.contains("kind = \"json_file\""));
    let restored = plan_from_bundle(&unresolved.bundle).unwrap().unwrap();
    assert!(
        restored[0]
            .unresolved_arguments
            .contains(&"context".to_owned())
    );
}

#[test]
fn acquisitions_follow_targets_and_preserve_the_independent_review_gates() {
    let (request, case) = fixtures();
    let bundle = export_with_plan(&request, &case, &[]).unwrap().bundle;
    for need in &case.evidence_plan {
        let observation = declaration(&bundle, &need.id, "planned_acquisition");
        let mapping = &bundle.source_map[&need.target_id];
        let role = if mapping
            .declarations
            .iter()
            .any(|entry| entry.role == "ground_support")
        {
            "ground_support"
        } else if mapping
            .declarations
            .iter()
            .any(|entry| entry.role == "assumption_condition_support")
        {
            "assumption_condition_support"
        } else {
            "evidence_gated_challenge"
        };
        let line = application(&bundle, &need.target_id, role);
        assert!(
            line.contains(observation),
            "{} missing from {line}",
            need.id
        );
        let review = bundle
            .obligations
            .iter()
            .find(|review| review.target_id == need.target_id)
            .unwrap();
        assert!(
            line.contains(&review.eal_evidence_name),
            "review removed from {line}"
        );
    }
    assert_eq!(
        bundle.source.matches("require accepted == true").count(),
        bundle.obligations.len()
    );
    assert!(bundle.source.contains("\"required_acquisitions\":[{"));
}

#[test]
fn binding_changes_change_revision_and_argument_order_has_no_effect() {
    let (request, case) = fixtures();
    let mut plans = custom_plans(&case);
    plans[0].tool_id = "http_json_get".into();
    plans[0].arguments = vec![ToolArgument {
        name: "url".into(),
        value: "https://observations.example/report".into(),
    }];
    plans[0].unresolved_arguments.clear();
    let first = export_with_plan(&request, &case, &plans).unwrap();
    plans.reverse();
    let reordered = export_with_plan(&request, &case, &plans).unwrap();
    assert_eq!(first.bundle.revision, reordered.bundle.revision);
    assert_eq!(first.bundle.source, reordered.bundle.source);
    assert!(first.tools_toml.contains("ENGARG_COLLECTOR_ORIGIN"));
    let selected = plans
        .iter_mut()
        .find(|plan| plan.tool_id == "http_json_get")
        .unwrap();
    selected.arguments[0].value = "https://observations.example/other".into();
    let changed = export_with_plan(&request, &case, &plans).unwrap();
    assert_ne!(first.bundle.revision, changed.bundle.revision);
    assert_ne!(first.bundle.context, changed.bundle.context);
}

#[test]
fn invalid_or_partial_explicit_bindings_are_rejected() {
    let (request, case) = fixtures();
    let mut plans = custom_plans(&case);
    plans.pop();
    assert!(export_with_plan(&request, &case, &plans).is_err());
    plans = custom_plans(&case);
    plans[0].tool_id = "shell".into();
    assert!(export_with_plan(&request, &case, &plans).is_err());
}

#[test]
fn malformed_or_duplicate_archive_markers_are_rejected() {
    let (request, case) = fixtures();
    let mut bundle = export_with_plan(&request, &case, &[]).unwrap().bundle;
    bundle.source.push_str("\n// engarg_acquisition_plan []\n");
    assert!(plan_from_bundle(&bundle).is_err());
    bundle.source = "language \"EAL/3\"\n// engarg_acquisition_plan bad-json\n".into();
    assert!(plan_from_bundle(&bundle).is_err());
    bundle.source = "language \"EAL/3\"\n// engarg_acquisition_plan[]\n".into();
    assert!(plan_from_bundle(&bundle).is_err());
}

#[test]
fn large_plans_do_not_enlarge_observation_context() {
    let (request, case) = fixtures();
    let mut plans = custom_plans(&case);
    plans[0].tool_id = "http_json_get".into();
    plans[0].arguments = vec![ToolArgument {
        name: "url".into(),
        value: format!(
            "https://observations.example/report?query={}",
            "x".repeat(20_000)
        ),
    }];
    plans[0].unresolved_arguments.clear();
    let bundle = export_with_plan(&request, &case, &plans).unwrap().bundle;
    let retained = plan_from_bundle(&bundle).unwrap().unwrap();
    assert!(serde_json::to_vec(&retained).unwrap().len() > 16_384);
    assert!(serde_json::to_vec(&bundle.context).unwrap().len() < 128);
    assert_eq!(bundle.context.as_object().unwrap().len(), 1);
}

fn host_call(
    binary: &std::ffi::OsStr,
    workspace: &std::path::Path,
    request: Value,
) -> (Output, Value) {
    let mut child = Command::new(binary)
        .arg("--workspace")
        .arg(workspace)
        .arg("--registry")
        .arg(workspace.join("tools.toml"))
        .arg("--timeout")
        .arg("15")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "host JSON failed {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (output, value)
}

/// Exercise the authoritative native registry parser and source validator.
/// Set ENGARG_EAL_HOST to the actual eal-host with its sibling server and companions.
#[test]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
fn upstream_refuses_prepared_stubs_and_missing_bindings_supply_no_evidence() {
    let binary =
        std::env::var_os("ENGARG_EAL_HOST").expect("compatibility test requires ENGARG_EAL_HOST");
    let (request, case) = fixtures();
    let artifacts = export_with_plan(&request, &case, &[]).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("tools.toml"), &artifacts.tools_toml).unwrap();
    let bundle = artifacts.bundle;
    let (rejected, diagnostic) = host_call(
        &binary,
        workspace.path(),
        json!({"operation":"validate","source":bundle.source}),
    );
    assert!(
        !rejected.status.success(),
        "prepared selections must be refused before execution: {diagnostic}"
    );
    std::fs::write(workspace.path().join("tools.toml"), "[tools]\n").unwrap();
    let (output, validation) = host_call(
        &binary,
        workspace.path(),
        json!({
            "operation":"validate", "source":bundle.source,
        }),
    );
    assert!(output.status.success(), "{validation}");
    assert_eq!(validation["result"]["valid"], true, "{validation}");
    let evidence = declaration(&bundle, &case.evidence_plan[0].id, "planned_acquisition");
    let (output, collected) = host_call(
        &binary,
        workspace.path(),
        json!({
            "operation":"collect", "source":bundle.source, "context":bundle.context,
            "evidence_ids":[evidence],
        }),
    );
    assert!(output.status.success(), "{collected}");
    assert_eq!(
        collected["result"]["records"][evidence]["status"], "error",
        "{collected}"
    );
    let (output, assessment) = host_call(
        &binary,
        workspace.path(),
        json!({
            "operation":"reason", "source":bundle.source, "context":bundle.context,
            "collection_id":collected["result"]["collection_id"],
        }),
    );
    assert!(output.status.success(), "{assessment}");
    assert_eq!(
        assessment["result"]["claims"][&bundle.claim_names["current_claim"]]["status"],
        "unsupported",
        "{assessment}"
    );
    assert_eq!(
        assessment["result"]["evidence"][evidence]["status"], "unavailable",
        "{assessment}"
    );

    // Synthetic test data establishes successful acquisition only. It never
    // enters an exported production case or discharges an independent review.
    let tool = declaration(
        &bundle,
        &case.evidence_plan[0].id,
        "planned_acquisition_tool",
    );
    let block = bundle
        .source
        .split(&format!("evidence {evidence} {{"))
        .nth(1)
        .unwrap();
    let input: Value = serde_json::from_str(
        block
            .lines()
            .find_map(|line| line.strip_prefix("  input "))
            .unwrap(),
    )
    .unwrap();
    let imported = json!({
        "value":{"collected":true,"observation":{"fixture":"synthetic regression"}},
        "observed_at":collected["result"]["records"][evidence]["collected_at"],
        "context":bundle.context,
        "request":{"tool":tool,"tool_version":"1","input":input,"context":bundle.context},
    });
    std::fs::create_dir(workspace.path().join("observations")).unwrap();
    std::fs::write(
        workspace
            .path()
            .join(format!("observations/{evidence}.json")),
        serde_json::to_vec(&imported).unwrap(),
    )
    .unwrap();
    // Explicit operator substitution with a file collector is supported by EAL.
    let registry = format!(
        "[tools.{tool}]\nkind = \"json_file\"\nversion = \"1\"\npath = \"observations/{evidence}.json\"\n"
    );
    std::fs::write(workspace.path().join("tools.toml"), registry).unwrap();
    let (output, collected) = host_call(
        &binary,
        workspace.path(),
        json!({
            "operation":"collect", "source":bundle.source, "context":bundle.context,
            "evidence_ids":[evidence],
        }),
    );
    assert!(output.status.success(), "{collected}");
    assert_eq!(
        collected["result"]["records"][evidence]["status"], "ok",
        "{collected}"
    );
    let (output, assessment) = host_call(
        &binary,
        workspace.path(),
        json!({
            "operation":"reason", "source":bundle.source, "context":bundle.context,
            "collection_id":collected["result"]["collection_id"],
        }),
    );
    assert!(output.status.success(), "{assessment}");
    assert_eq!(
        assessment["result"]["evidence"][evidence]["status"], "available",
        "{assessment}"
    );
    assert_eq!(
        assessment["result"]["claims"][&bundle.claim_names["current_claim"]]["status"],
        "unsupported",
        "{assessment}"
    );

    // The real native registry accepts every known adapter binding without
    // invoking its proposed Kubernetes or network operation.
    for (tool_id, arguments) in [
        (
            "kubectl_get",
            vec![
                ("context", "prod-a"),
                ("namespace", "platform"),
                ("resource", "applications.argoproj.io"),
                ("name", "pilot-sync-operator"),
            ],
        ),
        (
            "gh_api_get",
            vec![("endpoint", "repos/example/project/actions/runs")],
        ),
        (
            "http_json_get",
            vec![("url", "https://observations.example/report")],
        ),
    ] {
        let mut plans = custom_plans(&case);
        plans[0] = AcquisitionPlan {
            need_id: case.evidence_plan[0].id.clone(),
            tool_id: tool_id.into(),
            arguments: arguments
                .into_iter()
                .map(|(name, value)| ToolArgument {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
            unresolved_arguments: Vec::new(),
        };
        let artifacts = export_with_plan(&request, &case, &plans).unwrap();
        let tool = declaration(
            &artifacts.bundle,
            &case.evidence_plan[0].id,
            "planned_acquisition_tool",
        )
        .to_owned();
        let registry = eal_collector_contracts::CommandRegistry {
            tools: std::collections::BTreeMap::from([(
                tool.clone(),
                artifacts.registry.tools[&tool].clone(),
            )]),
        };
        std::fs::write(
            workspace.path().join("tools.toml"),
            registry.to_toml().unwrap(),
        )
        .unwrap();
        let (output, validation) = host_call(
            &binary,
            workspace.path(),
            json!({
                "operation":"validate", "source":artifacts.bundle.source,
            }),
        );
        assert!(output.status.success(), "{tool_id}: {validation}");
        assert_eq!(
            validation["result"]["valid"], true,
            "{tool_id}: {validation}"
        );
    }
}

#[test]
fn compatibility_binding_metadata_comes_from_the_adapter_owner() {
    use engarg::{
        acquisition::{AcquisitionPlan, override_plans},
        eal_acquisition::export_with_plan,
    };
    let request: engarg::domain::Request =
        serde_json::from_str(include_str!("../examples/request.json")).unwrap();
    let case: engarg::domain::ArgumentCase =
        serde_json::from_str(include_str!("../examples/case.json")).unwrap();
    for profile in eal_adapters::profiles::ALL {
        let plans = override_plans(
            &case,
            &[],
            vec![AcquisitionPlan {
                need_id: case.evidence_plan[0].id.clone(),
                tool_id: profile.id.into(),
                arguments: Vec::new(),
                unresolved_arguments: profile
                    .arguments
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            }],
        )
        .unwrap();
        let exported = export_with_plan(&request, &case, &plans).unwrap();
        let tool = &exported.bundle.source_map[&case.evidence_plan[0].id]
            .declarations
            .iter()
            .find(|declaration| declaration.kind == "tool")
            .unwrap()
            .name;
        let binding = &exported.registry.tools[tool];
        assert_eq!(binding.version, profile.version);
        assert_eq!(binding.inherit_env, profile.inherit_env);
        assert_eq!(binding.timeout_seconds, profile.timeout_seconds);
        assert_eq!(binding.max_output_bytes, profile.max_output_bytes);
        assert_eq!(binding.parallel_safe, profile.parallel_safe);
    }
    let defaults = engarg::acquisition::catalogue();
    let ids: Vec<_> = defaults
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["id"].as_str().unwrap())
        .collect();
    let expected: Vec<_> = eal_adapters::profiles::ALL
        .iter()
        .map(|profile| profile.id)
        .chain(std::iter::once("custom"))
        .collect();
    assert_eq!(ids, expected);
}
