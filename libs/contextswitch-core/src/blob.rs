//! BlobStore trait and standard implementations.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::storage::StorageError;

/// Preconditions for conditional writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// Create the object only if it is absent.
    IfAbsent,
    /// Compare-and-swap based on an ETag / revision version.
    IfMatch(String),
}

/// The blob-level storage abstraction.
pub trait BlobStore: Send + Sync {
    /// Retrieve the payload and its current ETag / version, if present.
    fn get(&self) -> Result<Option<(Vec<u8>, String)>, StorageError>;

    /// Put payload conditionally. Returns the new ETag / version.
    fn put(&self, bytes: &[u8], cond: Precondition) -> Result<String, StorageError>;

    /// Opaque change token for the stored blob, without fetching its body;
    /// `None` when the blob is absent. Cheap backends override this (HEAD
    /// request, file metadata); the default pays for a full `get`.
    fn stat(&self) -> Result<Option<String>, StorageError> {
        Ok(self.get()?.map(|(_, etag)| etag))
    }
}

/// In-memory fake blob store for testing concurrency and encryption offline.
pub struct InMemoryBlobStore {
    // Mutex allows interior mutability for thread safety in tests.
    data: std::sync::Mutex<Option<(Vec<u8>, String)>>,
    // Generates simple sequential revision ETags.
    next_revision: std::sync::Mutex<u64>,
}

impl InMemoryBlobStore {
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(None),
            next_revision: std::sync::Mutex::new(1),
        }
    }
}

impl Default for InMemoryBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobStore for InMemoryBlobStore {
    fn get(&self) -> Result<Option<(Vec<u8>, String)>, StorageError> {
        Ok(self.data.lock().unwrap().clone())
    }

    fn stat(&self) -> Result<Option<String>, StorageError> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .as_ref()
            .map(|(_, etag)| etag.clone()))
    }

    fn put(&self, bytes: &[u8], cond: Precondition) -> Result<String, StorageError> {
        let mut data_guard = self.data.lock().unwrap();
        let current_etag = data_guard.as_ref().map(|(_, etag)| etag.clone());

        match cond {
            Precondition::IfAbsent => {
                if current_etag.is_some() {
                    return Err(StorageError::Conflict {
                        expected: "None".to_string(),
                        actual: current_etag.unwrap_or_default(),
                    });
                }
            }
            Precondition::IfMatch(expected) => {
                let actual = current_etag.clone().unwrap_or_else(|| "0".to_string());
                if actual != expected {
                    return Err(StorageError::Conflict { expected, actual });
                }
            }
        }

        let mut rev_guard = self.next_revision.lock().unwrap();
        let new_etag = rev_guard.to_string();
        *rev_guard += 1;

        *data_guard = Some((bytes.to_vec(), new_etag.clone()));
        Ok(new_etag)
    }
}

/// Local filesystem blob store.
pub struct LocalFsBlobStore {
    path: PathBuf,
    lock_timeout: Duration,
    stale_lock_age: Duration,
}

impl LocalFsBlobStore {
    pub fn new(
        path: PathBuf,
        lock_timeout: Duration,
        stale_lock_age: Duration,
    ) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            path,
            lock_timeout,
            stale_lock_age,
        })
    }

    fn lock_path(&self) -> PathBuf {
        crate::storage::sibling_path(&self.path, "lock")
    }

    fn tmp_path(&self) -> PathBuf {
        crate::storage::sibling_path(&self.path, "tmp")
    }

    fn is_stale(&self, lock_path: &Path) -> bool {
        fs::metadata(lock_path)
            .and_then(|m| m.modified())
            .map(|modified| modified.elapsed().unwrap_or_default() > self.stale_lock_age)
            .unwrap_or(false)
    }

    /// Acquire the filesystem lock.
    fn acquire_lock(&self) -> Result<crate::storage::LockGuard, StorageError> {
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
                    return Ok(crate::storage::LockGuard(lock_path));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if self.is_stale(&lock_path) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(StorageError::Locked);
                    }
                    thread::sleep(crate::storage::LOCK_RETRY_INTERVAL);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl BlobStore for LocalFsBlobStore {
    fn stat(&self) -> Result<Option<String>, StorageError> {
        match fs::metadata(&self.path) {
            Ok(meta) => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                Ok(Some(format!("{}:{}", mtime, meta.len())))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn get(&self) -> Result<Option<(Vec<u8>, String)>, StorageError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path)?;
        // We can synthesize a simple etag or revision.
        // For compatibility with current conformance/tests which expect revision counter to be version,
        // let's parse the logbook revision or derive from the file contents.
        // Since we are decoupling, the file contains the serialized bytes (which could be plaintext or encrypted).
        // Let's generate a version/etag based on the bytes hash or read revision if plaintext.
        // Wait, if it's plaintext JSON, we can extract the revision from the JSON!
        // If it is encrypted envelope, the version can be parsed from the encrypted logbook, or we can use a hash.
        // Actually, the PRD says:
        // "Local storage synthesizes its ETag as a content hash compared under the existing lockfile, giving both backends identical precondition semantics."
        // And:
        // "The opaque version string remains the logbook revision, not the ETag. The ETag is used internally for compare-and-swap only. This keeps one version semantics across backends, keeps conflict messages human-meaningful, keeps the existing conformance suite intact as a genuine cross-provider contract..."
        // This is a beautiful distinction! Let's compute a hash of the content for the ETag.
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut hasher, &bytes);
        let etag = hex::encode(sha2::Digest::finalize(hasher));
        Ok(Some((bytes, etag)))
    }

    fn put(&self, bytes: &[u8], cond: Precondition) -> Result<String, StorageError> {
        // Acquire lock first to ensure atomicity.
        let _guard = self.acquire_lock()?;

        let current = self.get()?;
        let current_etag = current.as_ref().map(|(_, etag)| etag.clone());

        match cond {
            Precondition::IfAbsent => {
                if current_etag.is_some() {
                    return Err(StorageError::Conflict {
                        expected: "None".to_string(),
                        actual: current_etag.unwrap_or_default(),
                    });
                }
            }
            Precondition::IfMatch(expected) => {
                let actual = current_etag.clone().unwrap_or_else(|| "0".to_string());
                if actual != expected {
                    return Err(StorageError::Conflict { expected, actual });
                }
            }
        }

        // Write atomic using tmp file and rename.
        let tmp_path = self.tmp_path();
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.path)?;
        if let Some(dir) = self.path.parent() {
            if let Ok(dir) = File::open(dir) {
                let _ = dir.sync_all();
            }
        }

        // Re-read or calculate ETag
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut hasher, bytes);
        let etag = hex::encode(sha2::Digest::finalize(hasher));
        Ok(etag)
    }
}
