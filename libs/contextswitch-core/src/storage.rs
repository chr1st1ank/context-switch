//! Storage provider interface and the local filesystem implementation.
//!
//! The contract is deliberately small: [`StorageProvider::read`] returns a
//! snapshot plus an opaque version, and [`StorageProvider::commit`] writes a
//! logbook only if the stored version still equals the observed one. That
//! conditional write is what enforces the one-active-timer invariant and
//! prevents silent stale overwrites — callers read, apply one focused
//! mutation, and commit against the version they saw.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "python")]
use pyo3::prelude::*;
use thiserror::Error;

use crate::blob::{BlobStore, LocalFsBlobStore};
#[cfg(feature = "python")]
use crate::crypto::EnvelopeCipher;
use crate::crypto::IdentityCipher;
use crate::domain::{DomainError, Logbook};
#[cfg(feature = "python")]
use crate::exceptions;
use crate::provider::GenericProvider;
#[cfg(feature = "python")]
use crate::s3::S3BlobStore;

/// How long a commit waits for the lock before giving up.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay between lock acquisition attempts.
pub(crate) const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(50);
/// A lockfile older than this is treated as abandoned and reclaimed.
/// Commits take microseconds, so 30 s is far beyond any real write.
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);

/// Errors produced by storage providers.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Underlying I/O failure (missing file, permission denied, ...).
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    /// The stored bytes are not a valid, consistent logbook.
    #[error("corrupt logbook file: {0}")]
    Corrupt(String),
    /// The stored version no longer matches the version the caller wrote
    /// against — another commit won the race.
    #[error("write conflict: expected version {expected}, current version is {actual}")]
    Conflict { expected: String, actual: String },
    /// The provider lock could not be acquired in time.
    #[error("storage is locked by another process")]
    Locked,
    /// The logbook a caller tried to commit violates a domain invariant.
    #[error("invalid logbook: {0}")]
    InvalidData(#[from] DomainError),
    /// Decryption failed.
    #[error("decryption failed (wrong passphrase or tampered data)")]
    DecryptionFailed,
    /// Unauthorized or credential error.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// S3 or remote storage unavailable / network failure.
    #[error("storage unavailable: {0}")]
    Unavailable(String),
}

#[cfg(feature = "python")]
impl From<StorageError> for PyErr {
    fn from(e: StorageError) -> PyErr {
        exceptions::StorageError::new_err(e.to_string())
    }
}

/// A read of canonical data together with the version it was observed at.
/// Pass `version` back to [`StorageProvider::commit`] for a conditional write.
#[cfg_attr(feature = "python", pyclass)]
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    /// Opaque version string; a logbook revision for local files, an ETag
    /// for remote object storage.
    pub version: String,
    pub logbook: Logbook,
}

#[cfg(feature = "python")]
#[cfg_attr(feature = "python", pymethods)]
impl StorageSnapshot {
    #[getter]
    fn version(&self) -> String {
        self.version.clone()
    }

    #[getter]
    fn logbook(&self) -> Logbook {
        self.logbook.clone()
    }
}

/// The common contract for synchronized storage.
///
/// Implementations must guarantee that `commit` is atomic: the version check
/// and the write are serialized so that a stale `expected_version` can never
/// silently overwrite a newer logbook.
pub trait StorageProvider: Send + Sync {
    /// Read the canonical logbook and its current version.
    fn read(&self) -> Result<StorageSnapshot, StorageError>;

    /// Conditionally write `logbook` if the stored version still equals
    /// `expected_version`. Returns the new version on success.
    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError>;
}

/// Removes the lockfile when the guard goes out of scope.
pub struct LockGuard(pub(crate) PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Canonical storage as a single JSON logbook on the local filesystem.
///
/// Commits serialize through a sibling `<file>.lock` lockfile created with
/// `create_new` (atomic on all platforms), then write `<file>.tmp` and
/// atomically rename it over the data file. The version check happens under
/// the lock, so a stale commit always fails with [`StorageError::Conflict`].
/// A lockfile abandoned by a crashed process is reclaimed after
/// [`STALE_LOCK_AGE`].
#[cfg_attr(feature = "python", pyclass)]
pub struct LocalFsProvider {
    inner: GenericProvider,
    path: PathBuf,
}

impl LocalFsProvider {
    /// Open the logbook at `path`, creating an empty v1 logbook (and any
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
        let blob_store = Arc::new(LocalFsBlobStore::new(
            path.clone(),
            lock_timeout,
            stale_lock_age,
        )?);

        // Bootstrap an empty v1 logbook if none exists yet. Two processes
        // racing to open the same fresh path may both observe absence and
        // both attempt this; the loser's `IfAbsent` put fails with
        // `Conflict`, which is not a real failure here — the file exists
        // either way once one of them wins, so it is swallowed rather than
        // surfaced as an error opening the provider.
        if !path.exists() {
            let logbook = Logbook::new();
            let json = serde_json::to_string_pretty(&logbook)
                .map_err(|e| StorageError::Corrupt(e.to_string()))?;
            match blob_store.put(json.as_bytes(), crate::blob::Precondition::IfAbsent) {
                Ok(_) | Err(StorageError::Conflict { .. }) => {}
                Err(e) => return Err(e),
            }
        }

        let inner = GenericProvider::new(blob_store, Arc::new(IdentityCipher), None);

        Ok(Self { inner, path })
    }

    /// The data file this provider manages.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl StorageProvider for LocalFsProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        self.inner.commit(logbook, expected_version)
    }
}

#[cfg(feature = "python")]
#[cfg_attr(feature = "python", pymethods)]
impl LocalFsProvider {
    #[new]
    fn py_new(path: PathBuf) -> PyResult<Self> {
        Ok(Self::new(path)?)
    }

    #[getter(path)]
    fn py_path(&self) -> String {
        self.path.display().to_string()
    }

    /// Storage location as a `file://` URI, mirroring `S3Provider.location_url`
    /// so callers (e.g. `cosw status --verbose`) don't need to duck-type on
    /// which provider they were handed.
    #[getter(location_url)]
    fn py_location_url(&self) -> String {
        let absolute = fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        format!("file://{}", absolute.display())
    }

    #[pyo3(name = "read")]
    fn py_read(&self) -> PyResult<StorageSnapshot> {
        Ok(<Self as StorageProvider>::read(self)?)
    }

    #[pyo3(name = "commit", signature = (logbook, expected_version))]
    fn py_commit(&self, logbook: Logbook, expected_version: String) -> PyResult<String> {
        Ok(<Self as StorageProvider>::commit(
            self,
            logbook,
            &expected_version,
        )?)
    }
}

/// `logbook.json` + `"lock"` → `logbook.json.lock`.
pub fn sibling_path(path: &Path, extension: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{extension}"));
    path.with_file_name(name)
}

/// S3-compatible storage provider with mandatory client-side envelope
/// encryption. Its constructor sources credentials from the AWS-standard
/// mechanisms (env vars / `~/.aws/credentials`), so it is only exported
/// through the Python bindings; other FFI consumers compose
/// [`GenericProvider`](crate::provider::GenericProvider) with
/// [`S3BlobStore`](crate::s3::S3BlobStore)`::with_credentials` directly.
#[cfg(feature = "python")]
#[cfg_attr(feature = "python", pyclass)]
pub struct S3Provider {
    inner: GenericProvider,
    location_url: String,
}

#[cfg(feature = "python")]
#[cfg_attr(feature = "python", pymethods)]
impl S3Provider {
    #[new]
    #[pyo3(signature = (bucket, region, prefix, passphrase, endpoint=None, use_path_style=false, profile=None))]
    fn py_new(
        bucket: String,
        region: String,
        prefix: String,
        passphrase: String,
        endpoint: Option<String>,
        use_path_style: bool,
        profile: Option<String>,
    ) -> PyResult<Self> {
        let blob_store = Arc::new(S3BlobStore::new(
            bucket.clone(),
            region.clone(),
            prefix.clone(),
            endpoint,
            use_path_style,
            profile,
        ));

        let cipher = Arc::new(EnvelopeCipher::new());
        let inner = GenericProvider::new(blob_store, cipher, Some(passphrase));

        let file_key = if prefix.is_empty() {
            "logbook.json".to_string()
        } else if prefix.ends_with('/') {
            format!("{}logbook.json", prefix)
        } else {
            format!("{}/logbook.json", prefix)
        };
        let location_url = format!("s3://{}/{}", bucket, file_key);

        Ok(Self {
            inner,
            location_url,
        })
    }

    #[getter(location_url)]
    fn py_location_url(&self) -> String {
        self.location_url.clone()
    }

    #[pyo3(name = "read")]
    fn py_read(&self) -> PyResult<StorageSnapshot> {
        Ok(<Self as StorageProvider>::read(self)?)
    }

    #[pyo3(name = "commit", signature = (logbook, expected_version))]
    fn py_commit(&self, logbook: Logbook, expected_version: String) -> PyResult<String> {
        Ok(<Self as StorageProvider>::commit(
            self,
            logbook,
            &expected_version,
        )?)
    }
}

#[cfg(feature = "python")]
impl StorageProvider for S3Provider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        self.inner.read()
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        self.inner.commit(logbook, expected_version)
    }
}
