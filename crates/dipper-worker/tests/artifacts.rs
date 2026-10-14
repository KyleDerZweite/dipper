use dipper_worker::artifacts::{fingerprint_runtime, hash_artifact};
use std::fs;

#[test]
fn model_hash_identifies_content_and_detects_changed_or_replaced_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("model.bin");
    fs::write(&path, b"abc").unwrap();
    let (fingerprint, guard) = hash_artifact(&path).unwrap();
    assert_eq!(
        fingerprint.sha256.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(fingerprint.size_bytes, Some(3));
    assert!(fingerprint.modified_unix_ns.is_some());
    guard.validate().unwrap();
    fs::write(&path, b"abcd").unwrap();
    assert!(guard.validate().is_err());
    let (_, guard) = hash_artifact(&path).unwrap();
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, b"abcd").unwrap();
    fs::rename(replacement, &path).unwrap();
    assert!(guard.validate().is_err());
}

#[test]
fn runtime_reports_actual_or_explicitly_unavailable_native_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("model.bin");
    fs::write(&path, b"abc").unwrap();
    let (runtime, guards) = fingerprint_runtime(&path).unwrap();
    assert!(runtime.engine_version.contains("1.9.5"));
    assert_eq!(runtime.native_artifacts.len(), 5);
    for native in runtime.native_artifacts {
        assert!(native.sha256.is_some() || native.unavailable_reason.is_some());
    }
    for guard in guards {
        guard.validate().unwrap();
    }
}
