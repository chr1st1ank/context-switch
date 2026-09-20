# Implementation Backlog

Outstanding work items for context-switch. When an item is implemented,
remove it from this list. Check existing ADRs in `docs/decisions/` before
starting an item — significant work may warrant a new ADR first.

## Clients

### Android app — follow-ups

The Kotlin/UniFFI app scaffold landed (ADR-0009): timer lifecycle, span
log/editing, project/tag management, S3 settings, and a foreground-service
timer notification. Remaining:

- Keystore-wrapped credential storage (currently app-private
  SharedPreferences; `security-crypto` is deprecated — pick a maintained
  approach)
- Offline queueing of timer actions (architecture §7) — reads already work
  from the cached snapshot
- Background sync / conflict UX beyond surfacing `MobileError.Conflict`
- Play Store signing/distribution (currently sideloaded debug APK)
- Graceful handling of a timer that was started/stopped remotely (currently error message)

### Dashboard web app

New reporting/visualization client. The dashboard is a client, not a
reporting service — it reads the logbook through the storage provider
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

- **Sync latency follow-ups** (post-ADR-0010): a mutation is now 2 requests
  (read + conditional commit). Remaining levers if more speed is needed:
  cache the derived master key per provider instance to skip one Argon2
  derivation per commit (keeps key material resident in process memory),
  a local snapshot cache so a mutation is a single conditional PUT
  (ties into the offline-queue work in architecture §7), and a pooling
  HTTP client — `minreq` opens a fresh TCP/TLS connection per request.
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
- **Passphrase rotation**: no logbook-layer operation exists yet to rotate
  the passphrase through the normal conditional-commit path (decrypt with the
  existing passphrase, re-seal under the new passphrase, and conditional-PUT
  with `If-Match`).
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
- **Schema migration policy**: for the versioned JSON logbook (§12)
