# Envelope byte format

This is the cross-language source of truth for the client-side encrypted
envelope used by the remote (S3-compatible) storage provider — see
ADR-0007 and ADR-0008. Any reimplementation (Android, `cosw decrypt`, a
future exporter) must produce and consume exactly these bytes; there is no
other specification. The reference implementation is
`libs/contextswitch-core/src/crypto.rs`.

An envelope is opaque to the storage provider: the object stored in the
bucket (the "logbook", see `CONTEXT.md`) is always exactly one envelope.

## Layout

```text
+----------+-------------+------------------+----------------+------------------+
| magic    | header_len  | header (JSON)    | payload_nonce  | ciphertext       |
| 4 bytes  | 4 bytes LE  | header_len bytes | 24 bytes       | remaining bytes  |
+----------+-------------+------------------+----------------+------------------+
```

- **magic** (4 bytes): the ASCII literal `COSW`. Any other value is an
  invalid envelope (rejected as corrupt, not as a decryption failure).
- **header_len** (4 bytes, little-endian `u32`): the exact byte length of
  the header JSON that immediately follows.
- **header** (`header_len` bytes): UTF-8 JSON, see "Header" below.
- **payload_nonce** (24 bytes): the XChaCha20-Poly1305 nonce for the main
  payload.
- **ciphertext** (remaining bytes): the AEAD-encrypted document JSON,
  including its 16-byte authentication tag.

The header bytes are used verbatim as the AEAD associated data for the main
payload's decryption. Any bit flip anywhere in the header — including
fields not otherwise validated, such as a derivation salt — therefore
causes the main payload's authentication to fail. This is deliberate:
downgrade or key-identity tampering must be detected, not honoured.

## Header (JSON)

```json
{
  "version": 1,
  "method": "xchacha20-poly1305",
  "active_key_id": "<uuid>",
  "reserved_compression": 0,
  "keys": [
    {
      "key_id": "<uuid>",
      "derivation": {
        "algorithm": "argon2id",
        "m_cost": 16384,
        "t_cost": 3,
        "p_cost": 1,
        "salt": "<base64>"
      },
      "nonce": "<base64, 24 bytes>",
      "encrypted_key": "<base64, 32-byte key + 16-byte tag>"
    }
  ]
}
```

- **version**: envelope format version. Currently always `1`. A reader
  must reject any other value as an unsupported/invalid header rather than
  guess at a different layout.
- **method**: the AEAD method identifier for the main payload. Currently
  always the literal string `"xchacha20-poly1305"`.
- **active_key_id**: the `key_id` of the entry in `keys` that was used to
  wrap the master key most recently used to seal this envelope. Selects
  which wrapped-key entry to unwrap first; a reader may also attempt other
  entries with a passphrase it holds (multi-passphrase support at the
  document layer is not yet implemented — see `docs/backlog.md`).
- **reserved_compression**: reserved for a future compression method
  applied to the plaintext before encryption. Always `0` today; a reader
  must reject any non-zero value as unsupported rather than attempt to
  decompress.
- **keys**: one or more wrapped copies of the same 32-byte master key, one
  per passphrase/device that can unwrap it. Preserving every entry across
  an ordinary commit is a hard requirement (see ADR-0008 and the
  `key_rotation_and_preservation` test) — losing one locks out whichever
  device only holds that passphrase.
  - **key_id**: a UUID identifying this wrapped copy. Recording it lets a
    wrong passphrase be detected immediately (the corresponding entry
    won't decrypt) rather than surfacing as a confusing parse failure.
  - **derivation**: the Argon2id parameters and salt used to derive the
    key-encrypting key (KEK) from this entry's passphrase. Recorded
    per-entry, not once for the whole envelope, because different entries
    may be wrapped under different passphrases with different costs.
    `salt` is base64-encoded random bytes (16 bytes in the reference
    implementation).
  - **nonce**: base64-encoded 24-byte XChaCha20-Poly1305 nonce used to
    encrypt this entry's wrapped key.
  - **encrypted_key**: base64-encoded AEAD ciphertext of the 32-byte
    master key under the KEK, with associated data equal to this entry's
    `key_id` (UTF-8 bytes). Includes the 16-byte Poly1305 tag.

## Decryption procedure

1. Verify the magic bytes and read `header_len`; reject if the remaining
   bytes are shorter than `header_len` (corrupt/truncated).
2. Parse the header JSON; reject unparseable JSON, an unsupported
   `version`/`method`, or a non-zero `reserved_compression` as an invalid
   header (`StorageError::Corrupt`), never as a decryption failure.
3. Find the `keys` entry whose `key_id` equals `active_key_id`. Absence is
   an invalid header, not a decryption failure — it means the object
   itself is structurally inconsistent, independent of any passphrase.
4. Derive the KEK from the caller's passphrase using that entry's
   `derivation`. Decrypt `encrypted_key` with the KEK, the entry's `nonce`,
   and associated data `key_id`. An AEAD authentication failure here
   — wrong passphrase or tampered wrapped key — is
   `StorageError::DecryptionFailed`.
5. Read the 24-byte `payload_nonce` and the remaining ciphertext. Decrypt
   with the recovered master key, the payload nonce, and associated data
   equal to the raw header bytes (step 1's `header` slice, byte-for-byte,
   not a re-serialization of the parsed struct). An AEAD authentication
   failure here is also `StorageError::DecryptionFailed`.
6. The resulting plaintext is the canonical document's UTF-8 JSON, exactly
   as defined in ADR-0004.

## Known-answer test vector

`libs/contextswitch-core/tests/fixtures/kat_envelope.b64` is a base64
encoding of a committed envelope. Decrypting it with the passphrase
`correct horse battery staple` must yield the plaintext `{"hello":"world"}`
exactly. See `known_answer_envelope_decrypts_to_expected_plaintext` in
`libs/contextswitch-core/tests/encrypted_provider.rs` for the Rust
assertion; a reimplementation should be able to decode the same fixture
independently as proof of byte-for-byte compatibility.

If the envelope format ever changes, this vector must be regenerated
deliberately (bump `version` and update this document), never silently.
