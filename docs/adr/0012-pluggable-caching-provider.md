---
status: "accepted"
date: 2026-09-23
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Pluggable caching provider with a buffered write queue

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
- The cache must be optional: hooked in at startup for clients that
  want it (the long-lived Android store), left out for others (the
  CLI, which stays on the raw S3 provider — each invocation is a fresh
  process where a memory cache buys little).
- Rejected mutations must be preserved for the user, per
  architecture §6 / ADR-0002.
- Both an in-memory and an on-disk cache should be possible behind the
  same design; the memory variant ships first.

## Considered Options

- `MemoryCachingProvider` — a `StorageProvider` decorator wrapping any
  inner provider, with a snapshot cache, an interval-based background
  refresh, and a worker-drained write queue.
- Polling sync inside `GenericProvider` (extend ADR-0010's CachedHead
  into a full snapshot cache).
- A client-side cache in the UniFFI layer instead of the core library.

## Decision Outcome

Chosen option: `MemoryCachingProvider` in `contextswitch-core`, because
it sits inside the existing abstraction — wiring it in is a constructor
decision at startup, and every consumer of `StorageProvider` (including
the conformance suite and any future provider) works unchanged. The
UniFFI-layer alternative would leave the Rust API without the benefit
and duplicate logic the core already owns; folding it into
`GenericProvider` would couple cache policy to one provider instead of
composing over all of them.

### Read path

`read()` serves the cached `StorageSnapshot` immediately. When the
entry is older than `refresh_interval` (default 30 s) it signals the
background worker to revalidate; staleness is bounded by the interval.
Freshness is checked with a new defaulted `StorageProvider::fingerprint()`
— a blob-level change token backed by `BlobStore::stat()` (HEAD→ETag on
S3, `mtime:size` locally, stored etag in memory) — so a hit costs a HEAD
request instead of a full GET + decrypt + parse. `fingerprint` is a
different token than the snapshot `version` (logbook revision) and the
two are never compared.

### Commit path — two modes

`CommitMode::Sync` delegates synchronously (write-through, invalidate on
error) and preserves the exact `StorageProvider` contract — the
conformance suite runs against it. `CommitMode::Buffered` validates the
logbook in memory, rejects a provably stale `expected_version`
synchronously against the cached version, installs an optimistic entry
(predicted version = expected + 1), queues the write on an uncapped
queue, and returns the predicted version immediately.

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
   `take_failed_write()` / `sync_status()` / `flush()`. A `Conflict`
   additionally triggers an immediate reconcile refresh so the cache
   reconverges on canonical data.
3. **Refresh errors**: probe/read failures in the worker only set a
   `Degraded` status and retry next interval — the cached snapshot keeps
   being served, which is also the offline-read path of
   architecture §7.

Monotonic installs (`newer_or_equal` on the snapshot version) prevent a
refresh from clobbering a newer, possibly optimistic, entry. On a failed
buffered write the optimistic entry is dropped only if it still carries
that write's predicted version.

### Wiring

UniFFI gains `CoswStore::open_cached(config, refresh_interval_secs)`
(Buffered mode) plus `pending_writes()`, `sync_status()`,
`take_failed_write()` returning the rejected logbook as JSON, and
`flush()` for app pause/exit. `CoswStore::open` is unchanged and
uncached. The CLI is deliberately not wired — it keeps using the
underlying S3 provider directly.

### Consequences

- Good, because after the cold read no UI-visible call performs storage
  I/O: reads are in-memory, commits return immediately, refresh and
  write-through happen off the caller's thread.
- Good, because correctness is unchanged — the inner conditional write
  still rejects stale `expected_version`s; the cache only moves the
  *timing* of the error, which clients surface via the status surface.
- Good, because `Sync` mode lets the conformance suite validate the
  decorator for free and gives non-buffered clients a strict contract.
- Bad, because buffered commits are not durable until written — a
  process exit with a non-empty queue loses mutations (bounded by queue
  drain being ASAP; `flush()` on lifecycle events is the interim guard,
  the later `DiskCachingProvider`/WAL the real fix).
- Bad, because `mutate()` can no longer surface `Conflict` to the app in
  Buffered mode — the app must adopt `sync_status`/`take_failed_write`
  polling instead of relying on commit-time errors.
- Neutral, because `pending_writes`/`sync_status`/`take_failed_write`/
  `flush` become part of the client contract for cache-enabled stores.
- Neutral, because the queue depth is uncapped — accepted because
  mutations are tiny and the worker drains immediately.

## Verification

- [ ] `check_provider_conformance` passes against `MemoryCachingProvider`
  in `Sync` mode over both in-memory and local-filesystem providers.
- [ ] Cache unit tests cover: cache-hit reads, optimistic commits,
  synchronous stale-commit rejection, FIFO drain, conflict →
  failed-writes + reconcile, retry-then-success on `Unavailable`,
  status surface, interval refresh picking up external writes, and
  refresh preserving optimistic entries.
- [ ] `CoswStore::open_cached` exposes the status surface end-to-end on
  Android.
