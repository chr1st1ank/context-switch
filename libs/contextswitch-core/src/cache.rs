//! Read-through snapshot cache and write buffer wrapping any
//! [`StorageProvider`].
//!
//! [`MemoryCachingProvider`] is itself a `StorageProvider`, so it can be
//! wrapped around any provider at startup — or left out entirely. Neither
//! reads nor commits block the caller on storage: `read()` serves the
//! cached snapshot, `commit()` (in buffered mode) updates the cache and
//! returns the predicted version, and a single background worker drains
//! the write queue in order and revalidates freshness on an interval via
//! [`StorageProvider::fingerprint`].
//!
//! Staleness is bounded by the refresh interval; correctness is never
//! weakened — the underlying conditional write still rejects stale
//! `expected_version`s. Rejected buffered mutations are preserved for the
//! client (see [`MemoryCachingProvider::take_failed_write`]).

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
    /// [`StorageProvider`] contract — used by clients that surface commit
    /// failures immediately and by the conformance suite.
    Sync,
    /// Optimistic write-behind: validate in memory, update the cache,
    /// queue the write, and return the predicted version immediately.
    /// Failures of the actual write are reported asynchronously through
    /// [`MemoryCachingProvider::take_failed_write`] /
    /// [`MemoryCachingProvider::sync_status`] /
    /// [`MemoryCachingProvider::flush`].
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
    /// Blob-level change token from the last probe or confirmed commit;
    /// `None` while the entry reflects an unconfirmed optimistic write.
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

/// A [`StorageProvider`] decorator adding a read-through snapshot cache
/// plus (optionally) an optimistic write buffer on top of `inner`.
pub struct MemoryCachingProvider {
    inner: Arc<dyn StorageProvider>,
    state: Arc<Mutex<CacheState>>,
    tx: Sender<Task>,
    worker: Mutex<Option<JoinHandle<()>>>,
    refresh_interval: Duration,
    commit_mode: CommitMode,
}

impl MemoryCachingProvider {
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
    /// rejected). Returns the earliest recorded write failure, consuming
    /// it; `Ok(())` means every queued write landed. Intended for
    /// lifecycle hooks (app pause/exit), not steady-state calls.
    pub fn flush(&self) -> Result<(), StorageError> {
        let (tx, rx) = channel();
        self.tx
            .send(Task::Flush(tx))
            .map_err(|_| StorageError::Unavailable("cache worker stopped".to_string()))?;
        rx.recv()
            .map_err(|_| StorageError::Unavailable("cache worker stopped".to_string()))?;
        match self.take_failed_write() {
            Some(failed) => Err(failed.error),
            None => Ok(()),
        }
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
                let fingerprint = self.inner.fingerprint().ok();
                let mut state = self.state.lock().unwrap();
                state.entry = Some(CachedEntry {
                    fingerprint,
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
            state.pending -= 1;
            return Err(StorageError::Unavailable(
                "cache worker stopped".to_string(),
            ));
        }
        Ok(predicted)
    }
}

impl StorageProvider for MemoryCachingProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        {
            let mut state = self.state.lock().unwrap();
            if let Some(entry) = &state.entry {
                let snapshot = entry.snapshot.clone();
                // Signal a background refresh once per interval, but never
                // while writes are queued — the optimistic entry is
                // knowingly ahead of canonical until they drain.
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
        let snapshot = self.inner.read()?;
        let fingerprint = self.inner.fingerprint().ok();
        let mut state = self.state.lock().unwrap();
        state.entry = Some(CachedEntry {
            fingerprint,
            snapshot: snapshot.clone(),
        });
        state.last_probe = Some(Instant::now());
        Ok(snapshot)
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        match self.commit_mode {
            CommitMode::Sync => self.commit_sync(logbook, expected_version),
            CommitMode::Buffered => self.commit_buffered(logbook, expected_version),
        }
    }
}

impl Drop for MemoryCachingProvider {
    fn drop(&mut self) {
        let _ = self.tx.send(Task::Shutdown);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
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

    // Learn the confirmed blob token outside the state lock; a failed
    // commit leaves it unknown either way.
    let fingerprint = if result.is_ok() {
        inner.fingerprint().ok()
    } else {
        None
    };

    let reconcile = {
        let mut state = state.lock().unwrap();
        state.pending = state.pending.saturating_sub(1);
        match result {
            Ok(version) => {
                // Only stamp the token when the entry still reflects this
                // write — a later queued commit may already have advanced it.
                if let Some(entry) = &mut state.entry {
                    if entry.snapshot.version == version {
                        entry.fingerprint = fingerprint;
                    }
                }
                false
            }
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
        let state = state.lock().unwrap();
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
            state.last_probe_error = None;
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
