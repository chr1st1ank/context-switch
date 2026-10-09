# Component overview

Type-level map of the system: which structs exist, who constructs them,
and how a call travels through the stack. For the design rationale see
`architecture.md`; for recorded trade-offs see `docs/adr/`.

## Layers, bottom-up

```text
CLI (Python, cosw)                     Android app (Kotlin/Compose)
─────────────────                      ──────────────────────────
open_provider()                        CoswApp → LogbookStore.open()
        │                                    │
        │ PyO3 classes                       │ CoswStore.open / open_cached
        ▼                                    ▼
 S3Provider ─┐                     ┌─────── CoswStore ─────────────┐
 LocalFsProvider                   │ Backend::Direct(provider)     │
        │                          │ Backend::Synced(SyncEngine)   │── owns provider
        └──────────────┬───────────┴──────────────┬────────────────┘
                       ▼                          ▼
               StorageProvider (trait)    SyncEngine (owns a provider,
                       │                  is *not* a provider itself)
         ┌─────────────┼─────────────────────────────┐
         │             │                             │
 LocalFsProvider  GenericProvider           MemoryCachingProvider
 (GenericProvider  (BlobStore + Cipher +    (write-through decorator
  over local fs)    CachedHead, ADR-0010)    over a Sync-mode engine)
                       │
              ┌────────┴─────────┐
              ▼                  ▼
       BlobStore (trait)   Cipher (trait)
   ┌────────┬───────┬──┐   ┌────┴──────┐
LocalFs  S3Blob  InMemory Envelope  Identity
BlobStore Store  BlobStore Cipher    Cipher
```

## The traits

| Trait             | Contract                                                                                                                                                                                                                          | Implementations                                                                                              |
|-------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|
| `BlobStore`       | `get` returns bytes + ETag; `put(bytes, Precondition)` is a conditional write (`IfMatch(etag)` / `IfAbsent`); `stat` is a cheap change token (HEAD→ETag, `mtime:size`, default = full `get`)                                      | `LocalFsBlobStore` (lockfile + tmp-rename), `S3BlobStore` (SigV4-signed minreq), `InMemoryBlobStore` (tests) |
| `Cipher`          | `seal`/`open` plaintext ↔ envelope                                                                                                                                                                                                | `IdentityCipher` (local, plaintext), `EnvelopeCipher` (age, ADR-0011)                                        |
| `StorageProvider` | `read` → `StorageSnapshot{version, logbook}`; `commit(logbook, expected_version)` is atomic conditional write, returns new version, `Conflict` on stale; `fingerprint` is an opaque blob-change token (default: `read().version`) | `GenericProvider`, `LocalFsProvider`, `S3Provider` (Python-only), `MemoryCachingProvider`                    |

`version` is always the logbook **revision** (numeric string), not the
blob ETag — ETags stay internal to precondition checks. `fingerprint` is
a third, backend-defined token space, only ever compared to itself.

## The concrete types

| Type                                | Constructed by                                                   | Composes                                                       | Notes                                                                                                                                                       |
|-------------------------------------|------------------------------------------------------------------|----------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `Logbook`, `Span`, `Project`, `Tag` | domain ops on `Logbook`                                          | —                                                              | `domain.rs`; `Logbook::validate` enforces the one-active-span invariant; `commit` paths re-validate                                                         |
| `GenericProvider`                   | `LocalFsProvider`, `S3Provider`, UniFFI `open_provider`, tests   | `Arc<dyn BlobStore>` + `Arc<dyn Cipher>` + optional passphrase | `provider.rs`; owns `CachedHead` — commit against the just-read head skips a re-GET (ADR-0010)                                                              |
| `LocalFsProvider`                   | `cosw open_provider`, UniFFI `open_provider` (Local), PyO3 `new` | `GenericProvider(LocalFsBlobStore, IdentityCipher)`            | `storage.rs`; bootstraps an empty v1 logbook via `IfAbsent` put                                                                                             |
| `S3Provider`                        | `cosw open_provider` only                                        | `GenericProvider(S3BlobStore, EnvelopeCipher)`                 | Python-only constructor — it sources AWS creds from env/`~/.aws`, which don't exist on Android                                                              |
| `SyncEngine`                        | `CoswStore::open_cached`                                         | `Arc<dyn StorageProvider>` + worker thread                     | `sync.rs`; replica, not a provider: `snapshot()`, `commit()` (Sync or Buffered mode), `sync_status()`, `pending_writes()`, `take_failed_write()`, `flush()` |
| `MemoryCachingProvider`             | conformance tests; any strict-`StorageProvider` call site        | `SyncEngine` in `CommitMode::Sync`                             | The honest decorator: read-through cache + write-through commits, full trait contract                                                                       |
| `CoswStore`                         | `LogbookStore.open()` via UniFFI                                 | `Backend` enum + `location_url`                                | `contextswitch-uniffi`; owns the read→mutate→commit loop all Android ops go through                                                                         |
| `LogbookStore`                      | `CoswApp.onCreate`                                               | `CoswStore`, `SettingsStore`, on-disk snapshot cache           | Kotlin owner of the FFI object; all calls on `Dispatchers.IO`                                                                                               |
| `SettingsStore`                     | `LogbookStore`                                                   | shared prefs → `StorageConfig`                                 | Builds `StorageConfig.S3`/`Local` incl. injected credentials; `importPortableConfig` applies a parsed cosw `config.toml`                                    |
| `portable_config`                   | UniFFI `parse_portable_config`/`serialize_portable_config`       | `toml` crate                                                   | `portable_config.rs`; cosw `config.toml` parse/serialize for settings transfer — non-secret fields only, per ADR-0006                                       |

## Who builds the S3 stack, per client

- **cosw**: `open_provider()` → `S3Provider(bucket, region, prefix,
  passphrase, endpoint, use_path_style, profile)` → internally builds
  `S3BlobStore::new` + `EnvelopeCipher` + `GenericProvider`.
- **Android**: `LogbookStore.open()` → `SettingsStore.storageConfig()` →
  `CoswStore.open(config)` → `open_provider` → `GenericProvider::new(
  S3BlobStore::with_credentials(...), EnvelopeCipher, Some(passphrase))`.
  Credentials are injected explicitly via `S3Config`.
- `open_cached` wraps the same provider in `SyncEngine::new(provider,
  interval, CommitMode::Buffered)` instead.

## How a call travels

**Read (direct):** `CoswStore.snapshot()` → `Backend::read` →
`provider.read()` → `GenericProvider`: `blob.get` → `cipher.open` →
JSON parse + `validate` → `StorageSnapshot{version: revision}`, head
cached for the next commit.

**Read (synced):** same path, but `engine.snapshot()` serves the cached
entry without I/O; once per `refresh_interval` it enqueues a `Probe`
that the worker runs as `fingerprint` → re-`read` only when the token
moved (monotonic install — never downgrades an optimistic entry).

**Mutate (direct):** `CoswStore.mutate(f)`: `read` → apply `f` →
`commit(lb, snap.version)` → `GenericProvider.commit`: validate →
resolve head (cached or GET) → `revision = base + 1` → seal →
`put(IfMatch(etag))` → `412` maps to `Conflict` (with one re-read to
report the real winner). `mutate` retries once on `Conflict`.

**Mutate (synced, Buffered):** `engine.commit` validates, rejects a
provably stale version synchronously, installs an optimistic entry
(predicted `expected + 1`), queues the write, returns the prediction.
The worker drains FIFO through `inner.commit` — `Unavailable` retried
twice, then the `{logbook, error}` lands in the failed-writes list;
`Conflict` additionally fires an immediate reconcile refresh. Because
each queued write expects its predecessor's prediction, one conflict
fails the rest of the backlog — preserved per architecture §6.

**Lifecycle:** the app calls `CoswStore.flush()` on pause/exit — a FIFO
barrier that returns once the queue is drained, carrying the earliest
rejected write (logbook included) for export/retry.
