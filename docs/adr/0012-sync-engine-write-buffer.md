---
status: "accepted"
date: 2026-09-23
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Sync engine over a storage provider, with a buffered write queue

## Context and Problem Statement

Even with ADR-0010's cached head, every `CoswStore::snapshot` and every
mutation still blocks the UI on a network round trip: `read()` always
GETs the blob, and `mutate`'s commit is synchronous (a PUT plus a
re-read on conflict). On a remote endpoint at ~300–800 ms per request,
reading the timer state or starting a span visibly stalls the app.
This is the "torn edge" identified in architecture §7: the client wants
instant reads/writes against a local view while canonical data lives
in remote storage.

## Decision Drivers

- Zero storage I/O on the caller's path after the initial read —
  neither reads nor commits may introduce lag into the UI.
- The stale-write guarantee must not weaken; the conditional write
  remains the correctness mechanism.
- The `StorageProvider` contract must stay honest: `commit` returning
  `Ok` must mean the write was durably accepted. A write-behind queue
  cannot satisfy that, so it must not hide behind the trait — callers
  that want strict semantics must get them without knowing a cache
  exists.
- The sync layer must be optional: hooked in at startup for clients that
  want it (the long-lived Android store), left out for others (the
  CLI, which stays on the raw S3 provider — each invocation is a fresh
  process where a local replica buys little).
- Rejected mutations must be preserved for the user, per
  architecture §6 / ADR-0002.
- Both an in-memory and an on-disk replica should be possible behind the
  same design; the memory variant ships first.

## Considered Options

- `SyncEngine` — a replicated local view that *composes* an inner
  provider (snapshot cache, interval-based refresh, worker-drained
  write queue) plus `MemoryCachingProvider`, a strict write-through
  adapter that keeps the full `StorageProvider` contract for
  provider-shaped consumers and the conformance suite.
- `MemoryCachingProvider` as a `StorageProvider` decorator with a
  `CommitMode` flag — the original shape. Rejected: in buffered mode
  `commit` returning `Ok` would mean "queued", not "durable", which
  silently breaks the trait contract for any client that doesn't know
  to poll the out-of-band status surface — and reaching that surface
  requires the concrete type, i.e. a downcast past the interface.
- Polling sync inside `GenericProvider` (extend ADR-0010's CachedHead
  into a full snapshot cache) — rejected: it would couple replication
  policy to one provider instead of composing over all of them.
- A client-side cache in the UniFFI layer instead of the core library —
  rejected: it would leave the Rust API without the benefit and
  duplicate logic the core already owns.

## Decision Outcome

Chosen option: `SyncEngine` in `contextswitch-core` (`src/sync.rs`).
The engine *uses* a `StorageProvider` rather than being one: the seam
moves up to the client (`CoswStore` holds a `Direct(provider)` /
`Synced(engine)` backend enum), which is exactly where the sync status
surface is wanted anyway. `MemoryCachingProvider` — engine in `Sync`
commit mode — remains as the transparent decorator: it satisfies the
full provider contract, so the conformance suite and any strict
consumer work unchanged, and it is what future stacked decorators can
safely wrap.

### Read path

`snapshot()` serves the cached `StorageSnapshot` immediately. When the
entry is older than `refresh_interval` (default 30 s) it signals the
background worker to revalidate; staleness is bounded by the interval.
Freshness is checked with a new defaulted `StorageProvider::fingerprint()`
— a blob-level change token backed by `BlobStore::stat()` (HEAD→ETag on
S3, `mtime:size` locally, stored etag in memory) — so a hit costs a HEAD
request instead of a full GET + decrypt + parse. `fingerprint` is a
different token than the snapshot `version` (logbook revision) and the
two are never compared. The cold path probes *before* fetching, so a
stored token is never newer than the snapshot it labels.

### Commit path — two modes

`CommitMode::Sync` delegates synchronously (write-through, invalidate on
error) and preserves the exact `StorageProvider` contract — this is what
`MemoryCachingProvider` uses, and the conformance suite runs against it.
`CommitMode::Buffered` validates the logbook in memory, rejects a
provably stale `expected_version` synchronously against the cached
version, installs an optimistic entry (predicted version = expected + 1),
queues the write on an uncapped queue, and returns the predicted version
immediately. Buffered mode assumes numeric `revision = base + 1` version
allocation, which all current providers implement; a provider with
opaque non-numeric versions would drain chained writes into conflicts.

A single worker thread drains the queue FIFO ASAP (no batching) and also
runs the refresh probes; ordering on one thread keeps a probe from
racing a commit, and probes are suppressed while the queue is non-empty
because the optimistic entry is knowingly ahead of canonical.

### Error handling — three tiers

1. **Synchronous**: `InvalidData`, cached-version `Conflict` detection,
   and cold-read/`flush` errors are returned to the caller.
2. **Async write errors**: `Unavailable` is retried with bounded backoff
   (2 retries, 250 ms base) in the worker; final failures keep the
   rejected `{logbook, error}` in a failed-writes list surfaced via
   `take_failed_write()` / `sync_status()` / `flush()` — `flush()`
   returns the earliest rejected write *including its logbook* so the
   client can preserve it. A `Conflict` additionally triggers an
   immediate reconcile refresh so the replica reconverges on canonical
   data; because queued writes each expect their predecessor's
   predicted version, one conflict fails the rest of the backlog too —
   accepted, since no automatic merge exists anyway.
3. **Refresh errors**: probe/read failures in the worker only set a
   `Degraded` status and retry next interval — the cached snapshot keeps
   being served, which is also the offline-read path of
   architecture §7. A successful probe clears `Degraded` even when the
   fingerprint still matches.

Monotonic installs (`newer_or_equal` on the snapshot version) prevent a
refresh from clobbering a newer, possibly optimistic, entry. On a failed
buffered write the optimistic entry is dropped only if it still carries
that write's predicted version.

### Wiring

UniFFI gains `CoswStore::open_cached(config, refresh_interval_secs)`,
which composes a `SyncEngine` (Buffered mode) around the provider plus
`pending_writes()`, `sync_status()`, `take_failed_write()` and
`flush()` (both returning the rejected logbook as JSON) for app
pause/exit. `CoswStore::open` is unchanged and unreplicated. The CLI is
deliberately not wired — it keeps using the underlying S3 provider
directly.

### Consequences

- Good, because after the cold read no UI-visible call performs storage
  I/O: reads are in-memory, commits return immediately, refresh and
  write-through happen off the caller's thread.
- Good, because the `StorageProvider` contract stays honest: buffered
  `commit`'s weaker "queued, not durable" semantics live only on
  `SyncEngine`, never behind the trait. Strict consumers get a real
  decorator in `MemoryCachingProvider`.
- Good, because correctness is unchanged — the inner conditional write
  still rejects stale `expected_version`s; the engine only moves the
  *timing* of the error, which clients surface via the status surface.
- Good, because the status surface is first-class engine API — a later
  disk-backed replica can expose the same shape without `CoswStore`
  needing the concrete type.
- Bad, because buffered commits are not durable until written — a
  process exit with a non-empty queue loses mutations (bounded by queue
  drain being ASAP; `flush()` on lifecycle events is the interim guard,
  the later disk-backed replica/WAL is the real fix).
- Bad, because `mutate()` can no longer surface `Conflict` to the app in
  Buffered mode — the app must adopt `sync_status`/`take_failed_write`
  polling instead of relying on commit-time errors.
- Bad, because one `Conflict` while draining rejects the rest of the
  queued backlog (each write expected its predecessor's predicted
  version) — the rejected logbooks are preserved, but reconciling them
  is manual work for the user.
- Neutral, because `pending_writes`/`sync_status`/`take_failed_write`/
  `flush` become part of the client contract for replica-backed stores.
- Neutral, because the queue depth is uncapped — accepted because
  mutations are tiny and the worker drains immediately.

## Verification

- [x] `check_provider_conformance` passes against
  `MemoryCachingProvider` over both in-memory and local-filesystem
  providers.
- [x] Engine tests cover: cache-hit reads, optimistic commits,
  synchronous stale-commit rejection, FIFO drain, conflict →
  failed-writes + reconcile, retry-then-success on `Unavailable`,
  status surface, interval refresh picking up external writes, refresh
  preserving optimistic entries, and `Degraded` recovery.
- [ ] `CoswStore::open_cached` exposes the status surface end-to-end on
  Android (the app itself still calls `CoswStore::open`; wiring it over
  plus lifecycle `flush()` is follow-up work).
