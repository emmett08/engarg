//! Native tool bindings use the consuming collector's exact stdin contract.
use engarg::{
    acquisition::{AcquisitionPlan, ToolArgument, override_plans_with_catalogue},
    collector_catalogue::CollectorCatalogue,
    domain::{ArgumentCase, Request},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

fn fixture() -> (Request, ArgumentCase) {
    (
        serde_json::from_str(include_str!("../examples/request.json")).unwrap(),
        serde_json::from_str(include_str!("../examples/case.json")).unwrap(),
    )
}
fn catalogue() -> CollectorCatalogue {
    CollectorCatalogue::from_json(include_bytes!("../contracts/collectors/native.json")).unwrap()
}

fn selected(
    case: &ArgumentCase,
    id: &str,
    args: &[(&str, &str)],
    unknown: &[&str],
) -> AcquisitionPlan {
    AcquisitionPlan {
        input_preparation: None,
        need_id: case.evidence_plan[0].id.clone(),
        tool_id: id.into(),
        arguments: args
            .iter()
            .map(|(name, value)| ToolArgument {
                name: (*name).into(),
                value: (*value).into(),
            })
            .collect(),
        unresolved_arguments: unknown.iter().map(|s| (*s).into()).collect(),
    }
}
#[path = "common/collector_source.rs"]
mod collector_source;

#[test]
fn native_templates_preserve_typed_inputs_and_unresolved_scope() {
    let (_, case) = fixture();
    for (id, args, unknown, expected, predicate) in [
        (
            "kubernetes",
            vec![("name", "pilot-sync"), ("kind", "Application")],
            vec!["inventory", "resource", "namespace"],
            json!({"name":"pilot-sync","resource":null,"namespace":null}),
            "cluster_count >= 1",
        ),
        (
            "github",
            vec![
                ("repository", "example/platform"),
                ("commit", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            ],
            vec![],
            json!({"repository":"example/platform","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
            "found == true",
        ),
        (
            "tempo",
            vec![
                ("kind", "Application"),
                ("name", "pilot-sync"),
                ("start_unix", "1"),
                ("end_unix", "2"),
            ],
            vec!["inventory", "url", "cluster_attribute"],
            json!({"start_unix":1,"end_unix":2}),
            "cluster_count >= 1",
        ),
    ] {
        let plans = override_plans_with_catalogue(
            &case,
            &[],
            vec![selected(&case, id, &args, &unknown)],
            Some(&catalogue()),
        )
        .unwrap();
        let selected = catalogue();
        let descriptor = selected.get(id).unwrap();
        assert_eq!(descriptor.input(&plans[0]).unwrap(), expected);
        assert_eq!(descriptor.acquisition_predicate, predicate);
        let binding = descriptor.binding(&plans[0]).unwrap();
        assert!(
            binding
                .argv
                .starts_with(&["eal-collector".to_owned(), id.to_owned()])
        );
    }
}
#[test]
fn native_ids_require_explicit_catalogue_selection_during_discovery() {
    let (_, case) = fixture();
    for descriptor in catalogue().0.collectors {
        let plan = AcquisitionPlan {
            input_preparation: None,
            need_id: case.evidence_plan[0].id.clone(),
            tool_id: descriptor.id.clone(),
            arguments: Vec::new(),
            unresolved_arguments: descriptor.parameters.clone(),
        };
        assert!(override_plans_with_catalogue(&case, &[], vec![plan.clone()], None).is_err());
        let plans =
            override_plans_with_catalogue(&case, &[], vec![plan], Some(&catalogue())).unwrap();
        assert!(!plans.is_empty());
    }
}

#[test]
fn overrides_reject_duplicate_unknown_and_invalid_native_inputs() {
    let (_, case) = fixture();
    let plan = selected(&case, "custom", &[], &["adapter"]);
    assert!(
        override_plans_with_catalogue(
            &case,
            &[],
            vec![plan.clone(), plan.clone()],
            Some(&catalogue())
        )
        .is_err()
    );
    let mut unknown = plan;
    unknown.need_id = "unknown".into();
    assert!(override_plans_with_catalogue(&case, &[], vec![unknown], Some(&catalogue())).is_err());
    let mut plan = selected(
        &case,
        "tempo",
        &[("start_unix", "yesterday")],
        &[
            "inventory",
            "url",
            "kind",
            "name",
            "cluster_attribute",
            "end_unix",
        ],
    );
    assert!(
        override_plans_with_catalogue(&case, &[], vec![plan.clone()], Some(&catalogue())).is_err()
    );
    plan.arguments[0].value = "1".into();
    assert!(override_plans_with_catalogue(&case, &[], vec![plan], Some(&catalogue())).is_ok());
}

fn host(binary: &std::path::Path, dir: &std::path::Path, request: Value) -> Value {
    let mut child = Command::new(binary)
        .arg("--workspace")
        .arg(dir)
        .arg("--registry")
        .arg(dir.join("tools.toml"))
        .args(["--timeout", "15"])
        .env_clear()
        .env("PATH", binary.parent().unwrap())
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
    let out = child.wait_with_output().unwrap();
    let result: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("host failed {}", String::from_utf8_lossy(&out.stderr)));
    assert!(out.status.success(), "{result}");
    assert_ne!(result["is_error"], true, "{result}");
    result["result"].clone()
}
#[test]
#[cfg(unix)]
#[ignore = "requires the pinned EAL producer; mandatory compatibility CI supplies it"]
fn latest_native_collectors_execute_with_mock_services_and_preserve_negatives() {
    use std::os::unix::fs::PermissionsExt;
    let binary =
        std::env::var_os("ENGARG_EAL_HOST").expect("compatibility test requires ENGARG_EAL_HOST");
    let binary = std::path::PathBuf::from(binary);
    let bin_dir = binary.parent().unwrap();
    let collector = bin_dir.join("eal-collector");
    let supervisor = bin_dir.join("eal-process-supervisor");
    assert!(
        collector.is_file() && supervisor.is_file(),
        "Build the current native collector and runtime binaries"
    );
    for provider in ["kubernetes", "github", "tempo"] {
        let dir = tempfile::tempdir().unwrap();
        let inventory = dir.path().join("inventory.json");
        std::fs::write(&inventory,json!({"revision":"test-only","clusters":[{"id":"pilot","context":"pilot","tempo_cluster":"pilot"}]}).to_string()).unwrap();
        let fake = dir.path().join("fake-cli");
        let script = if provider == "github" {
            "#!/bin/sh\nprintf '%s' '{\"sha\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"commit\":{\"tree\":{\"sha\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}}}'\n"
        } else {
            "#!/bin/sh\nexit 0\n"
        };
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/api/search", listener.local_addr().unwrap());
        let server = (provider == "tempo").then(|| {
            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let text = String::from_utf8(bytes).unwrap();
                assert!(text.contains("start=1") && text.contains("end=2") && text.contains("q="));
                let body = r#"{"traces":[]}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            })
        });
        let (_, case) = fixture();
        let args = match provider {
            "kubernetes" => vec![
                ("inventory", inventory.to_str().unwrap()),
                ("resource", "applications.argoproj.io"),
                ("kind", "Application"),
                ("namespace", "pilot"),
                ("name", "pilot-sync"),
            ],
            "github" => vec![
                ("repository", "example/platform"),
                ("commit", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            ],
            _ => vec![
                ("inventory", inventory.to_str().unwrap()),
                ("url", url.as_str()),
                ("kind", "Application"),
                ("name", "pilot-sync"),
                ("cluster_attribute", "resource.k8s.cluster.name"),
                ("start_unix", "1"),
                ("end_unix", "2"),
            ],
        };
        let plan = override_plans_with_catalogue(
            &case,
            &[],
            vec![selected(&case, provider, &args, &[])],
            Some(&catalogue()),
        )
        .unwrap();
        let selected = catalogue();
        let descriptor = selected.get(provider).unwrap();
        let source = collector_source::source(
            &descriptor.version,
            &descriptor.input(&plan[0]).unwrap(),
            &descriptor.acquisition_predicate,
        );
        let tool = "source".to_owned();
        let mut binding = descriptor.binding(&plan[0]).unwrap();
        for argument in &mut binding.argv {
            *argument = match argument.as_str() {
                "eal-collector" => collector.to_str().unwrap().to_owned(),
                "/REPLACE_EXECUTABLE" => fake.to_str().unwrap().to_owned(),
                "/REPLACE_SUPERVISOR" => supervisor.to_str().unwrap().to_owned(),
                _ => argument.clone(),
            };
        }
        binding.validate(&tool).expect("resolved native binding");
        let registry = eal_collector_contracts::CommandRegistry {
            tools: std::collections::BTreeMap::from([(tool, binding)]),
        };
        std::fs::write(dir.path().join("tools.toml"), registry.to_toml().unwrap()).unwrap();
        let validation = host(
            &binary,
            dir.path(),
            json!({"operation":"validate","source":source}),
        );
        assert_eq!(validation["valid"], true, "{validation}");
        let evidence = "observation";
        let collected = host(
            &binary,
            dir.path(),
            json!({"operation":"collect","source":source,"context":{},"evidence_ids":[evidence]}),
        );
        assert_eq!(
            collected["records"][evidence]["status"], "ok",
            "{collected}"
        );
        let measured = &collected["records"][evidence]["value"];
        match provider {
            "kubernetes" => assert_eq!(measured["all_present"], false),
            "tempo" => {
                assert_eq!(measured["all_matched"], false);
                assert_eq!(measured["negative_inference"], "unavailable");
                assert_eq!(measured["search_exhaustive"], false);
            }
            "github" => assert_eq!(
                measured["commit"],
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            _ => unreachable!(),
        }
        assert!(measured.get("accepted").is_none());
        let assessment = host(
            &binary,
            dir.path(),
            json!({"operation":"reason","source":source,"context":{},"collection_id":collected["collection_id"]}),
        );
        assert_eq!(
            assessment["evidence"][evidence]["status"], "available",
            "{assessment}"
        );
        if let Some(server) = server {
            server.join().unwrap();
        }
    }
}
