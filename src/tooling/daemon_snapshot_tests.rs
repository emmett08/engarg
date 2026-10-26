use super::*;
use std::io::Read;

fn file(path: &str, content: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        content: content.into(),
    }
}

#[test]
fn archive_preserves_exact_bytes_and_fixed_safe_metadata() {
    let files = [
        file("nested/collector.py", "print('λ')\n"),
        file("other.txt", "unchanged\n"),
    ];
    let bytes = archive(&files).unwrap();
    assert_eq!(bytes, archive(&files).unwrap());
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut names = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        assert_eq!(entry.header().uid().unwrap(), 65534);
        assert_eq!(entry.header().gid().unwrap(), 65534);
        assert_eq!(entry.header().mtime().unwrap(), 0);
        if entry.header().entry_type().is_dir() {
            assert_eq!(entry.header().mode().unwrap(), 0o755);
        } else {
            assert!(entry.header().entry_type().is_file());
            assert_eq!(entry.header().mode().unwrap(), 0o444);
            let expected = files.iter().find(|file| file.path == name).unwrap();
            let mut body = String::new();
            entry.read_to_string(&mut body).unwrap();
            assert_eq!(body, expected.content);
        }
        names.push(name);
    }
    assert_eq!(names, ["nested", "nested/collector.py", "other.txt"]);
}

#[test]
fn archive_rejects_traversal_aliases_conflicts_and_resource_overflow() {
    for path in [
        "../escape",
        "/absolute",
        "a/../escape",
        "a/./file",
        "a//file",
        "a\\file",
        "a:file",
        "a\nfile",
        "",
    ] {
        assert!(archive(&[file(path, "x")]).is_err(), "{path:?}");
    }
    for files in [
        vec![file("a", "x"), file("A", "y")],
        vec![file("a", "x"), file("a/file", "y")],
        vec![file("A/one", "x"), file("a/two", "y")],
    ] {
        assert!(archive(&files).is_err());
    }
    assert!(archive(&vec![file("file", "x"); 65]).is_err());
    assert!(archive(&[file("file", &"x".repeat(MAX_FILE_BYTES + 1))]).is_err());
    assert!(
        archive(&[
            file("one", &"x".repeat(MAX_FILE_BYTES)),
            file("two", &"x".repeat(MAX_FILE_BYTES)),
            file("three", "x")
        ])
        .is_err()
    );
    assert!(archive(&[]).is_ok());
    let long_name = "a".repeat(200);
    let bytes = archive(&[file(&long_name, "long")]).unwrap();
    let mut tar = tar::Archive::new(Cursor::new(bytes));
    let mut entry = tar.entries().unwrap().next().unwrap().unwrap();
    assert_eq!(entry.path().unwrap().to_string_lossy(), long_name);
    let mut body = String::new();
    entry.read_to_string(&mut body).unwrap();
    assert_eq!(body, "long");
}

fn mounts(source: &str, fixtures: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!([
        {"Type":"volume","Name":source,"Destination":"/src","RW":true,"Source":"/daemon/private/path"},
        {"Type":"volume","Name":fixtures,"Destination":"/fixtures","RW":true,"Source":"/daemon/other/path"}
    ])).unwrap()
}

#[test]
fn worker_uses_only_admitted_daemon_volume_names_and_readonly_mounts() {
    let mut snapshot =
        DaemonSnapshot::new("engarg-1", &[file("main.rs", "fn main() {}")], &[]).unwrap();
    assert!(snapshot.worker_mount_args().is_err());
    snapshot
        .admit_mounts(&mounts("anonymous_source", "anonymous_fixtures"))
        .unwrap();
    let argv = snapshot.worker_mount_args().unwrap();
    assert!(argv.contains(
        &"type=volume,source=anonymous_source,target=/src,readonly,volume-nocopy".into()
    ));
    assert!(argv.contains(
        &"type=volume,source=anonymous_fixtures,target=/fixtures,readonly,volume-nocopy".into()
    ));
    assert!(!argv.iter().any(|arg| arg.contains("/daemon/")));
    let staging = snapshot.staging_argv("sha256:immutable");
    assert_eq!(staging[0], "create");
    assert!(staging.contains(&"type=volume,target=/src,volume-nocopy".into()));
    assert!(staging.contains(&"engarg.snapshot-owner=engarg-1".into()));
    assert_eq!(snapshot.expected_owner(), "engarg-1");
    assert!(
        snapshot
            .ownership_argv()
            .contains(&"{{index .Config.Labels \"engarg.snapshot-owner\"}}".into())
    );
    assert!(
        !staging
            .iter()
            .any(|arg| arg.contains("type=bind") || arg == "start" || arg == "run")
    );
    assert_eq!(
        snapshot.cleanup_argv(),
        ["container", "rm", "--force", "--volumes", "engarg-1-inputs"]
    );
}

#[test]
fn snapshot_volume_metadata_rejects_mount_injection_and_shared_volumes() {
    for bytes in [mounts("same", "same"), mounts("unsafe,readonly", "valid"), b"[]".to_vec(), br#"[{"Type":"bind","Name":"x","Destination":"/src","RW":true},{"Type":"volume","Name":"y","Destination":"/fixtures","RW":true}]"#.to_vec()] {
        let mut snapshot = DaemonSnapshot::new("engarg-1", &[], &[]).unwrap();
        assert!(snapshot.admit_mounts(&bytes).is_err());
        assert!(snapshot.worker_mount_args().is_err());
    }
    assert!(DaemonSnapshot::new("unsafe,name", &[], &[]).is_err());
}
