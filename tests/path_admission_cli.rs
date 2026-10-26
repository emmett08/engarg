//! Destination aliases are rejected before coordinator startup or provider IO.
use std::{net::TcpListener, process::Command};

#[test]
fn relative_absolute_and_symlink_aliases_fail_without_provider_requests() {
    let directory = tempfile::tempdir().unwrap();
    let request = directory.path().join("request.json");
    std::fs::write(&request, include_bytes!("../examples/request.json")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let output = directory.path().join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_engarg"))
        .current_dir(directory.path())
        .args(["analyse", "--input"])
        .arg(&request)
        .args([
            "--model",
            "fixture",
            "--endpoint",
            &endpoint,
            "--no-tool-planning",
            "--topology",
            "ring",
            "--debate-participants",
            "3",
            "--max-calls",
            "9",
            "--output",
            "report.json",
            "--coordination-run",
        ])
        .arg(&output)
        .args(["--eal-coordinate", "missing-coordinator"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overlaps the output"));
    assert!(!output.exists());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
