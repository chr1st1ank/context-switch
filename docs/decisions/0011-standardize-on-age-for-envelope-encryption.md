---
status: "accepted"
date: 2026-09-20
decision-makers: "Project owner"
consulted: ""
informed: ""
supersedes: "ADR-0008"
---

# Standardize on age for client-side envelope encryption

## Context and Problem Statement

ADR-0008 implemented client-side envelope encryption using low-level cryptographic
primitives (`argon2`, `chacha20poly1305`) combined with a custom wire format
(`COSW` magic, custom JSON header, and manual payload/AAD framing) and custom
multi-wrapped DEK logic (`KeyState`).

Because context-switch is a time-tracking tool, maintaining bespoke cryptographic
composition logic introduces security risk: flaws in custom framing and key-wrapping
logic are not covered by upstream security advisories or public vulnerability
monitoring. We want to outsource the envelope encryption logic entirely to a standard,
audited, higher-level cryptography library.

## Decision Drivers

- Outsource cryptographic framing, key derivation, and authenticated encryption
  to a widely used and audited library tracked by RustSec/CVE advisories.
- Preserve zero-file onboarding: a user needs only remote storage coordinates,
  credentials, and a passphrase (no private key files or identity setup).
- Keep interactive CLI operations snappy (<100 ms crypto latency per commit).
- Enable disaster recovery using standard off-the-shelf tools without requiring
  the context-switch toolchain.
- Simplify internal interfaces: eliminate bespoke key state tracking from the
  storage provider seam.

## Considered Options

- **`age` (`age-encryption.org/v1`) via the `age` crate**: The standard modern
  file encryption format in Go and Rust. Passphrase mode uses scrypt. Outsources
  envelope framing, chunked AEAD streaming, and key wrapping completely.
- **Custom envelope format (status quo, ADR-0008)**: Retain the custom `COSW`
  framing and multi-wrapped DEK list.
- **Standards-based container formats (JWE / COSE / CMS)**: Heavyweight dependencies,
  complex specs, token/web oriented rather than simple blob storage.

## Decision Outcome

**Adopt `age` (`age` crate) implementing the `age-encryption.org/v1` specification.**

1. **Format and Library**: All client-side encrypted blobs are standard age files.
   The custom binary framing (`COSW` magic, LE header length, JSON header) is
   retired in favor of the age file specification.
2. **Key Derivation & Work Factor**: Passphrase encryption uses age's native `scrypt`
   recipient. To prevent sluggishness during frequent interactive commits (`cosw start`,
   `cosw stop`), the scrypt work factor is pinned to $\log_2(N) = 16$ (~20–50 ms derivation),
   providing solid brute-force resistance while keeping the CLI responsive.
3. **Interface Simplification**: The `Cipher` trait drops `KeyState` and the multi-key
   wrapping list:
   ```rust
   pub trait Cipher: Send + Sync {
       fn open(&self, envelope: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError>;
       fn seal(&self, plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError>;
   }
   ```
   `GenericProvider` and `CachedHead` no longer carry key state across calls;
   sealing is a pure function of `(plaintext, passphrase)`.
4. **Disaster Recovery**: Any standard `age` tool (`age --decrypt`, `rage`) can
   decrypt the logbook directly from remote storage. The `cosw decrypt` command
   is retained for convenience, backed by the same `age` implementation.

### Consequences

- Good, because zero custom encryption framing or wrapping code remains in context-switch;
  vulnerability reporting and patching are delegated to the `age` crate maintainers.
- Good, because disaster recovery is trivial and independent of `cosw`: `age --decrypt logbook.json > plain.json`.
- Good, because `KeyState`, `EnvelopeHeader`, and `WrappedKey` are eliminated, simplifying
  `GenericProvider` and `CachedHead`.
- Bad, because `age` passphrase encryption does not support multiple distinct passphrases
  for the same file without re-encrypting the payload (unlike ADR-0008's multi-wrapped DEK list).
  This trade-off is accepted for single-user, multi-device usage sharing a passphrase.
- Neutral, because existing pre-1.0 S3 envelopes are broken by the format change; test
  vectors and fixtures are updated accordingly.

## Implementation Plan

- **Affected paths**:
  - `libs/contextswitch-core/Cargo.toml`: remove `chacha20poly1305`, `argon2`; add `age`.
  - `libs/contextswitch-core/src/crypto.rs`: implement `EnvelopeCipher` with `age::scrypt`.
  - `libs/contextswitch-core/src/provider.rs`: simplify `GenericProvider` and `CachedHead`.
  - `libs/contextswitch-core/src/lib.rs`: update PyO3 bindings (`decrypt_envelope`, `encrypt_envelope`).
  - `libs/contextswitch-uniffi/src/lib.rs`: instantiate `EnvelopeCipher::new()`.
  - `libs/contextswitch-core/tests/encrypted_provider.rs`: update test suite and known-answer test vector.
  - `docs/envelope-format.md`: document `age-encryption.org/v1`.

## Verification

- [x] Round-trip encryption and decryption preserves logbook plaintext (`seal_then_open_round_trips_plaintext`).
- [x] Wrong passphrase fails with `StorageError::DecryptionFailed`.
- [x] Corrupted ciphertext fails with `StorageError::DecryptionFailed`.
- [x] Truncated / malformed header fails with `StorageError::Corrupt`.
- [x] Committed known-answer vector decrypts to expected plaintext (`known_answer_envelope_decrypts_to_expected_plaintext`).
- [x] All Python bindings and `cosw decrypt` pass tests.
