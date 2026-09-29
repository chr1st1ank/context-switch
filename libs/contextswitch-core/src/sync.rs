//! A replicated local view over any [`StorageProvider`], plus a strict
//! write-through adapter for provider-shaped call sites.
//!
//! [`SyncEngine`] owns an inner provider and keeps a local replica of the
//! canonical snapshot: [`SyncEngine::snapshot`] serves the cached entry,
//! [`SyncEngine::commit`] (in buffered mode) updates the replica and
//! returns the predicted version, and a single background worker drains
//! the write queue in order and revalidates freshness on an interval via
//! [`StorageProvider::fingerprint`]. It is deliberately *not* a
//! `StorageProvider` — a buffered `commit` returning `Ok` does not mean
//! the write is durable, so the sync surface ([`SyncEngine::sync_status`],
//! [`SyncEngine::take_failed_write`], [`SyncEngine::flush`]) is first-class
//! API on the engine instead of a side channel smuggled past the trait.
//!
//! [`MemoryCachingProvider`] is the strict-contract adapter for contexts
//! that need a real `StorageProvider`: a read-through cache with
//! write-through commits that keep the exact provider semantics. It is
//! the honest decorator in this module — the conformance suite runs
//! against it.
//!
//! Staleness is bounded by the refresh interval; correctness is never
//! weakened — the underlying conditional write still rejects stale
//! `expected_version`s. Rejected buffered mutations are preserved for the
//! client (see [`SyncEngine::take_failed_write`]).
//!
//! One assumption is baked into buffered commits: the inner provider
//! assigns versions as `expected + 1` on a numeric revision counter (all
//! current providers do). The optimistic version is only a prediction —
//! but chained queued writes each expect their predecessor's prediction,
//! so a provider with opaque non-numeric versions would drain every
//! chained write into a `Conflict`.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::domain::Logbook;
use crate::storage::{StorageError, StorageProvider, StorageSnapshot};

/// How often a cached snapshot is revalidated in the background.
pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// How many times a buffered commit is retried on `Unavailable` before
/// it is moved to the failed-writes list.
const WRITE_RETRIES: u32 = 2;

/// Base backoff between write retries (attempt n waits `n * RETRY_BACKOFF`).
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// Whether `commit` blocks on the underlying provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitMode {
    /// Delegate synchronously and return real errors (`Conflict`,
    /// `Unavailable`, ...) to the caller. Keeps the exact
    /// [`StorageProvider`] contract — this is the mode
    /// [`MemoryCachingProvider`] uses and the conformance suite runs
    /// against.
    Sync,
    /// Optimistic write-behind: validate in memory, update the replica,
    /// queue the write, and return the predicted version immediately.
    /// `Ok` does not mean durable; failures of the actual write are
    /// reported asynchronously through [`SyncEngine::take_failed_write`] /
    /// [`SyncEngine::sync_status`] / [`SyncEngine::flush`]. Requires the
    /// inner provider to assign `expected + 1` numeric versions.
    Buffered,
}

/// Aggregate state of the write buffer and background revalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// No pending writes and no refresh failure.
    Clean,
    /// This many buffered commits are still queued or being written.
    Pending(usize),
    /// The last background probe failed; the cached snapshot is still
    /// served. Carries the error's display text.
    Degraded(String),
}

/// A buffered commit the underlying provider rejected. The mutation is
/// preserved verbatim so a client can export or retry it.
#[derive(Debug)]
pub struct FailedWrite {
    /// The logbook as it was submitted to `commit`.
    pub logbook: Logbook,
    /// Why the write was rejected.
    pub error: StorageError,
}

struct CachedEntry {
    /// Blob-level change token from the last probe; `None` while the
    /// entry reflects unconfirmed optimistic state or a just-committed
    /// write whose token was not probed yet.
    fingerprint: Option<String>,
    snapshot: StorageSnapshot,
}

struct PendingWrite {
    logbook: Logbook,
    expected_version: String,
}

struct CacheState {
    entry: Option<CachedEntry>,
    last_probe: Option<Instant>,
    /// Commits queued or in flight; reads never probe while this is > 0.
    pending: usize,
    failed: Vec<FailedWrite>,
    last_probe_error: Option<String>,
}

enum Task {
    Commit(PendingWrite),
    Probe,
    Flush(Sender<()>),
    Shutdown,
}

/// A replicated local view over `inner`: a snapshot cache plus
/// (optionally) an optimistic write buffer, reconciled in the background.
pub struct SyncEngine {
    inner: Arc<dyn StorageProvider>,
    state: Arc<Mutex<CacheState>>,
    tx: Sender<Task>,
    worker: Mutex<Option<JoinHandle<()>>>,
    refresh_interval: Duration,
    commit_mode: CommitMode,
}

impl SyncEngine {
    /// Wrap `inner`; commits follow `commit_mode` and cached reads are
    /// revalidated in the background at most every `refresh_interval`.
    /// The worker thread starts immediately and stops on `Drop`.
    pub fn new(
        inner: Arc<dyn StorageProvider>,
        refresh_interval: Duration,
        commit_mode: CommitMode,
    ) -> Self {
        let state = Arc::new(Mutex::new(CacheState {
            entry: None,
            last_probe: None,
            pending: 0,
            failed: Vec::new(),
            last_probe_error: None,
        }));
        let (tx, rx) = channel();
        let worker = {
            let inner = inner.clone();
            let state = state.clone();
            thread::spawn(move || worker_main(inner, state, rx))
        };
        Self {
            inner,
            state,
            tx,
            worker: Mutex::new(Some(worker)),
            refresh_interval,
            commit_mode,
        }
    }

    /// The replicated snapshot. After the first call this serves the
    /// cached entry without touching storage; a background revalidation
    /// is signaled once per `refresh_interval` (never while writes are
    /// queued — the optimistic entry is knowingly ahead of canonical).
    pub fn snapshot(&self) -> Result<StorageSnapshot, StorageError> {
        {
            let mut state = self.state.lock().unwrap();
            if let Some(entry) = &state.entry {
                let snapshot = entry.snapshot.clone();
                let stale = state
                    .last_probe
                    .map(|t| t.elapsed() >= self.refresh_interval)
                    .unwrap_or(true);
                if stale && state.pending == 0 {
                    state.last_probe = Some(Instant::now());
                    let _ = self.tx.send(Task::Probe);
                }
                return Ok(snapshot);
            }
        }

        // Cold path: the only delegating read on the caller's path.
        // Probe before the fetch so the stored token is never newer than
        // the snapshot it labels — a token captured *after* the read could
        // already reflect a concurrent commit and suppress its detection.
        let fingerprint = self.inner.fingerprint().ok();
        let snapshot = self.inner.read()?;
        let mut state = self.state.lock().unwrap();
        state.entry = Some(CachedEntry {
            fingerprint,
            snapshot: snapshot.clone(),
        });
        state.last_probe = Some(Instant::now());
        Ok(snapshot)
    }

    /// Commit against the replica. In `Sync` mode this delegates
    /// synchronously and `Ok` means durable. In `Buffered` mode the
    /// logbook is validated, installed optimistically, and queued; the
    /// returned version is *predicted* (`expected + 1`) and the write is
    /// confirmed asynchronously — see [`SyncEngine::sync_status`].
    pub fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        match self.commit_mode {
            CommitMode::Sync => self.commit_sync(logbook, expected_version),
            CommitMode::Buffered => self.commit_buffered(logbook, expected_version),
        }
    }

    /// Buffered commits queued or in flight. Always 0 in `Sync` mode.
    pub fn pending_writes(&self) -> usize {
        self.state.lock().unwrap().pending
    }

    /// The oldest buffered write the inner provider rejected, if any.
    /// Consumes it — call repeatedly to drain the failed-writes list.
    pub fn take_failed_write(&self) -> Option<FailedWrite> {
        let mut state = self.state.lock().unwrap();
        if state.failed.is_empty() {
            None
        } else {
            Some(state.failed.remove(0))
        }
    }

    /// Snapshot of buffer/revalidation state for status displays.
    pub fn sync_status(&self) -> SyncStatus {
        let state = self.state.lock().unwrap();
        if let Some(e) = &state.last_probe_error {
            return SyncStatus::Degraded(e.clone());
        }
        if state.pending > 0 {
            return SyncStatus::Pending(state.pending);
        }
        SyncStatus::Clean
    }

    /// Wait until all commits queued so far have been written (or
    /// rejected). `Ok(Some(failed))` carries the earliest rejected write —
    /// consumed from the list — so the caller can preserve or retry it;
    /// `Ok(None)` means every queued write landed. Intended for lifecycle
    /// hooks (app pause/exit), not steady-state calls.
    pub fn flush(&self) -> Result<Option<FailedWrite>, StorageError> {
        let (tx, rx) = channel();
        self.tx
            .send(Task::Flush(tx))
            .map_err(|_| StorageError::Unavailable("sync worker stopped".to_string()))?;
        rx.recv()
            .map_err(|_| StorageError::Unavailable("sync worker stopped".to_string()))?;
        Ok(self.take_failed_write())
    }

    fn commit_sync(
        &self,
        logbook: Logbook,
        expected_version: &str,
    ) -> Result<String, StorageError> {
        match self.inner.commit(logbook.clone(), expected_version) {
            Ok(version) => {
                let mut committed = logbook;
                committed.revision = version.parse().unwrap_or(committed.revision);
                let mut state = self.state.lock().unwrap();
                // No fingerprint is stamped: a post-commit probe could
                // observe a concurrent write's token and mislabel this
                // entry. The next interval probe re-fetches once and
                // replaces the entry with confirmed canonical data.
                state.entry = Some(CachedEntry {
                    fingerprint: None,
                    snapshot: StorageSnapshot {
                        version: version.clone(),
                        logbook: committed,
                    },
                });
                state.last_probe = Some(Instant::now());
                Ok(version)
            }
            Err(e) => {
                // The stored head may have moved (conflict) or the write's
                // outcome is unknown; don't trust the cached snapshot.
                self.state.lock().unwrap().entry = None;
                Err(e)
            }
        }
    }

    fn commit_buffered(
        &self,
        mut logbook: Logbook,
        expected_version: &str,
    ) -> Result<String, StorageError> {
        logbook.validate()?;

        let mut state = self.state.lock().unwrap();
        if let Some(entry) = &state.entry {
            if entry.snapshot.version != expected_version {
                // The cache already knows a newer version — this commit is
                // provably stale with zero I/O, so it is still reported
                // synchronously instead of landing in the queue.
                return Err(StorageError::Conflict {
                    expected: expected_version.to_string(),
                    actual: entry.snapshot.version.clone(),
                });
            }
        }

        let predicted = next_version(expected_version);
        logbook.revision = predicted.parse().unwrap_or(logbook.revision);
        state.entry = Some(CachedEntry {
            fingerprint: None,
            snapshot: StorageSnapshot {
                version: predicted.clone(),
                logbook: logbook.clone(),
            },
        });
        state.pending += 1;

        if self
            .tx
            .send(Task::Commit(PendingWrite {
                logbook,
                expected_version: expected_version.to_string(),
            }))
            .is_err()
        {
            // The worker is gone: roll back the optimistic entry so reads
            // repopulate from canonical data instead of serving a write
            // that will never happen.
            state.pending -= 1;
            if state.entry.as_ref().map(|e| e.snapshot.version.as_str()) == Some(predicted.as_str())
            {
                state.entry = None;
            }
            return Err(StorageError::Unavailable("sync worker stopped".to_string()));
        }
        Ok(predicted)
    }
}

impl Drop for SyncEngine {
    fn drop(&mut self) {
        let _ = self.tx.send(Task::Shutdown);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

/// A strict `StorageProvider` decorator: read-through snapshot cache plus
/// write-through commits, keeping the exact provider contract. Unlike
/// [`SyncEngine`] in `Buffered` mode, `commit` only returns `Ok` once the
/// inner provider has durably accepted the write.
pub struct MemoryCachingProvider {
    engine: SyncEngine,
}

impl MemoryCachingProvider {
    /// Wrap `inner` with a read-through cache revalidated in the
    /// background at most every `refresh_interval`.
    pub fn new(inner: Arc<dyn StorageProvider>, refresh_interval: Duration) -> Self {
        Self {
            engine: SyncEngine::new(inner, refresh_interval, CommitMode::Sync),
        }
    }
}

impl StorageProvider for MemoryCachingProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.engine.snapshot()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        self.engine.commit(logbook, expected_version)
    }

    /// The token describes canonical storage, not the cached view — a
    /// layer stacked on top must observe real changes, so this delegates
    /// straight to the inner provider.
    fn fingerprint(&self) -> Result<String, StorageError> {
        self.engine.inner.fingerprint()
    }
}

fn worker_main(inner: Arc<dyn StorageProvider>, state: Arc<Mutex<CacheState>>, rx: Receiver<Task>) {
    while let Ok(task) = rx.recv() {
        match task {
            Task::Commit(write) => drain_commit(&inner, &state, write),
            Task::Probe => refresh(&inner, &state),
            Task::Flush(ack) => {
                // FIFO: every commit queued before the barrier has
                // been attempted by now.
                let _ = ack.send(());
            }
            Task::Shutdown => break,
        }
    }
}

/// One queued write, retried a bounded number of times on `Unavailable`.
/// All other errors are final immediately; conflicts always are.
fn drain_commit(
    inner: &Arc<dyn StorageProvider>,
    state: &Arc<Mutex<CacheState>>,
    write: PendingWrite,
) {
    let mut attempts = 0u32;
    let result = loop {
        match inner.commit(write.logbook.clone(), &write.expected_version) {
            Err(e @ StorageError::Unavailable(_)) if attempts < WRITE_RETRIES => {
                attempts += 1;
                let _ = e;
                thread::sleep(RETRY_BACKOFF * attempts);
                continue;
            }
            r => break r,
        }
    };

    let reconcile = {
        let mut state = state.lock().unwrap();
        state.pending = state.pending.saturating_sub(1);
        match result {
            // The optimistic entry already shows this write (or a newer
            // one); it stays fingerprint-less until the next probe swaps
            // it for confirmed canonical data.
            Ok(_) => false,
            Err(e) => {
                // If the optimistic entry still shows this write's
                // predicted version, drop it so the next read repopulates
                // from canonical data. A newer optimistic entry (a later
                // queued write) is left alone — it resolves on its own.
                let predicted = next_version(&write.expected_version);
                if state.entry.as_ref().map(|e| e.snapshot.version.as_str())
                    == Some(predicted.as_str())
                {
                    state.entry = None;
                }
                let is_conflict = matches!(e, StorageError::Conflict { .. });
                state.failed.push(FailedWrite {
                    logbook: write.logbook,
                    error: e,
                });
                is_conflict
            }
        }
    };

    // A conflict means another writer moved canonical data: reconcile
    // right away instead of waiting out the refresh interval.
    if reconcile {
        refresh(inner, state);
    }
}

/// Compare the blob's change token and re-fetch the logbook only when it
/// moved. Installs are monotonic: a refresh never overwrites a newer
/// (possibly optimistic) entry.
fn refresh(inner: &Arc<dyn StorageProvider>, state: &Arc<Mutex<CacheState>>) {
    let fingerprint = match inner.fingerprint() {
        Ok(fp) => fp,
        Err(e) => {
            state.lock().unwrap().last_probe_error = Some(e.to_string());
            return;
        }
    };

    {
        let mut state = state.lock().unwrap();
        // The probe succeeded; a past failure is recovered even when the
        // token still matches and no re-fetch is needed.
        state.last_probe_error = None;
        if let Some(entry) = &state.entry {
            if entry.fingerprint.as_deref() == Some(fingerprint.as_str()) {
                return;
            }
        }
    }

    match inner.read() {
        Ok(snapshot) => {
            let mut state = state.lock().unwrap();
            let install = match &state.entry {
                None => true,
                Some(entry) => newer_or_equal(&snapshot.version, &entry.snapshot.version),
            };
            if install {
                state.entry = Some(CachedEntry {
                    fingerprint: Some(fingerprint),
                    snapshot,
                });
            }
        }
        Err(e) => {
            state.lock().unwrap().last_probe_error = Some(e.to_string());
        }
    }
}

fn next_version(version: &str) -> String {
    version
        .parse::<u64>()
        .map(|v| (v + 1).to_string())
        .unwrap_or_else(|_| version.to_string())
}

fn newer_or_equal(new: &str, old: &str) -> bool {
    match (new.parse::<u64>(), old.parse::<u64>()) {
        (Ok(n), Ok(o)) => n >= o,
        _ => new != old,
    }
}
