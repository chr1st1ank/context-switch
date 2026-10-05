//! Tests for [`SyncEngine`] and [`MemoryCachingProvider`]: the
//! conformance suite against the write-through decorator, plus
//! engine-specific behavior — read hits, optimistic buffered commits,
//! the write buffer's async error surface, and background refresh.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tempfile::tempdir;

use contextswitch_core::blob::InMemoryBlobStore;
use contextswitch_core::conformance::check_provider_conformance;
use contextswitch_core::crypto::IdentityCipher;
use contextswitch_core::domain::Logbook;
use contextswitch_core::provider::GenericProvider;
use contextswitch_core::storage::{
    LocalFsProvider, StorageError, StorageProvider, StorageSnapshot,
};
use contextswitch_core::sync::{CommitMode, MemoryCachingProvider, SyncEngine, SyncStatus};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

fn generic_provider() -> Arc<GenericProvider> {
    Arc::new(GenericProvider::new(
        Arc::new(InMemoryBlobStore::new()),
        Arc::new(IdentityCipher),
        None,
    ))
}

fn cached(inner: Arc<dyn StorageProvider>) -> SyncEngine {
    SyncEngine::new(inner, Duration::from_secs(30), CommitMode::Buffered)
}

/// Poll `f` until it holds or the deadline passes.
fn eventually(f: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Counts delegating calls so tests can prove the cache avoided them.
struct CountingProvider {
    inner: Arc<dyn StorageProvider>,
    reads: AtomicUsize,
    commits: AtomicUsize,
}

impl CountingProvider {
    fn new(inner: Arc<dyn StorageProvider>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            reads: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
        })
    }
}

impl StorageProvider for CountingProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.inner.commit(logbook, expected_version)
    }

    fn fingerprint(&self) -> Result<String, StorageError> {
        self.inner.fingerprint()
    }
}

/// Commits slowly, so the write buffer's `pending` state is observable.
struct SlowProvider {
    inner: Arc<dyn StorageProvider>,
    delay: Duration,
}

impl StorageProvider for SlowProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        thread::sleep(self.delay);
        self.inner.commit(logbook, expected_version)
    }

    fn fingerprint(&self) -> Result<String, StorageError> {
        self.inner.fingerprint()
    }
}

/// Fails `fingerprint` a fixed number of times, then delegates.
struct FlakyProbeProvider {
    inner: Arc<dyn StorageProvider>,
    failures_left: AtomicUsize,
}

impl StorageProvider for FlakyProbeProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        self.inner.commit(logbook, expected_version)
    }

    fn fingerprint(&self) -> Result<String, StorageError> {
        let exhausted = self
            .failures_left
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_err();
        if exhausted {
            return self.inner.fingerprint();
        }
        Err(StorageError::Unavailable("probe outage".to_string()))
    }
}

/// Fails commits with `Unavailable` a fixed number of times.
struct FlakyProvider {
    inner: Arc<dyn StorageProvider>,
    failures_left: AtomicUsize,
}

impl StorageProvider for FlakyProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        if self.failures_left.fetch_sub(1, Ordering::SeqCst) > 0 {
            return Err(StorageError::Unavailable("simulated outage".to_string()));
        }
        self.inner.commit(logbook, expected_version)
    }

    fn fingerprint(&self) -> Result<String, StorageError> {
        self.inner.fingerprint()
    }
}

#[test]
fn write_through_conformance_in_memory() {
    check_provider_conformance(|| {
        Box::new(MemoryCachingProvider::new(
            generic_provider(),
            Duration::from_secs(30),
        ))
    });
}

#[test]
fn write_through_conformance_local_fs() {
    // Keep the temp dirs alive for the duration of the suite.
    let mut dirs = Vec::new();
    check_provider_conformance(move || {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.json");
        dirs.push(dir);
        let inner = LocalFsProvider::new(&path).unwrap();
        Box::new(MemoryCachingProvider::new(
            Arc::new(inner),
            Duration::from_secs(30),
        ))
    });
}

#[test]
fn second_read_is_a_cache_hit() {
    let inner = CountingProvider::new(generic_provider());
    let provider = cached(inner.clone());

    let first = provider.snapshot().unwrap();
    let second = provider.snapshot().unwrap();
    assert_eq!(first.version, second.version);
    assert_eq!(inner.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn buffered_commit_is_optimistic() {
    let inner = generic_provider();
    let provider = cached(inner);

    let snap = provider.snapshot().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_project("work", None, at(0)).unwrap();

    let version = provider.commit(logbook, &snap.version).unwrap();
    assert_eq!(version, "1");

    // The optimistic state is immediately visible without a delegating read.
    let reread = provider.snapshot().unwrap();
    assert_eq!(reread.version, "1");
    assert_eq!(reread.logbook.projects.len(), 1);

    provider.flush().unwrap();
    assert_eq!(provider.pending_writes(), 0);
}

#[test]
fn stale_buffered_commit_is_rejected_synchronously() {
    let provider = cached(generic_provider());
    let snap = provider.snapshot().unwrap();
    provider
        .commit(snap.logbook.clone(), &snap.version)
        .unwrap();
    provider.flush().unwrap();

    // The cache knows version 1 — a commit against 0 is provably stale.
    match provider.commit(snap.logbook.clone(), &snap.version) {
        Err(StorageError::Conflict { expected, actual }) => {
            assert_eq!(expected, "0");
            assert_eq!(actual, "1");
        }
        other => panic!("expected synchronous Conflict, got {other:?}"),
    }
    assert_eq!(provider.pending_writes(), 0);
}

#[test]
fn buffered_commits_drain_in_order() {
    let inner = generic_provider();
    let provider = cached(inner.clone());

    let snap = provider.snapshot().unwrap();
    let mut lb1 = snap.logbook.clone();
    lb1.add_tag("one", at(0)).unwrap();
    let v1 = provider.commit(lb1, &snap.version).unwrap();

    let mut lb2 = provider.snapshot().unwrap().logbook;
    lb2.add_tag("two", at(0)).unwrap();
    let v2 = provider.commit(lb2, &v1).unwrap();
    assert_eq!(v2, "2");

    provider.flush().unwrap();
    let canonical = inner.read().unwrap();
    assert_eq!(canonical.version, "2");
    assert_eq!(canonical.logbook.tags.len(), 2);
}

#[test]
fn conflicting_write_lands_in_failed_writes() {
    let inner = generic_provider();
    let provider = cached(inner.clone());

    let snap = provider.snapshot().unwrap();

    // Another writer commits behind the cache's back.
    let mut external = snap.logbook.clone();
    external.add_tag("external", at(0)).unwrap();
    inner.commit(external, &snap.version).unwrap();

    // The cache's entry is still version 0, so this commit is queued...
    let mut logbook = snap.logbook.clone();
    logbook.add_tag("mine", at(0)).unwrap();
    provider.commit(logbook, &snap.version).unwrap();

    // ...and rejected when drained.
    assert!(eventually(|| provider.take_failed_write().is_some()));
    assert_eq!(provider.pending_writes(), 0);

    // The cache reconciles to canonical state.
    assert!(eventually(|| provider.snapshot().unwrap().version == "1"));
    assert_eq!(provider.snapshot().unwrap().logbook.tags.len(), 1);
}

#[test]
fn flush_propagates_write_errors() {
    let inner = generic_provider();
    let provider = cached(inner.clone());

    let snap = provider.snapshot().unwrap();
    let mut external = snap.logbook.clone();
    external.add_tag("external", at(0)).unwrap();
    inner.commit(external, &snap.version).unwrap();

    let mut mine = snap.logbook.clone();
    mine.add_tag("mine", at(0)).unwrap();
    provider.commit(mine, &snap.version).unwrap();
    match provider.flush() {
        Ok(Some(failed)) => {
            assert!(matches!(failed.error, StorageError::Conflict { .. }));
            // The rejected mutation is preserved for export/retry.
            assert_eq!(failed.logbook.tags.len(), 1);
        }
        other => panic!("expected a failed write from flush, got {other:?}"),
    }
}

#[test]
fn unavailable_writes_are_retried_then_succeed() {
    let flaky = Arc::new(FlakyProvider {
        inner: generic_provider(),
        failures_left: AtomicUsize::new(2),
    });
    let provider = cached(flaky);

    let snap = provider.snapshot().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_tag("resilient", at(0)).unwrap();
    provider.commit(logbook, &snap.version).unwrap();

    // 2 failures < retry budget, so the write eventually lands.
    provider.flush().unwrap();
    assert!(eventually(|| provider.pending_writes() == 0));
    assert_eq!(provider.sync_status(), SyncStatus::Clean);
}

#[test]
fn pending_writes_and_sync_status() {
    let slow = Arc::new(SlowProvider {
        inner: generic_provider(),
        delay: Duration::from_millis(300),
    });
    let provider = cached(slow);

    let snap = provider.snapshot().unwrap();
    provider
        .commit(snap.logbook.clone(), &snap.version)
        .unwrap();

    assert!(eventually(|| provider.pending_writes() == 1));
    assert_eq!(provider.sync_status(), SyncStatus::Pending(1));

    provider.flush().unwrap();
    assert_eq!(provider.sync_status(), SyncStatus::Clean);
}

#[test]
fn refresh_picks_up_external_change() {
    let inner = generic_provider();
    let provider = SyncEngine::new(
        inner.clone(),
        Duration::from_millis(20),
        CommitMode::Buffered,
    );

    let snap = provider.snapshot().unwrap();
    assert_eq!(snap.version, "0");

    let mut external = snap.logbook.clone();
    external.add_tag("external", at(0)).unwrap();
    inner.commit(external, &snap.version).unwrap();

    assert!(eventually(|| provider.snapshot().unwrap().version == "1"));
}

#[test]
fn degraded_status_recovers_when_probes_resume() {
    // One failure is consumed by the cold-read probe, one by the first
    // worker probe; later probes succeed.
    let flaky = Arc::new(FlakyProbeProvider {
        inner: generic_provider(),
        failures_left: AtomicUsize::new(2),
    });
    let provider = SyncEngine::new(flaky, Duration::from_millis(0), CommitMode::Buffered);

    provider.snapshot().unwrap();

    // Interval 0 signals a probe on every read.
    assert!(eventually(|| {
        let _ = provider.snapshot();
        matches!(provider.sync_status(), SyncStatus::Degraded(_))
    }));
    // A successful probe clears the status even when the fingerprint
    // still matches and no re-fetch is needed.
    assert!(eventually(|| {
        let _ = provider.snapshot();
        provider.sync_status() == SyncStatus::Clean
    }));
}

#[test]
fn refresh_preserves_optimistic_entry() {
    // While a write is queued, a refresh must not downgrade the entry to
    // canonical state — the optimistic view is newer by definition.
    let slow = Arc::new(SlowProvider {
        inner: generic_provider(),
        delay: Duration::from_millis(300),
    });
    let provider = SyncEngine::new(slow, Duration::from_millis(0), CommitMode::Buffered);

    let snap = provider.snapshot().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_tag("pending", at(0)).unwrap();
    provider.commit(logbook, &snap.version).unwrap();

    // Refresh interval of 0 triggers a probe on every read, but pending > 0
    // suppresses it; the optimistic entry stands.
    assert_eq!(provider.snapshot().unwrap().version, "1");
    provider.flush().unwrap();
    assert_eq!(provider.snapshot().unwrap().version, "1");
}
