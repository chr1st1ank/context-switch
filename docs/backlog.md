# Implementation Backlog

Outstanding work items for context-switch. When an item is implemented,
remove it from this list. Check existing ADRs in `docs/decisions/` before
starting an item — significant work may warrant a new ADR first.

## Clients

### Android app — basic time tracking

Scaffold the native Android app in `android/` (currently a README stub).

- Timer lifecycle: start, stop, switch, status
- Project/tag selection from cached data
- Storage provider integration — reuse the Rust core via FFI or implement
  the provider contract natively (see `libs/contextswitch-core`)
- Follow-ups: offline queueing of timer actions, background sync,
  notification/background behavior (open question, `architecture.md` §12)

### Dashboard web app

New reporting/visualization client. The dashboard is a client, not a
reporting service — it reads the document through the storage provider
interface and computes reports on demand (`architecture.md` §3, §8).

- Initial scope: span log, totals by project/tag, time-series by day,
  filtering by project/tag and date range

## Storage

### S3 provider follow-ups

The S3-compatible provider with client-side envelope encryption
(`libs/contextswitch-core/src/{s3,crypto,blob,provider}.rs`) is implemented,
but the PRD (`docs/prd-s3-provider.md`, now removed — see the ADRs and
`docs/envelope-format.md` for its normative content) identified several
behaviors that did not land in the first version:

- **Retry policy** (stories 39-41): transient S3 failures (network errors,
  5xx) are surfaced immediately as `StorageError::Unavailable` rather than
  retried with bounded, jittered backoff; conflicts/auth failures should
  continue to fail fast.
- **Conditional-write probe at config time** (story 44): a backend that
  cannot enforce `If-None-Match`/`If-Match` should be rejected when the
  provider is opened, not discovered later as silent data loss. Needs a
  cheap way to detect precondition support (e.g. a HEAD/probe write) before
  trusting a non-AWS S3-compatible endpoint.
- **`StorageError` kind discriminator for Python** (part of the error
  taxonomy story): the CLI currently distinguishes error kinds only by
  matching on message text; add a `kind`/`args` discriminator to the
  PyO3-exposed exception so `cosw`'s transaction helper can branch reliably.
- **Integration lane against a real S3-compatible endpoint** (story 56): the
  conformance suite runs offline against `InMemoryBlobStore`/local files
  only; add an opt-in test lane that can point `S3BlobStore` at a local
  server (e.g. MinIO) or a real bucket.
- **Explicit precondition-honoring assertion** (story 57): add a
  conformance check that fails loudly if a configured backend silently
  ignores `IfAbsent`/`IfMatch` instead of producing a vacuous pass.
- **Passphrase rotation** (stories 46-47, the PRD's "Open decision"): no
  document-layer operation exists yet to rotate the passphrase through the
  normal conditional-commit path; `EnvelopeCipher::seal`'s `KeyState`
  threading already supports adding a wrapped key (see
  `key_rotation_and_preservation` in
  `libs/contextswitch-core/tests/encrypted_provider.rs`), but nothing calls
  it from `cosw`.
- **OS secret-store support for passphrase sourcing** (real fix for story
  8): read the passphrase from the platform secret store (Keychain /
  Secret Service / Credential Manager) as a first-class option alongside
  `passphrase_command`. Until this lands, `cosw` should at least reject a
  `storage.passphrase` (or similarly named secret) key if present in the
  config file, as an interim guard against a passphrase silently sitting
  ignored in a file a user might commit.

## cosw CLI

### Offline queue for timer actions

Currently every mutation requires a reachable provider. Per
`architecture.md` §7:

- Queue start/stop/switch locally when disconnected; sync when
  connectivity returns
- Degraded read-only access to cached projects, spans, and reports
- Preserve conflicted actions for export/copying (§6); conflict export
  format and retention are open questions (§12)
- Deferred from ADR-0005

## Cross-cutting / future

- **Export capability**: exporters that read the native domain model and
  produce selected formats (`architecture.md` §9); final formats not yet
  chosen
- **Encryption wrapper**: optional encryption at the provider boundary
  (§10)
- **Schema migration policy**: for the versioned JSON document (§12)
