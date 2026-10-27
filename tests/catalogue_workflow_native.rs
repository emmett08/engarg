//! The shipped portable example executes MCP, a command and authored deduction.
#![cfg(unix)]

use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn copy_fixture(destination: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/executable/finite-inventory");
    fs::create_dir_all(destination.join("tools")).unwrap();
    for file in [
        "case.eal",
        "tools.toml",
        "tools/snapshot.py",
        "tools/audit.py",
    ] {
        fs::copy(root.join(file), destination.join(file)).unwrap();
    }
}

#[test]
#[ignore = "requires the matching ENGARG_EAL_HOST binary"]
fn executable_inventory_collects_actual_mcp_and_command_observations() {
    let host = std::env::var_os("ENGARG_EAL_HOST").expect("Set ENGARG_EAL_HOST");
    let host = Path::new(&host).canonicalize().unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (scenario, supported) in [
        ("positive", true),
        ("case-sensitive", true),
        ("duplicate", false),
        ("incomplete", false),
        ("stale", false),
        ("error", false),
        ("manifest-mismatch", false),
        ("lying-audit", false),
    ] {
        let package = directory.path().join(scenario);
        let output = if scenario == "positive" {
            Command::new("bash")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/scripts/run-catalog-sync-authoring.sh"
                ))
                .args(["--simple-example", "--eal-host"])
                .arg(&host)
                .arg("--output-dir")
                .arg(&package)
                .output()
                .unwrap()
        } else {
            copy_fixture(&package);
            let source = fs::read_to_string(package.join("case.eal")).unwrap();
            let source_scenario = if scenario == "lying-audit" {
                "duplicate"
            } else {
                scenario
            };
            fs::write(
                package.join("case.eal"),
                source.replace(
                    "\"scenario\":\"positive\"",
                    &format!("\"scenario\":\"{source_scenario}\""),
                ),
            )
            .unwrap();
            if scenario == "lying-audit" {
                fs::write(package.join("tools/audit.py"), "import json,sys\njson.load(sys.stdin)\nprint(json.dumps({'value':{'count':3,'distinct':True}}))\n").unwrap();
            }
            Command::new(&host)
                .arg("run")
                .arg(package.join("case.eal"))
                .arg("--tools")
                .arg(package.join("tools.toml"))
                .arg("--workspace")
                .arg(package.join("run"))
                .args(["--claim", "inventory_verified"])
                .output()
                .unwrap()
        };
        assert_eq!(
            output.status.code(),
            Some(0),
            "scenario {scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            envelope["is_error"], false,
            "scenario {scenario}: {envelope}"
        );
        let result = &envelope["result"];
        assert_eq!(result["assessment"]["valid"], true);
        assert_eq!(
            result["assessment"]["claims"]["inventory_verified"]["status"],
            if supported {
                "supported"
            } else {
                "unsupported"
            }
        );
        let records = result["collection"]["records"].as_object().unwrap();
        assert_eq!(records.len(), 4, "no artificial formal-query acquisition");
        if supported {
            assert_eq!(records["snapshot"]["transport"], "mcp_stdio");
            assert_eq!(records["snapshot"]["operation"], "snapshot");
            assert_eq!(
                records["audit"]["input"]["items"],
                records["snapshot"]["value"]["items"]
            );
            assert_eq!(
                records["audit"]["dependencies"][0]["run_id"],
                records["snapshot"]["run_id"]
            );
            assert_eq!(
                result["assessment"]["arguments"]["inventory_argument"]["reasoning_result"]["input_origin"],
                "authored_program"
            );
        }
        if scenario == "lying-audit" {
            assert_eq!(records["audit"]["value"]["distinct"], true);
            assert_eq!(records["distinct_check"]["value"]["distinct"], false);
        }
    }
}
