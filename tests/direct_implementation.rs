//! Direct operation generation retains real gaps and composes portable authority.
use engarg::{
    Error,
    provider::{Completion, CompletionRequest, LanguageModel},
    tooling::{
        ToolchainProfile,
        capability::CapabilityImplementationConfig,
        direct::{DirectImplementationConfig, implement},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "common/executable_case.rs"]
mod executable_case;
#[allow(dead_code)]
#[path = "common/reasoning.rs"]
mod reasoning;
#[allow(dead_code)]
#[path = "common/reasoning_graph.rs"]
mod reasoning_graph;

struct NoModel;
impl LanguageModel for NoModel {
    async fn complete(&self, _: CompletionRequest) -> Result<Completion, Error> {
        panic!("bound operations and missing semantic specifications cannot spend coding calls")
    }
}
fn report() -> Value {
    let (request, argument, support) = executable_case::fixture();
    json!({"schema_version":engarg::report::RUN_REPORT_SCHEMA,"execution_mode":"offline_synthetic_fixture",
        "input_sha256":eal_collector_contracts::execution::canonical_digest(&request).unwrap(),
        "mode":"single","request":request,"argument":argument,"transcript":[],"support_elaboration":support,
        "limitations":["Synthetic operation admission fixture."]})
}
fn config(root: &Path) -> DirectImplementationConfig {
    DirectImplementationConfig {
        implementation: CapabilityImplementationConfig {
            language: "python".into(),
            libraries: Default::default(),
            profile: ToolchainProfile {
                schema: "engarg.toolchain/1".into(),
                language: "python".into(),
                docker: "must-not-start".into(),
                image: "must-not-start".into(),
                build_argv: vec![],
                run_argv: vec!["python3".into()],
                inherit_env: vec![],
                runtime_network: "none".into(),
                timeout_seconds: 30,
                memory_mb: 128,
                output_bytes: 1048576,
            },
            store: root.join("store"),
            max_calls: 2,
            max_input_bytes: 1048576,
            max_response_bytes: 1048576,
            supervisor: PathBuf::from("must-not-start"),
            engarg_executable: std::env::current_exe().unwrap(),
        },
        output_dir: root.join("attempt"),
    }
}
#[tokio::test]
async fn missing_semantics_are_retained_without_model_or_tool_execution() {
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("tools.toml");
    fs::write(&tools, "[tools]\n").unwrap();
    let original = report();
    let implemented = implement(&NoModel, &original, &tools, config(root.path()))
        .await
        .unwrap();
    let metadata = implemented.tooling.as_ref().unwrap();
    assert_eq!(metadata.model_calls, 0);
    assert_eq!(metadata.operations.len(), 0);
    assert_eq!(metadata.deferred.len(), 2);
    assert_eq!(
        implemented
            .direct_bindings
            .as_ref()
            .unwrap()
            .unbound_operations
            .len(),
        2
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&metadata.retained_input).unwrap()).unwrap(),
        original
    );
    engarg::report::read_direct(&serde_json::to_value(&implemented).unwrap()).unwrap();
    assert!(!root.path().join("store").exists());
}
#[tokio::test]
async fn admitted_existing_tools_are_copied_with_pins_and_require_no_generation() {
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("tools.toml");
    fs::create_dir(root.path().join("tools")).unwrap();
    let data = b"import json\nprint(json.dumps({'measurement':{'count':3,'distinct':True}}))\n";
    fs::write(root.path().join("tools/source.py"), data).unwrap();
    let pin = engarg::authoring_checkpoint::digest(data);
    fs::write(&tools,format!("[tools.premise_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\npinned_files=[{{path='tools/source.py',sha256='{pin}'}}]\n[tools.applicability_source]\nkind='command'\nversion='1'\nargv=['python3','tools/source.py']\n")).unwrap();
    let implemented = implement(&NoModel, &report(), &tools, config(root.path()))
        .await
        .unwrap();
    let metadata = implemented.tooling.as_ref().unwrap();
    assert_eq!(metadata.model_calls, 0);
    assert!(metadata.deferred.is_empty());
    assert!(
        implemented
            .direct_bindings
            .unwrap()
            .unbound_operations
            .is_empty()
    );
    assert_eq!(
        fs::read(metadata.tools.parent().unwrap().join("tools/source.py")).unwrap(),
        data
    );
    assert!(!root.path().join("store").exists());
}
#[test]
fn merged_exports_rebase_pins_and_reject_replacing_existing_authority() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("tools.toml"), "[tools]\n").unwrap();
    fs::create_dir(root.path().join("export")).unwrap();
    let source = b"import json; print(json.dumps({'value':True}))\n";
    fs::write(root.path().join("export/operation.py"), source).unwrap();
    let pin = engarg::authoring_checkpoint::digest(source);
    let export = root.path().join("export/tools.toml");
    fs::write(&export,format!("[tools.observe]\nkind='command'\nversion='1'\nargv=['python3','operation.py']\ninherit_env=['PATH']\npinned_files=[{{path='operation.py',sha256='{pin}'}}]\n")).unwrap();
    let selected = vec![("observe".into(), export)];
    let merged = engarg::package::merge_tool_exports(
        &root.path().join("tools.toml"),
        &selected,
        &root.path().join("merged"),
    )
    .unwrap();
    let document =
        eal_collector_contracts::RegistryDocument::from_toml(&fs::read_to_string(&merged).unwrap())
            .unwrap();
    let binding = &document.tools["observe"];
    assert_eq!(binding.argv[1], "tools/generated/observe/operation.py");
    assert_eq!(binding.pinned_files[0].path, binding.argv[1]);
    assert_eq!(binding.pinned_files[0].sha256, pin);
    assert_eq!(binding.inherit_env, Some(vec!["PATH".into()]));
    assert!(
        engarg::package::merge_tool_exports(&merged, &selected, &root.path().join("replacement"))
            .is_err()
    );
    assert!(!root.path().join("replacement").exists());
}
