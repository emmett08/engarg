//! A moved package runs from EAL and TOML; build receipts carry no semantics.
use eal_collector_contracts::execution::EvaluationCriterion;
use engarg::{
    domain::{ArgumentCase, Request},
    export_policy::ExportPolicy,
    package,
    support_requirements::SupportElaboration,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

#[path = "common/executable_case.rs"]
mod executable_case;
use executable_case::fixture;

fn source(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("tools")).unwrap();
    fs::write(
        root.join("tools/source.py"),
        r#"import json,sys
request=json.load(sys.stdin)
assert request['input']['fixture']=='frozen-fixture-v1'
if request['input']['operation']=='premise_source':
    value={'measurement':{'count':len(['a','b','c']),'distinct':len(set(['a','b','c']))==3}}
else:
    value={'applies':True}
print(json.dumps({'value':value,'observed_at':'2026-10-25T12:00:00Z'}))
"#,
    )
    .unwrap();
    let manifest = root.join("tools.toml");
    fs::write(&manifest,"[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\ninherit_env=['PATH']\n").unwrap();
    manifest
}
fn build(tools: &Path, output: &Path) -> Result<Value, engarg::Error> {
    let (request, case, elaboration) = fixture();
    package::build(
        &request,
        &case,
        &elaboration,
        tools,
        output,
        &ExportPolicy::default(),
    )
}

#[test]
fn package_has_executable_source_direct_tools_and_inspection_only_receipt() {
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let output = root.path().join("package");
    let receipt = build(&tools, &output).unwrap();
    let eal = fs::read_to_string(output.join("case.eal")).unwrap();
    assert!(eal.starts_with("language \"EAL/4\""));
    assert!(!eal.contains("engarg_support_graph"));
    assert!(!eal.contains("execution_plan_sha256"));
    assert!(!eal.contains("formal_source"));
    assert!(
        !fs::read_to_string(output.join("tools/source.py"))
            .unwrap()
            .contains("premises")
    );
    assert!(eal.contains("method \"deductive/1\""));
    assert!(eal.contains("bind "));
    assert_eq!(receipt["model_calls"], 0);
    assert!(output.join("tools/source.py").is_file());
    let manifest = eal_collector_contracts::RegistryDocument::from_toml(
        &fs::read_to_string(output.join("tools.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.tools.len(), 2);
    assert!(manifest.context.contains_key("engarg_export_digest"));
    assert!(
        manifest
            .tools
            .keys()
            .all(|name| name.starts_with("source_"))
    );
}

#[test]
fn build_rejects_output_reuse_and_context_substitution_before_writing() {
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let existing = root.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("keep.txt"), "preserve").unwrap();
    assert!(
        build(&tools, &existing)
            .unwrap_err()
            .to_string()
            .contains("new directory")
    );
    assert_eq!(
        fs::read_to_string(existing.join("keep.txt")).unwrap(),
        "preserve"
    );
    let manifest = fs::read_to_string(&tools).unwrap();
    fs::write(
        &tools,
        format!("[context]\nengarg_export_digest='substituted'\n{manifest}"),
    )
    .unwrap();
    let output = root.path().join("rejected");
    assert!(
        build(&tools, &output)
            .unwrap_err()
            .to_string()
            .contains("conflicts")
    );
    assert!(!output.exists());
}

#[test]
fn resources_cannot_replace_package_files_or_escape_the_manifest_root() {
    for path in ["../outside.json", "case.eal", "tools.toml", "build.json"] {
        let root = TempDir::new().unwrap();
        let tools = source(root.path());
        if !path.contains("..") && path != "tools.toml" {
            fs::write(root.path().join(path), "{}").unwrap();
        }
        let manifest = fs::read_to_string(&tools).unwrap();
        fs::write(
            &tools,
            format!("{manifest}\n[tools.retained]\nkind='json_file'\nversion='1'\npath='{path}'\n"),
        )
        .unwrap();
        let output = root.path().join("rejected");
        assert!(build(&tools, &output).is_err(), "accepted {path}");
        assert!(!output.exists());
    }
}

#[cfg(unix)]
#[test]
fn package_refuses_symlink_resources_and_oversized_resource_trees() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let tools = source(root.path());
    fs::write(
        outside.path().join("outside.py"),
        "raise RuntimeError('outside')",
    )
    .unwrap();
    symlink(
        outside.path().join("outside.py"),
        root.path().join("tools/escape.py"),
    )
    .unwrap();
    let output = root.path().join("rejected");
    assert!(
        build(&tools, &output)
            .unwrap_err()
            .to_string()
            .contains("regular files")
    );
    assert!(!output.exists());
    fs::remove_file(root.path().join("tools/escape.py")).unwrap();
    fs::File::create(root.path().join("tools/huge.bin"))
        .unwrap()
        .set_len(256 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        build(&tools, &output)
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    assert!(!output.exists());
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn moved_package_executes_through_mcp_without_authoring_or_semantic_sidecars() {
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let output = root.path().join("package");
    let receipt = build(&tools, &output).unwrap();
    let moved = TempDir::new().unwrap();
    for file in ["case.eal", "tools.toml", "tools/source.py"] {
        let target = moved.path().join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(output.join(file), target).unwrap();
    }
    // Destroy the complete original inputs and build receipt before invoking EAL.
    drop(root);
    assert!(!moved.path().join("build.json").exists());
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let run = Command::new(host)
        .current_dir(moved.path())
        .args([
            "run",
            "case.eal",
            "--tools",
            "tools.toml",
            "--workspace",
            "run",
            "--claim",
            receipt["claims"]["current_claim"].as_str().unwrap(),
            "--now",
            reasoning::NOW,
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let result: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["is_error"], false, "{result}");
    assert_eq!(
        result["result"]["assessment"]["claims"]
            [receipt["claims"]["current_claim"].as_str().unwrap()]["status"],
        "supported",
        "{result}"
    );
}

#[test]
fn a_late_pin_failure_removes_the_private_stage_without_publishing_a_package() {
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let text = fs::read_to_string(&tools).unwrap().replace(
        "inherit_env=['PATH']",
        &format!(
            "inherit_env=['PATH']\npinned_files=[{{path='tools/source.py',sha256='{}'}}]",
            "0".repeat(64)
        ),
    );
    fs::write(&tools, text).unwrap();
    let output = root.path().join("rejected");
    let error = build(&tools, &output).unwrap_err().to_string();
    assert!(error.contains("SHA-256 pin"), "{error}");
    assert!(!output.exists());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".engarg-package-")
    }));
}

#[cfg(unix)]
#[test]
fn packaged_implementations_preserve_executable_permission() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    fs::set_permissions(
        root.path().join("tools/source.py"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let output = root.path().join("package");
    build(&tools, &output).unwrap();
    assert_eq!(
        output
            .join("tools/source.py")
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0o111
    );
}

fn batch_fixture(variant: &str) -> (Request, ArgumentCase, SupportElaboration) {
    let (request, case, mut elaboration) = fixture();
    let mut batch = elaboration.acquisitions[0].clone();
    batch.id = "batch".into();
    for (name, unit) in [
        ("applies", "boolean"),
        ("returned_ids", "json"),
        ("complete", "boolean"),
    ] {
        batch
            .contract
            .quantities
            .push(serde_json::from_value(json!({"name":name,"unit":unit})).unwrap());
    }
    batch.arguments =
        json!({"fixture":"frozen-fixture-v1","requested":["a","b","c"],"variant":variant});
    elaboration.acquisitions = vec![batch];
    for evaluation in &mut elaboration.evaluations {
        for input in &mut evaluation.inputs {
            input.step_id = "batch".into();
        }
    }
    let ground = elaboration
        .requirements
        .iter()
        .find(|r| r.target_id == "G1")
        .unwrap()
        .clone();
    let ground_evaluation = elaboration
        .evaluations
        .iter()
        .find(|e| e.requirement_id == ground.id)
        .unwrap()
        .clone();
    for (id, pointer, criterion) in [
        (
            "requested_ids",
            "/returned_ids",
            EvaluationCriterion::Equals {
                expected: json!(["a", "b", "c"]),
            },
        ),
        ("untruncated", "/complete", EvaluationCriterion::AllTrue),
    ] {
        let mut requirement = ground.clone();
        requirement.id = id.into();
        requirement.evidence_need_ids.clear();
        requirement.original_acceptance_criteria.clear();
        requirement.acceptance = criterion.clone();
        requirement.contract.as_mut().unwrap().quantities = vec![serde_json::from_value(json!({"name":pointer.trim_start_matches('/'),"unit":if id=="requested_ids" {"json"} else {"boolean"}})).unwrap()];
        requirement.acceptance_rationale="Check this finite requested set explicitly; no population completeness follows from a provider assertion.".into();
        let mut evaluation = ground_evaluation.clone();
        evaluation.id = format!("check_{id}");
        evaluation.requirement_id = id.into();
        evaluation.criterion = criterion;
        evaluation.inputs =
            vec![serde_json::from_value(json!({"step_id":"batch","pointer":pointer})).unwrap()];
        elaboration.requirements.push(requirement);
        elaboration.evaluations.push(evaluation);
    }
    (request, case, elaboration)
}
fn batch_source(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("tools")).unwrap();
    fs::write(root.join("tools/batch.py"),r#"import json,sys
request=json.load(sys.stdin)['input']
assert request['fixture']=='frozen-fixture-v1'
assert request['requested']==['a','b','c']
with open('calls.jsonl','a') as log:
    log.write(json.dumps(request)+'\n')
variant=request['variant']
ids=list(request['requested'])
if variant=='omitted': ids=ids[:-1]
if variant=='wrong_ids': ids=['a','b','z']
if variant=='duplicates': ids=['a','a','c']
value={'measurement':{'count':len(ids),'distinct':len(set(ids))==len(ids)},'returned_ids':ids,'complete':variant!='truncated','applies':True}
if variant=='unavailable': value.pop('measurement')
print(json.dumps({'value':value,'observed_at':'2026-10-25T12:00:00Z'}))
"#).unwrap();
    let tools = root.join("tools.toml");
    fs::write(&tools,"[tools.batch]\nkind='command'\nversion='1'\nargv=['python3','tools/batch.py']\ninherit_env=['PATH']\n").unwrap();
    tools
}

#[test]
fn batch_package_declares_one_explicit_operation_with_shared_finite_set_checks() {
    let root = TempDir::new().unwrap();
    let tools = batch_source(root.path());
    let (request, case, elaboration) = batch_fixture("complete");
    let output = root.path().join("package");
    package::build(
        &request,
        &case,
        &elaboration,
        &tools,
        &output,
        &ExportPolicy::default(),
    )
    .unwrap();
    let text = fs::read_to_string(output.join("case.eal")).unwrap();
    let program =
        eal_core::composition::compile(&text, None, "", eal_core::ExecutionLimits::default())
            .unwrap();
    assert_eq!(
        program
            .evidence
            .values()
            .filter(|e| e.tool.as_deref().is_some_and(|t| t.starts_with("source_")))
            .count(),
        1
    );
    assert!(text.contains("returned_ids"));
    assert!(text.contains("complete"));
    assert!(!output.join("execution-plan.json").exists());
}

fn run_batch(variant: &str) -> Value {
    let root = TempDir::new().unwrap();
    let tools = batch_source(root.path());
    let (request, case, elaboration) = batch_fixture(variant);
    let output = root.path().join("package");
    let receipt = package::build(
        &request,
        &case,
        &elaboration,
        &tools,
        &output,
        &ExportPolicy::default(),
    )
    .unwrap();
    fs::remove_file(output.join("build.json")).unwrap();
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("set ENGARG_EAL_HOST"));
    let run = Command::new(host)
        .current_dir(&output)
        .args([
            "run",
            "case.eal",
            "--tools",
            "tools.toml",
            "--workspace",
            "run",
            "--claim",
            receipt["claims"]["current_claim"].as_str().unwrap(),
            "--now",
            reasoning::NOW,
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    let response: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(response["is_error"], false, "{response}");
    let calls = fs::read_to_string(output.join("calls.jsonl")).unwrap();
    assert_eq!(
        calls.lines().count(),
        1,
        "shared requirements must acquire the batch only once"
    );
    assert_eq!(
        serde_json::from_str::<Value>(calls.lines().next().unwrap()).unwrap()["requested"],
        json!(["a", "b", "c"])
    );
    response["result"]["assessment"]["claims"][receipt["claims"]["current_claim"].as_str().unwrap()]
        .clone()
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn a_named_batch_operation_is_acquired_once_for_shared_requirements() {
    let claim = run_batch("complete");
    assert_eq!(claim["status"], "supported", "{claim}");
}

#[test]
#[ignore = "requires real EAL installation; set ENGARG_EAL_HOST"]
fn incomplete_or_unavailable_batch_results_do_not_support_the_claim() {
    for variant in [
        "omitted",
        "truncated",
        "wrong_ids",
        "duplicates",
        "unavailable",
    ] {
        let claim = run_batch(variant);
        assert_ne!(claim["status"], "supported", "{variant}: {claim}");
    }
}

#[test]
fn copied_command_implementations_are_automatically_pinned_and_rebuild_rejects_mutation() {
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let output = root.path().join("package");
    build(&tools, &output).unwrap();
    let manifest = eal_collector_contracts::RegistryDocument::from_toml(
        &fs::read_to_string(output.join("tools.toml")).unwrap(),
    )
    .unwrap();
    let expected =
        engarg::authoring_checkpoint::digest(fs::read(output.join("tools/source.py")).unwrap());
    for binding in manifest.tools.values() {
        assert!(
            binding
                .pinned_files
                .iter()
                .any(|pin| pin.path == "tools/source.py" && pin.sha256 == expected)
        );
    }
    fs::write(
        output.join("tools/source.py"),
        "raise RuntimeError('changed implementation')\n",
    )
    .unwrap();
    let rejected = root.path().join("rebuilt");
    assert!(
        build(&output.join("tools.toml"), &rejected)
            .unwrap_err()
            .to_string()
            .contains("SHA-256 pin")
    );
    assert!(!rejected.exists());
}

#[test]
#[ignore = "requires installed ENGARG_EAL_HOST with runtime companions"]
fn changed_packaged_implementation_is_rejected_before_it_can_execute() {
    let host = PathBuf::from(std::env::var_os("ENGARG_EAL_HOST").expect("matching host required"));
    let root = TempDir::new().unwrap();
    let tools = source(root.path());
    let output = root.path().join("package");
    let receipt = build(&tools, &output).unwrap();
    let marker = root.path().join("unexpected-execution");
    fs::write(
        output.join("tools/source.py"),
        format!(
            "from pathlib import Path\nPath({:?}).write_text('must not execute')\n",
            marker.to_str().unwrap()
        ),
    )
    .unwrap();
    let result = Command::new(host)
        .arg("run")
        .arg(output.join("case.eal"))
        .arg("--tools")
        .arg(output.join("tools.toml"))
        .arg("--workspace")
        .arg(root.path().join("run"))
        .arg("--claim")
        .arg(receipt["claims"]["current_claim"].as_str().unwrap())
        .args(["--now", reasoning::NOW])
        .output()
        .unwrap();
    assert!(
        !marker.exists(),
        "mutated implementation executed before pin admission"
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stdout.contains("Pinned collector file identity")
            || stderr.contains("Pinned collector file identity"),
        "{stdout}\n{stderr}"
    );
}
