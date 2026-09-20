# Envelope format (age-encryption.org/v1)

This is the specification for the client-side encrypted envelope used by
the remote (S3-compatible) storage provider — see ADR-0007 and ADR-0011
(which superseded the custom envelope format originally in ADR-0008).

An envelope is opaque to the storage provider: the object stored in the
remote bucket (the "logbook file", see `CONTEXT.md`) is always exactly
one standard `age` file.

The reference implementation is `libs/contextswitch-core/src/crypto.rs`
using the `age` crate.

## Specification

Envelopes conform strictly to the **`age-encryption.org/v1`** file format specification
(authored by Filippo Valsorda and Benjojo).

### Header & Encryption Parameters

- **Header preamble**: `age-encryption.org/v1\n`
- **Recipient type**: `scrypt` (native passphrase recipient for age)
  - Work factor: $\log_2(N) = 16$ (pinned for interactive CLI performance and deterministic operation)
  - Salt: 16 random bytes (base64-encoded in the stanza)
- **Header MAC**: Authenticated with HMAC-SHA256 over the entire header using a key derived from the file key.
- **Payload encryption**: STREAM construction using ChaCha20-Poly1305 with 64 KiB chunks and per-chunk Poly1305 authentication tags.

## Decryption Procedure

1. Read and parse the age header. Reject missing preamble or unparseable stanzas as corrupt (`StorageError::Corrupt`).
2. Verify that the file contains an `scrypt` recipient stanza.
3. Derive the key using `scrypt` with the file's specified salt and work factor.
4. Unwrap the file key and verify the header MAC. A wrong passphrase or tampered header MAC causes `StorageError::DecryptionFailed`.
5. Stream-decrypt the ChaCha20-Poly1305 chunks. An authentication failure during streaming read is `StorageError::DecryptionFailed`.
6. The resulting plaintext is the canonical logbook's UTF-8 JSON.

## Disaster Recovery

Because envelopes are standard age files, users do not need `cosw` to recover their data:

```bash
# Using standard age CLI
age --decrypt logbook.json > logbook_decrypted.json

# Using cosw decrypt
cosw decrypt logbook.json
```

## Known-Answer Test Vector

`libs/contextswitch-core/tests/fixtures/kat_envelope.b64` is a base64
encoding of a committed age envelope. Decrypting it with the passphrase
`correct horse battery staple` yields the plaintext `{"hello":"world"}`
exactly. See `known_answer_envelope_decrypts_to_expected_plaintext` in
`libs/contextswitch-core/tests/encrypted_provider.rs` for the Rust assertion.
