---
status: "accepted"
date: 2026-09-13
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Client-side envelope encryption and key management

## Context and Problem Statement

ADR-0007 puts canonical data into object storage the user does not fully
trust to hold plaintext. `docs/architecture.md` §10 permitted an encryption
wrapper "later" without specifying it. This record defines that wrapper:
where it sits in the storage stack, its on-disk byte format, and how key
material is generated, wrapped, and rotated.

## Decision Drivers

- The storage vendor must hold only an opaque blob: never plaintext, the
  passphrase, or the master key.
- A wrong passphrase must be reported as a wrong passphrase, never as
  corruption, and vice versa — the storage layer's error taxonomy depends
  on this distinction (see the crypto/storage error mapping in
  `src/provider.rs`).
- Adding a second device must need only bucket coordinates, credentials,
  and the passphrase — no key file to copy.
- The format must be fully specified outside of any one implementation, so
  Android can reimplement it without reading Rust.
- Encryption for remote storage is mandatory, not configurable, so there is
  no configuration in which a user accidentally publishes plaintext.

## Considered Options

- Encrypt at the provider boundary (decorate `StorageProvider`) vs. encrypt
  at the blob boundary (decorate `BlobStore`, below serialization).
- Derive the master key directly from the passphrase vs. a random master
  key wrapped under a passphrase-derived key.
- PBKDF2 vs. Argon2id for the passphrase-derived key.
- Cache the derived/master key across calls vs. re-derive per operation and
  thread key state explicitly.

## Decision Outcome

**Encryption sits at the blob boundary**, below serialization and above
transport. This is forced: the provider contract passes domain `Logbook`
values, so bytes only exist below it — a decorator over the existing
provider contract isn't expressible without either duplicating
serialization or leaking bytes above the contract. Consequence: the same
`Cipher` works for any future `BlobStore`, resolving the "encryption
wrapper" item from `docs/backlog.md`.

**Master key generated locally, wrapped under a passphrase-derived key.**
The master key is generated from a secure random source and never derived
from the passphrase directly, so its strength never depends on how
memorable the passphrase is. It is wrapped (encrypted) under a key derived
from the passphrase via Argon2id, and the wrapped copy travels inside the
stored object — see the "Master key" glossary entry in `CONTEXT.md` and
`docs/envelope-format.md` for the exact bytes.

**Key material threaded explicitly, not cached.**

```rust
pub trait Cipher: Send + Sync {
    fn open(&self, envelope: &[u8], passphrase: &str) -> Result<(Vec<u8>, KeyState), CryptoError>;
    fn seal(&self, plaintext: &[u8], passphrase: &str, prior: Option<&KeyState>) -> Result<Vec<u8>, CryptoError>;
}
```

`KeyState` carries the active key identity and the full wrapped-key list.
Threading it makes preserving existing wrapped keys on every ordinary
commit a compile-time obligation (story 49); hiding it in an internal cache
would create a silent lock-out failure mode the first time rotation
happened. `prior = None` means first-ever write: generate and wrap a fresh
master key.

**Each wrapped key carries its own derivation parameters.** `WrappedKey`
(not `KeyState` or the envelope header) owns `DerivationParams` — salt and
Argon2id costs — because different wrapped copies of the same master key
may exist under different passphrases with different parameters (rotation,
future multi-credential scenarios). A header-level derivation would
constrain every wrapped key to one passphrase's parameters, which is
incompatible with carrying more than one wrapped copy (story 48).

**Envelope format.** Self-describing: magic bytes, format version, method
identifier, active key identity, a reserved compression byte, and the
wrapped-key list, followed by a nonce and the ciphertext. XChaCha20-Poly1305
for its wide random nonce (removes a class of nonce-management error from
reimplementation); Argon2id for key wrapping. The full header is bound as
AEAD associated data on the main payload, so tampering with the algorithm,
key identity, or wrapped-key list is detected as an authentication failure
rather than honoured. No chunking: the logbook is a single blob handled in
native code. Full byte layout: `docs/envelope-format.md`.

**Error taxonomy.** Structural envelope problems (bad magic, truncated
data, an unsupported method or non-zero reserved-compression byte, an
unparseable header) are reported as corruption (`StorageError::Corrupt`).
Only an actual AEAD authentication failure — wrong passphrase or tampered
ciphertext, which are cryptographically indistinguishable from each other —
is reported as `StorageError::DecryptionFailed`. See the `From<CryptoError>
for StorageError` mapping in `src/provider.rs`.

**Passphrase sourcing.** A configured shell command (`passphrase_command`)
supplies the passphrase, with an interactive prompt as fallback. This
delegates to whatever secret manager the user already runs, works
headless, and adds no dependency. No caching beyond process lifetime. A
platform secret-store integration is tracked in `docs/backlog.md` as the
long-term replacement.

**Passphrase rotation is not yet wired in.** Nothing in `cosw` currently
calls `seal` with a second passphrase against an existing `KeyState`; the
cipher/logbook-layer seam supports it (see
`key_rotation_and_preservation` in
`libs/contextswitch-core/tests/encrypted_provider.rs`), but exposing it as
a logbook-layer operation (so it inherits conditional-commit safety) is
tracked in `docs/backlog.md`.

### Consequences

- Good, because the storage vendor never sees plaintext, the passphrase, or
  the master key, regardless of which `BlobStore` is behind it.
- Good, because per-wrapped-key derivation parameters mean strengthening
  Argon2id costs later doesn't require every client to upgrade in lockstep.
- Good, because tampering with any part of the header (including
  downgrading the method or key identity) fails authentication rather than
  being silently honoured.
- Bad, because there is no in-repo passphrase rotation command yet — a
  compromised passphrase requires manual key-state surgery until the
  backlog item lands.
- Neutral, because a `cosw decrypt` standalone command exists for disaster
  recovery independent of a working config or provider (see `cli/cosw/cli.py`).

## Implementation Plan

- **Affected paths**: `libs/contextswitch-core/src/{crypto,provider,lib}.rs`;
  `cli/cosw/cli.py` (`decrypt` command); `docs/envelope-format.md`.
- **Patterns to follow**: thread `KeyState` explicitly through
  `seal`/`open`; bind the full header as AEAD associated data; map
  structural errors to `Corrupt` and authentication failures to
  `DecryptionFailed`, never the reverse.
- **Patterns to avoid**: deriving the master key directly from the
  passphrase; caching derived keys across calls; a header-level derivation
  field that assumes one passphrase.
- **Migration steps**: none yet — this is envelope format version 1. A
  future version 2 requires a new `method`/`version` value and an explicit
  compatibility decision, not a silent reinterpretation of existing bytes.

## Verification

- [x] Wrong passphrase and tampered ciphertext both fail as
  `DecryptionFailed`, never `Corrupt`.
- [x] A truncated or structurally malformed envelope fails as `Corrupt`,
  never `DecryptionFailed`.
- [x] Header tampering (including the wrapped key's derivation salt) is
  caught by AEAD authentication.
- [x] An ordinary commit preserves every wrapped key, not only the active
  one (`key_rotation_and_preservation`).
- [x] A committed known-answer vector decrypts to its expected plaintext
  (`known_answer_envelope_decrypts_to_expected_plaintext`).
- [ ] Passphrase rotation goes through the same conditional-commit path as
  every other mutation (not yet implemented; `docs/backlog.md`).
