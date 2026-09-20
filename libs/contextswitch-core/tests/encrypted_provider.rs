//! Conformance and behavior tests for the client-side encrypted storage provider.

use chrono::{DateTime, Utc};
use std::sync::Arc;

use contextswitch_core::blob::{BlobStore, InMemoryBlobStore};
use contextswitch_core::conformance::check_provider_conformance;
use contextswitch_core::crypto::{Cipher, CryptoError, EnvelopeCipher};
use contextswitch_core::provider::GenericProvider;
use contextswitch_core::storage::{StorageError, StorageProvider};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
}

#[test]
fn in_memory_plain_conformance() {
    check_provider_conformance(|| {
        let blob_store = Arc::new(InMemoryBlobStore::new());
        let cipher = Arc::new(contextswitch_core::crypto::IdentityCipher);
        Box::new(GenericProvider::new(blob_store, cipher, None))
    });
}

#[test]
fn in_memory_encrypted_conformance() {
    check_provider_conformance(|| {
        let blob_store = Arc::new(InMemoryBlobStore::new());
        let cipher = Arc::new(EnvelopeCipher::with_work_factor(10));
        Box::new(GenericProvider::new(
            blob_store,
            cipher,
            Some("test_passphrase".to_string()),
        ))
    });
}

#[test]
fn wrong_passphrase_fails_decryption() {
    let blob_store = Arc::new(InMemoryBlobStore::new());
    let cipher = Arc::new(EnvelopeCipher::with_work_factor(10));
    let provider_write = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("correct_pass".to_string()),
    );

    let snap = provider_write.read().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_project("secret_project", at(0)).unwrap();
    provider_write.commit(logbook, &snap.version).unwrap();

    let provider_wrong = GenericProvider::new(blob_store, cipher, Some("wrong_pass".to_string()));
    assert!(matches!(
        provider_wrong.read(),
        Err(StorageError::DecryptionFailed)
    ));
}

#[test]
fn tampered_data_fails_decryption() {
    let blob_store = Arc::new(InMemoryBlobStore::new());
    let cipher = Arc::new(EnvelopeCipher::with_work_factor(10));
    let provider = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("passphrase".to_string()),
    );

    let snap = provider.read().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_project("p1", at(0)).unwrap();
    provider.commit(logbook, &snap.version).unwrap();

    // Now corrupt the bytes in the blob store manually
    let mut data = blob_store.get().unwrap().unwrap();
    let len = data.0.len();
    data.0[len - 5] ^= 0xFF;

    blob_store
        .put(
            &data.0,
            contextswitch_core::blob::Precondition::IfMatch(data.1),
        )
        .unwrap();

    assert!(matches!(
        provider.read(),
        Err(StorageError::DecryptionFailed)
    ));
}

#[test]
fn header_tampering_fails() {
    let blob_store = Arc::new(InMemoryBlobStore::new());
    let cipher = Arc::new(EnvelopeCipher::with_work_factor(10));
    let provider = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("passphrase".to_string()),
    );

    let snap = provider.read().unwrap();
    let mut logbook = snap.logbook.clone();
    logbook.add_project("p1", at(0)).unwrap();
    provider.commit(logbook, &snap.version).unwrap();

    let mut data = blob_store.get().unwrap().unwrap();
    // Tamper with the age header bytes
    data.0[10] ^= 0xFF;

    blob_store
        .put(
            &data.0,
            contextswitch_core::blob::Precondition::IfMatch(data.1),
        )
        .unwrap();

    let err = provider.read().unwrap_err();
    assert!(matches!(
        err,
        StorageError::Corrupt(_) | StorageError::DecryptionFailed
    ));
}

#[test]
fn reseal_preserves_logbook_integrity() {
    let cipher = EnvelopeCipher::with_work_factor(10);
    let mut payload = b"{\"revision\":1}".to_vec();

    for i in 1..=3 {
        let sealed = cipher.seal(&payload, "my_passphrase").unwrap();
        let opened = cipher.open(&sealed, "my_passphrase").unwrap();
        assert_eq!(opened, payload);
        payload = format!("{{\"revision\":{}}}", i + 1).into_bytes();
    }
}

/// Positive-path round trip through the public `Cipher` contract: what a
/// client actually depends on is that sealing and reopening with the
/// correct passphrase returns the original bytes unchanged.
#[test]
fn seal_then_open_round_trips_plaintext() {
    let cipher = EnvelopeCipher::with_work_factor(10);
    let plaintext = br#"{"projects":[],"spans":[],"tags":[],"schema_version":1,"revision":0,"active_span_id":null}"#;

    let envelope = cipher.seal(plaintext, "a strong passphrase").unwrap();
    let decrypted = cipher.open(&envelope, "a strong passphrase").unwrap();

    assert_eq!(decrypted, plaintext);
}

/// A structurally truncated envelope must be reported as an invalid header,
/// never as a decryption failure.
#[test]
fn truncated_envelope_is_invalid_header_not_decryption_failure() {
    let cipher = EnvelopeCipher::with_work_factor(10);

    match cipher.open(b"not an age file", "whatever") {
        Err(CryptoError::InvalidHeader(_)) => {}
        other => panic!("expected InvalidHeader, got {other:?}"),
    }

    let envelope = cipher.seal(b"{}", "passphrase").unwrap();
    let truncated = &envelope[..20];
    match cipher.open(truncated, "passphrase") {
        Err(CryptoError::InvalidHeader(_)) => {}
        other => panic!("expected InvalidHeader, got {other:?}"),
    }
}

const KAT_PASSPHRASE: &str = "correct horse battery staple";
const KAT_PLAINTEXT: &str = r#"{"hello":"world"}"#;

/// Generated via `EnvelopeCipher::seal` with `KAT_PASSPHRASE` and
/// `KAT_PLAINTEXT`; committed as a fixed byte vector so this test
/// detects any accidental change to the envelope format.
const KAT_ENVELOPE_BASE64: &str = include_str!("fixtures/kat_envelope.b64");

#[test]
fn known_answer_envelope_decrypts_to_expected_plaintext() {
    let cipher = EnvelopeCipher::new();
    let envelope_bytes = base64_decode(KAT_ENVELOPE_BASE64.trim());

    let plaintext = cipher.open(&envelope_bytes, KAT_PASSPHRASE).unwrap();
    assert_eq!(plaintext, KAT_PLAINTEXT.as_bytes());
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}
