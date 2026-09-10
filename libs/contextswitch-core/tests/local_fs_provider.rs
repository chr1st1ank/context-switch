//! Tests for the local filesystem storage provider: the shared conformance
//! suite plus local-specific behavior (bootstrap, corruption, locking).

use std::fs::{self, File};
use std::io::Write;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use contextswitch_core::conformance::check_provider_conformance;
use contextswitch_core::storage::{LocalFsProvider, StorageError, StorageProvider};
use tempfile::tempdir;

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

#[test]
fn local_provider_conformance() {
    // Each conformance check needs fresh storage; keep the dirs alive for
    // the duration of the suite.
    let mut dirs = Vec::new();
    check_provider_conformance(move || {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.json");
        dirs.push(dir);
        Box::new(LocalFsProvider::new(&path).unwrap())
    });
}

#[test]
fn new_creates_document_and_parents() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested/deep/data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    assert!(path.exists());
    let snapshot = provider.read().unwrap();
    assert_eq!(snapshot.version, "0");
}

#[test]
fn new_does_not_clobber_existing_document() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    document.add_project("keep", at(0)).unwrap();
    provider.commit(document, &snapshot.version).unwrap();

    // Re-opening must not reset the document.
    let reopened = LocalFsProvider::new(&path).unwrap();
    assert_eq!(reopened.read().unwrap().version, "1");
    assert_eq!(reopened.read().unwrap().document.projects.len(), 1);
}

#[test]
fn read_reports_corrupt_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    fs::write(&path, "{not json").unwrap();
    let provider = LocalFsProvider::new(&path).unwrap();
    assert!(matches!(provider.read(), Err(StorageError::Corrupt(_))));
}

#[test]
fn read_reports_divergent_active_span() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    document.start_timer(at(0), None, vec![]).unwrap();
    provider.commit(document, &snapshot.version).unwrap();

    // Hand-corrupt the file: point active_span_id at nothing.
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    json["active_span_id"] = serde_json::Value::String(uuid::Uuid::new_v4().to_string());
    fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();
    assert!(matches!(provider.read(), Err(StorageError::Corrupt(_))));
}

#[test]
fn held_lock_blocks_commit_until_timeout() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider =
        LocalFsProvider::with_options(&path, Duration::from_millis(200), Duration::from_secs(30))
            .unwrap();
    let _held = File::create(dir.path().join("data.json.lock")).unwrap();

    let snapshot = provider.read().unwrap();
    let document = snapshot.document.clone();
    assert!(matches!(
        provider.commit(document, &snapshot.version),
        Err(StorageError::Locked)
    ));
}

#[test]
fn stale_lock_is_reclaimed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider =
        LocalFsProvider::with_options(&path, Duration::from_secs(1), Duration::from_secs(30))
            .unwrap();

    // A lockfile abandoned a minute ago must not block commits.
    let lock = File::create(dir.path().join("data.json.lock")).unwrap();
    lock.set_modified(SystemTime::now() - Duration::from_secs(60))
        .unwrap();
    drop(lock);

    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    document.add_tag("reclaimed", at(0)).unwrap();
    let version = provider.commit(document, &snapshot.version).unwrap();
    assert_eq!(version, "1");
}

#[test]
fn commit_writes_pretty_human_readable_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    document.add_project("work", at(0)).unwrap();
    provider.commit(document, &snapshot.version).unwrap();

    let json = fs::read_to_string(&path).unwrap();
    assert!(json.contains("\"schema_version\": 1"));
    assert!(json.contains("\"revision\": 1"));
    assert!(json.contains("\"name\": \"work\""));
    // The lockfile and temp file must be cleaned up.
    assert!(!dir.path().join("data.json.lock").exists());
    assert!(!dir.path().join("data.json.tmp").exists());
}

#[test]
fn commit_does_not_leave_tmp_on_conflict() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    let snapshot = provider.read().unwrap();
    let mut document = snapshot.document.clone();
    document.add_project("a", at(0)).unwrap();
    provider
        .commit(document.clone(), &snapshot.version)
        .unwrap();
    let _ = provider.commit(document, &snapshot.version);
    assert!(!dir.path().join("data.json.tmp").exists());
}

#[test]
fn writes_do_not_corrupt_sibling_files() {
    // Regression guard: lock/tmp names must derive from the data file name.
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.json");
    let provider = LocalFsProvider::new(&path).unwrap();
    let mut other = File::create(dir.path().join("data.other")).unwrap();
    other.write_all(b"keep me").unwrap();
    let snapshot = provider.read().unwrap();
    provider
        .commit(snapshot.document.clone(), &snapshot.version)
        .unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("data.other")).unwrap(),
        "keep me"
    );
}
