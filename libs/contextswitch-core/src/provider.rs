//! Composable generic StorageProvider implementation.

use crate::blob::{BlobStore, Precondition};
use crate::crypto::{Cipher, CryptoError, KeyState};
use crate::domain::Document;
use crate::storage::{StorageError, StorageProvider, StorageSnapshot};
use std::sync::Arc;

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
            CryptoError::InvalidParams(msg) => StorageError::Corrupt(msg),
            CryptoError::Internal(msg) => StorageError::Corrupt(msg),
        }
    }
}

pub struct GenericProvider {
    pub blob_store: Arc<dyn BlobStore>,
    pub cipher: Arc<dyn Cipher>,
    pub passphrase: Option<String>,
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
        }
    }
}

impl StorageProvider for GenericProvider {
    fn read(&self) -> Result<StorageSnapshot, StorageError> {
        let got = self.blob_store.get()?;
        let (plaintext, _key_state) = match got {
            Some((bytes, _etag)) => {
                let passphrase = self.passphrase.as_deref().unwrap_or("");
                self.cipher.open(&bytes, passphrase)?
            }
            None => {
                // If it's absent, bootstrap as empty v1 document.
                let doc = Document::new();
                let json = serde_json::to_string_pretty(&doc)
                    .map_err(|e| StorageError::Corrupt(e.to_string()))?;
                let dummy_state = KeyState {
                    active_key_id: "identity".to_string(),
                    keys: vec![],
                };
                (json.into_bytes(), dummy_state)
            }
        };

        let json_str = std::str::from_utf8(&plaintext).map_err(|_| {
            StorageError::Corrupt("decrypted payload is not valid UTF-8".to_string())
        })?;

        let document: Document =
            serde_json::from_str(json_str).map_err(|e| StorageError::Corrupt(e.to_string()))?;

        document
            .validate()
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;

        // The opaque version string remains the document revision, not the ETag.
        Ok(StorageSnapshot {
            version: document.revision.to_string(),
            document,
        })
    }

    fn commit(&self, document: Document, expected_version: &str) -> Result<String, StorageError> {
        document.validate()?;

        let got = self.blob_store.get()?;
        let passphrase = self.passphrase.as_deref().unwrap_or("");

        let (current_doc, current_etag, key_state) = match got {
            Some((bytes, etag)) => {
                let (plaintext, state) = self.cipher.open(&bytes, passphrase)?;
                let json_str = std::str::from_utf8(&plaintext).map_err(|_| {
                    StorageError::Corrupt("decrypted payload is not valid UTF-8".to_string())
                })?;
                let doc: Document = serde_json::from_str(json_str)
                    .map_err(|e| StorageError::Corrupt(e.to_string()))?;
                (Some(doc), Some(etag), Some(state))
            }
            None => (None, None, None),
        };

        let actual_version = current_doc
            .as_ref()
            .map(|d| d.revision.to_string())
            .unwrap_or_else(|| "0".to_string());
        if actual_version != expected_version {
            return Err(StorageError::Conflict {
                expected: expected_version.to_string(),
                actual: actual_version,
            });
        }

        let mut document = document;
        document.revision = current_doc.as_ref().map(|d| d.revision + 1).unwrap_or(1);

        let json = serde_json::to_string_pretty(&document)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;

        let sealed = self
            .cipher
            .seal(json.as_bytes(), passphrase, key_state.as_ref())?;

        let cond = match current_etag {
            Some(etag) => Precondition::IfMatch(etag),
            None => Precondition::IfAbsent,
        };

        self.blob_store.put(&sealed, cond)?;

        Ok(document.revision.to_string())
    }
}
