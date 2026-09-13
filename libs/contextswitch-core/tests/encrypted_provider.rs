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
        let cipher = Arc::new(EnvelopeCipher);
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
    let cipher = Arc::new(EnvelopeCipher);
    let provider_write = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("correct_pass".to_string()),
    );

    let snap = provider_write.read().unwrap();
    let mut doc = snap.document.clone();
    doc.add_project("secret_project", at(0)).unwrap();
    provider_write.commit(doc, &snap.version).unwrap();

    let provider_wrong = GenericProvider::new(blob_store, cipher, Some("wrong_pass".to_string()));
    assert!(matches!(
        provider_wrong.read(),
        Err(StorageError::DecryptionFailed)
    ));
}

#[test]
fn tampered_data_fails_decryption() {
    let blob_store = Arc::new(InMemoryBlobStore::new());
    let cipher = Arc::new(EnvelopeCipher);
    let provider = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("passphrase".to_string()),
    );

    let snap = provider.read().unwrap();
    let mut doc = snap.document.clone();
    doc.add_project("p1", at(0)).unwrap();
    provider.commit(doc, &snap.version).unwrap();

    // Now corrupt the bytes in the blob store manually
    let mut data = blob_store.get().unwrap().unwrap();
    // Tamper with the ciphertext (which is at the very end of the envelope)
    let len = data.0.len();
    data.0[len - 5] ^= 0xFF;

    // Put it back with IfMatch
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
fn header_tampering_fails_authentication() {
    let blob_store = Arc::new(InMemoryBlobStore::new());
    let cipher = Arc::new(EnvelopeCipher);
    let provider = GenericProvider::new(
        blob_store.clone(),
        cipher.clone(),
        Some("passphrase".to_string()),
    );

    let snap = provider.read().unwrap();
    let mut doc = snap.document.clone();
    doc.add_project("p1", at(0)).unwrap();
    provider.commit(doc, &snap.version).unwrap();

    // Now corrupt the JSON header bytes — specifically the derivation
    // salt, which is neither structurally validated nor used as a lookup
    // key, so this flip stays JSON-valid and reaches the AEAD check this
    // test targets (a syntax- or lookup-breaking flip would instead fail
    // earlier as InvalidHeader, which is exercised separately).
    let mut data = blob_store.get().unwrap().unwrap();
    let header_len = u32::from_le_bytes(data.0[4..8].try_into().unwrap()) as usize;
    let header_json = &data.0[8..8 + header_len];
    let marker = b"\"salt\":\"";
    let marker_pos = header_json
        .windows(marker.len())
        .position(|w| w == marker)
        .expect("header JSON must contain a derivation salt");
    let tamper_index = 8 + marker_pos + marker.len(); // first base64 char of the salt
    data.0[tamper_index] ^= 0x01;

    // Put it back
    blob_store
        .put(
            &data.0,
            contextswitch_core::blob::Precondition::IfMatch(data.1),
        )
        .unwrap();

    // Decryption of ciphertext should fail because the header JSON is bound as associated data
    assert!(matches!(
        provider.read(),
        Err(StorageError::DecryptionFailed)
    ));
}

/// Story 49: "an ordinary commit must preserve all existing wrapped keys,
/// so committing from one device can never lock out another." A second
/// wrapped copy of the master key (as rotation, story 46-48, would add) is
/// injected directly into the `KeyState` — full rotation is not yet wired
/// into the document layer (see the PRD's "Open decision" and
/// `docs/backlog.md`), but the preservation property `seal` must honor is
/// independent of how a second key got there.
#[test]
fn key_rotation_and_preservation() {
    let cipher = EnvelopeCipher;

    let sealed = cipher.seal(b"{}", "passphrase_one", None).unwrap();
    let (plaintext, mut state) = cipher.open(&sealed, "passphrase_one").unwrap();
    assert_eq!(state.keys.len(), 1, "fresh envelope wraps exactly one key");
    let active_key_id = state.active_key_id.clone();

    // Inject a second wrapped-key entry, as a rotation to a second
    // passphrase would. Its wrapped bytes don't need to decrypt to
    // anything real for this test: what's under test is that `seal`
    // preserves every entry in the list, not just the active one.
    state.keys.push(contextswitch_core::crypto::WrappedKey {
        key_id: "second-device-key".to_string(),
        derivation: state.keys[0].derivation.clone(),
        nonce: base64_encode(&[0u8; 24]),
        encrypted_key: base64_encode(&[0u8; 48]),
    });

    // An ordinary commit — still against the original active key and
    // passphrase — must not drop the second entry.
    let resealed = cipher
        .seal(&plaintext, "passphrase_one", Some(&state))
        .unwrap();
    let (_, state_after_commit) = cipher.open(&resealed, "passphrase_one").unwrap();

    assert_eq!(
        state_after_commit.keys.len(),
        2,
        "an ordinary commit must preserve every wrapped key, not just the active one"
    );
    assert!(state_after_commit
        .keys
        .iter()
        .any(|k| k.key_id == active_key_id));
    assert!(state_after_commit
        .keys
        .iter()
        .any(|k| k.key_id == "second-device-key"));
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Positive-path round trip through the public `Cipher` contract: what a
/// client actually depends on is that sealing and reopening with the
/// correct passphrase returns the original bytes unchanged.
#[test]
fn seal_then_open_round_trips_plaintext() {
    let cipher = EnvelopeCipher;
    let plaintext = br#"{"projects":[],"spans":[],"tags":[],"schema_version":1,"revision":0,"active_span_id":null}"#;

    let envelope = cipher.seal(plaintext, "a strong passphrase", None).unwrap();
    let (decrypted, state) = cipher.open(&envelope, "a strong passphrase").unwrap();

    assert_eq!(decrypted, plaintext);
    assert_eq!(state.keys.len(), 1);
}

/// A structurally truncated envelope (missing magic, missing payload
/// nonce, ...) must be reported as an invalid header, never as a
/// decryption failure — it was never a valid envelope to attack in the
/// first place, wrong-passphrase or otherwise.
#[test]
fn truncated_envelope_is_invalid_header_not_decryption_failure() {
    let cipher = EnvelopeCipher;

    // Too short to even contain the magic bytes.
    match cipher.open(b"CO", "whatever") {
        Err(CryptoError::InvalidHeader(_)) => {}
        other => panic!("expected InvalidHeader, got {other:?}"),
    }

    // Valid envelope, sliced down to lose the trailing payload nonce.
    let envelope = cipher.seal(b"{}", "passphrase", None).unwrap();
    let header_len = u32::from_le_bytes(envelope[4..8].try_into().unwrap()) as usize;
    let truncated = &envelope[..8 + header_len + 4];
    match cipher.open(truncated, "passphrase") {
        Err(CryptoError::InvalidHeader(_)) => {}
        other => panic!("expected InvalidHeader, got {other:?}"),
    }
}

/// Known-answer test: a committed envelope byte layout, decryptable with a
/// known passphrase and yielding known plaintext. Any reimplementation
/// (e.g. the Android client) can decode this vector to prove byte-for-byte
/// compatibility without reading Rust. If the envelope format ever
/// changes, this vector must be regenerated deliberately, not silently.
#[test]
fn known_answer_envelope_decrypts_to_expected_plaintext() {
    let cipher = EnvelopeCipher;
    let envelope_bytes = base64_decode(KAT_ENVELOPE_BASE64.trim());

    let (plaintext, _) = cipher.open(&envelope_bytes, KAT_PASSPHRASE).unwrap();
    assert_eq!(plaintext, KAT_PLAINTEXT.as_bytes());
}

const KAT_PASSPHRASE: &str = "correct horse battery staple";
const KAT_PLAINTEXT: &str = r#"{"hello":"world"}"#;

/// Generated once via `EnvelopeCipher::seal` with `KAT_PASSPHRASE` and
/// `KAT_PLAINTEXT`; committed as a fixed byte vector (see
/// `docs/envelope-format.md` for the full byte layout this proves) so this
/// test detects any accidental change to the envelope format.
const KAT_ENVELOPE_BASE64: &str = include_str!("fixtures/kat_envelope.b64");

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}
