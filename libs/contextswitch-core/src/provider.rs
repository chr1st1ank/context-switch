//! Composable generic StorageProvider implementation.

use crate::blob::{BlobStore, Precondition};
use crate::crypto::{Cipher, CryptoError};
use crate::domain::Logbook;
use crate::storage::{StorageError, StorageProvider, StorageSnapshot};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Structural envelope problems (bad header, truncated data, unsupported
/// features) are reported as corruption; only an actual AEAD failure —
/// wrong passphrase or tampered ciphertext — is reported as a decryption
/// failure. See the PRD's error taxonomy: "wrong passphrase" must never be
/// reported as corruption, and vice versa.
impl From<CryptoError> for StorageError {
    fn from(e: CryptoError) -> Self {
        match e {
            CryptoError::DecryptionFailed => StorageError::DecryptionFailed,
            CryptoError::InvalidHeader(msg) => StorageError::Corrupt(msg),
            CryptoError::Internal(msg) => StorageError::Corrupt(msg),
        }
    }
}

/// The blob head as last observed by this instance: logbook revision and
/// blob ETag.
///
/// Every mutation follows read → mutate → commit, so the commit almost
/// always targets the head this instance just read. Caching it lets
/// `commit` skip a redundant GET; the blob store's conditional PUT still
/// enforces atomicity, so a stale cache can only produce a `Conflict`,
/// never a lost update.
struct CachedHead {
    /// Opaque version — the logbook revision, `"0"` when the blob is absent.
    version: String,
    /// ETag to precondition the write on; `None` means the blob is absent.
    etag: Option<String>,
}

fn timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("COSW_TIMING").is_some())
}

/// Run `f`, logging its wall time as `cosw-timing <phase>` when the
/// `COSW_TIMING` env var is set. Used to attribute the latency of remote
/// providers across transport and crypto phases.
fn timed<T>(phase: &str, f: impl FnOnce() -> Result<T, StorageError>) -> Result<T, StorageError> {
    let start = Instant::now();
    let result = f();
    if timing_enabled() {
        eprintln!("cosw-timing {phase}: {:?}", start.elapsed());
    }
    result
}

pub struct GenericProvider {
    pub blob_store: Arc<dyn BlobStore>,
    pub cipher: Arc<dyn Cipher>,
    pub passphrase: Option<String>,
    head: Mutex<Option<CachedHead>>,
}

impl GenericProvider {
    pub fn new(
        blob_store: Arc<dyn BlobStore>,
        cipher: Arc<dyn Cipher>,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            blob_store,
            cipher,
            passphrase,
            head: Mutex::new(None),
        }
    }

    /// The cached head, if it still matches `expected_version`.
    fn head_for(&self, expected_version: &str) -> Option<CachedHead> {
        let head = self.head.lock().unwrap();
        match head.as_ref() {
            Some(h) if h.version == expected_version => Some(CachedHead {
                version: h.version.clone(),
                etag: h.etag.clone(),
            }),
            _ => None,
        }
    }

    fn set_head(&self, version: String, etag: Option<String>) {
        *self.head.lock().unwrap() = Some(CachedHead { version, etag });
    }

    fn invalidate_head(&self) {
        *self.head.lock().unwrap() = None;
    }
}

impl StorageProvider for GenericProvider {
    fn fingerprint(&self) -> Result<String, StorageError> {
        timed("blob.stat", || self.blob_store.stat()).map(|t| t.unwrap_or_default())
    }

    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        let got = timed("blob.get", || self.blob_store.get())?;
        let plaintext = match &got {
            Some((bytes, _etag)) => {
                let passphrase = self.passphrase.as_deref().unwrap_or("");
                timed("cipher.open", || {
                    self.cipher
                        .open(bytes, passphrase)
                        .map_err(StorageError::from)
                })?
            }
            // If it's absent, bootstrap as empty v1 logbook.
            None => serde_json::to_string_pretty(&Logbook::new())
                .map_err(|e| StorageError::Corrupt(e.to_string()))?
                .into_bytes(),
        };

        let json_str = std::str::from_utf8(&plaintext).map_err(|_| {
            StorageError::Corrupt("decrypted payload is not valid UTF-8".to_string())
        })?;

        let logbook: Logbook =
            serde_json::from_str(json_str).map_err(|e| StorageError::Corrupt(e.to_string()))?;

        logbook
            .validate()
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;

        let version = logbook.revision.to_string();
        self.set_head(version.clone(), got.as_ref().map(|(_, etag)| etag.clone()));

        // The opaque version string remains the logbook revision, not the ETag.
        Ok(StorageSnapshot { version, logbook })
    }

    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError> {
        logbook.validate()?;

        let passphrase = self.passphrase.as_deref().unwrap_or("");

        // Fast path: the caller commits against the head this instance just
        // read — the normal read → mutate → commit shape — so the GET that
        // would only re-fetch what we already have is skipped. The blob's
        // own conditional write still enforces atomicity; a stale head
        // surfaces as `StorageError::Conflict`, never a lost update.
        let (base_revision, current_etag) = match self.head_for(expected_version) {
            Some(head) => (expected_version.parse::<u64>().unwrap_or(0), head.etag),
            None => {
                let got = timed("blob.get", || self.blob_store.get())?;
                match got {
                    Some((bytes, etag)) => {
                        let plaintext = timed("cipher.open", || {
                            self.cipher
                                .open(&bytes, passphrase)
                                .map_err(StorageError::from)
                        })?;
                        let json_str = std::str::from_utf8(&plaintext).map_err(|_| {
                            StorageError::Corrupt(
                                "decrypted payload is not valid UTF-8".to_string(),
                            )
                        })?;
                        let current: Logbook = serde_json::from_str(json_str)
                            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
                        (current.revision, Some(etag))
                    }
                    None => (0, None),
                }
            }
        };

        let actual_version = base_revision.to_string();
        if actual_version != expected_version {
            return Err(StorageError::Conflict {
                expected: expected_version.to_string(),
                actual: actual_version,
            });
        }

        let mut logbook = logbook;
        logbook.revision = base_revision + 1;

        let json = serde_json::to_string_pretty(&logbook)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;

        let sealed = timed("cipher.seal", || {
            self.cipher
                .seal(json.as_bytes(), passphrase)
                .map_err(StorageError::from)
        })?;

        let cond = match &current_etag {
            Some(etag) => Precondition::IfMatch(etag.clone()),
            None => Precondition::IfAbsent,
        };

        match timed("blob.put", || self.blob_store.put(&sealed, cond)) {
            Ok(new_etag) => {
                self.set_head(logbook.revision.to_string(), Some(new_etag));
                Ok(logbook.revision.to_string())
            }
            Err(e) => {
                // The blob may have changed (conflict) or the write's
                // outcome is unknown (transport error); the next operation
                // must not trust this head.
                self.invalidate_head();
                if let StorageError::Conflict { .. } = e {
                    // Re-read once so the reported `actual` is the revision
                    // that really won — and the head is fresh for a retry.
                    let actual = self
                        .read()
                        .map(|s| s.version)
                        .unwrap_or_else(|_| "unknown".to_string());
                    return Err(StorageError::Conflict {
                        expected: expected_version.to_string(),
                        actual,
                    });
                }
                Err(e)
            }
        }
    }
}
