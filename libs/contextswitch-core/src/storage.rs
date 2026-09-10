//! Storage provider interface and the local filesystem implementation.
//!
//! The contract is deliberately small: [`StorageProvider::read`] returns a
//! snapshot plus an opaque version, and [`StorageProvider::commit`] writes a
//! document only if the stored version still equals the observed one. That
//! conditional write is what enforces the one-active-timer invariant and
//! prevents silent stale overwrites — callers read, apply one focused
//! mutation, and commit against the version they saw.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use pyo3::prelude::*;
use thiserror::Error;

use crate::domain::{Document, DomainError};
use crate::exceptions;

/// How long a commit waits for the lock before giving up.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay between lock acquisition attempts.
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(50);
/// A lockfile older than this is treated as abandoned and reclaimed.
/// Commits take microseconds, so 30 s is far beyond any real write.
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);

/// Errors produced by storage providers.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Underlying I/O failure (missing file, permission denied, ...).
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The stored bytes are not a valid, consistent document.
    #[error("corrupt document: {0}")]
    Corrupt(String),
    /// The stored version no longer matches the version the caller wrote
    /// against — another commit won the race.
    #[error("write conflict: expected version {expected}, current version is {actual}")]
    Conflict { expected: String, actual: String },
    /// The provider lock could not be acquired in time.
    #[error("storage is locked by another process")]
    Locked,
    /// The document a caller tried to commit violates a domain invariant.
    #[error("invalid document: {0}")]
    InvalidData(#[from] DomainError),
}

impl From<StorageError> for PyErr {
    fn from(e: StorageError) -> PyErr {
        exceptions::StorageError::new_err(e.to_string())
    }
}

/// A read of canonical data together with the version it was observed at.
/// Pass `version` back to [`StorageProvider::commit`] for a conditional write.
#[pyclass]
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    /// Opaque version string; a document revision for local files, an ETag
    /// for remote object storage.
    #[pyo3(get)]
    pub version: String,
    #[pyo3(get)]
    pub document: Document,
}

/// The common contract for synchronized storage.
///
/// Implementations must guarantee that `commit` is atomic: the version check
/// and the write are serialized so that a stale `expected_version` can never
/// silently overwrite a newer document.
pub trait StorageProvider: Send + Sync {
    /// Read the canonical document and its current version.
    fn read(&self) -> Result<StorageSnapshot, StorageError>;

    /// Conditionally write `document` if the stored version still equals
    /// `expected_version`. Returns the new version on success.
    fn commit(&self, document: Document, expected_version: &str) -> Result<String, StorageError>;
}

/// Removes the lockfile when the guard goes out of scope.
struct LockGuard(PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Canonical storage as a single JSON document on the local filesystem.
///
/// Commits serialize through a sibling `<file>.lock` lockfile created with
/// `create_new` (atomic on all platforms), then write `<file>.tmp` and
/// atomically rename it over the data file. The version check happens under
/// the lock, so a stale commit always fails with [`StorageError::Conflict`].
/// A lockfile abandoned by a crashed process is reclaimed after
/// [`STALE_LOCK_AGE`].
#[pyclass]
pub struct LocalFsProvider {
    path: PathBuf,
    lock_timeout: Duration,
    stale_lock_age: Duration,
}

impl LocalFsProvider {
    /// Open the document at `path`, creating an empty v1 document (and any
    /// missing parent directories) if it does not exist.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::with_options(path, LOCK_TIMEOUT, STALE_LOCK_AGE)
    }

    /// Like [`LocalFsProvider::new`] with explicit lock tuning; used by tests.
    pub fn with_options(
        path: impl AsRef<Path>,
        lock_timeout: Duration,
        stale_lock_age: Duration,
    ) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // create_new avoids clobbering a document a concurrent init just wrote.
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let json = serde_json::to_string_pretty(&Document::new())
                    .map_err(|e| StorageError::Corrupt(e.to_string()))?;
                file.write_all(json.as_bytes())?;
                file.sync_all()?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        Ok(Self {
            path,
            lock_timeout,
            stale_lock_age,
        })
    }

    /// The data file this provider manages.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_path(&self) -> PathBuf {
        sibling_path(&self.path, "lock")
    }

    fn tmp_path(&self) -> PathBuf {
        sibling_path(&self.path, "tmp")
    }

    /// Acquire the commit lock, waiting up to `lock_timeout` and reclaiming
    /// lockfiles older than `stale_lock_age`.
    fn acquire_lock(&self) -> Result<LockGuard, StorageError> {
        let lock_path = self.lock_path();
        let deadline = Instant::now() + self.lock_timeout;
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "pid={}", std::process::id());
                    return Ok(LockGuard(lock_path));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if self.is_stale(&lock_path) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(StorageError::Locked);
                    }
                    thread::sleep(LOCK_RETRY_INTERVAL);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn is_stale(&self, lock_path: &Path) -> bool {
        fs::metadata(lock_path)
            .and_then(|m| m.modified())
            .map(|modified| modified.elapsed().unwrap_or_default() > self.stale_lock_age)
            .unwrap_or(false)
    }

    /// Serialize `document` and atomically replace the data file.
    fn write_atomic(&self, document: &Document) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(document)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        let tmp_path = self.tmp_path();
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        // Flush the directory entry so the rename survives a crash.
        if let Some(dir) = self.path.parent() {
            if let Ok(dir) = File::open(dir) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }
}

/// `data.json` + `"lock"` → `data.json.lock`.
fn sibling_path(path: &Path, extension: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{extension}"));
    path.with_file_name(name)
}

impl StorageProvider for LocalFsProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        let json = fs::read_to_string(&self.path)?;
        let document: Document =
            serde_json::from_str(&json).map_err(|e| StorageError::Corrupt(e.to_string()))?;
        // A document that fails domain validation is treated as corrupt:
        // never silently repair canonical data.
        document
            .validate()
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        Ok(StorageSnapshot {
            version: document.revision.to_string(),
            document,
        })
    }

    fn commit(&self, document: Document, expected_version: &str) -> Result<String, StorageError> {
        // Reject documents violating domain invariants before taking the lock.
        document.validate()?;
        let _guard = self.acquire_lock()?;
        let current = self.read()?;
        if current.version != expected_version {
            return Err(StorageError::Conflict {
                expected: expected_version.to_string(),
                actual: current.version,
            });
        }
        // The provider owns the revision counter.
        let mut document = document;
        document.revision = current.document.revision + 1;
        self.write_atomic(&document)?;
        Ok(document.revision.to_string())
    }
}

#[pymethods]
impl LocalFsProvider {
    #[new]
    fn py_new(path: PathBuf) -> PyResult<Self> {
        Ok(Self::new(path)?)
    }

    #[getter(path)]
    fn py_path(&self) -> String {
        self.path.display().to_string()
    }

    #[pyo3(name = "read")]
    fn py_read(&self) -> PyResult<StorageSnapshot> {
        Ok(<Self as StorageProvider>::read(self)?)
    }

    #[pyo3(name = "commit", signature = (document, expected_version))]
    fn py_commit(&self, document: Document, expected_version: String) -> PyResult<String> {
        Ok(<Self as StorageProvider>::commit(
            self,
            document,
            &expected_version,
        )?)
    }
}
