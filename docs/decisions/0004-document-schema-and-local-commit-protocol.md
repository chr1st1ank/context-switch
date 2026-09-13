---
status: "proposed"
date: 2026-09-10
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Document schema and local commit protocol

## Context and Problem Statement

ADR-0003 chose a native versioned JSON format and ADR-0002 chose short
conditional transactions. The cross-language contract still needed a concrete
shape: the exact document fields, how the active timer is represented, which
invariants a commit enforces, and how a filesystem provides atomic
compare-and-write without a server.

## Decision Drivers

- The document is the cross-language source of truth; Android will reimplement it.
- The one-active-timer invariant must be enforced by the commit, not by client
  convention.
- Canonical data must never be silently repaired or overwritten.
- Timer actions captured offline replay against a fresh read, so mutations take
  explicit timestamps.
- Local commits must be atomic and safe against crashed processes.

## Considered Options

- Derive the active timer by scanning `stopped_at` vs. store an explicit
  `active_span_id`.
- Hard delete vs. an `archived` flag for projects and tags.
- Lockfile via `create_new` vs. OS `flock` vs. no lock for the local commit.

## Decision Outcome

### Document schema (version 1)

```json
{
  "schema_version": 1,
  "revision": 0,
  "active_span_id": null,
  "projects": { "<uuid>": { "id", "name", "archived", "created_at", "updated_at" } },
  "tags":     { "<uuid>": { "id", "name", "archived", "created_at", "updated_at" } },
  "spans":    { "<uuid>": { "id", "started_at", "stopped_at", "project_id", "tag_ids", "created_at", "updated_at" } }
}
```

- Collections are objects keyed by UUID; timestamps are RFC 3339 UTC;
  `stopped_at` and `project_id` are nullable.
- `revision` is a monotonic counter incremented by the provider on every
  commit and surfaced through the provider contract as the opaque version
  string.
- `active_span_id` is the authoritative pointer to the one active span. It is
  `null` when no timer runs, and otherwise points at the unique span whose
  `stopped_at` is `null`. It only changes as part of a switch (old span
  stopped, new one started at the same instant) or a stop (cleared to
  `null`). A document where it disagrees with the spans is corrupt — it is
  never silently repaired.
- Projects and tags are never hard-deleted; `archived` hides them from
  pickers while preserving history. Names are case-insensitively unique,
  including archived records.

### Commit validation

Every commit validates the whole document: schema version, `active_span_id`
consistency, referential integrity (`project_id`/`tag_ids` exist),
`started_at <= stopped_at`, non-overlapping spans (the active span is
unbounded, so nothing may end after its start), and name uniqueness. Stopping the active span
goes exclusively through `stop_timer`/`switch`; span edits may change
`started_at`, `project_id`, `tag_ids`, and may correct `stopped_at` on
already-stopped spans only. Mutations take an explicit `at` timestamp so
offline captures replay faithfully.

### Local commit protocol

`LocalFsProvider` is constructed with a file path and creates an empty v1
document if absent. A commit:

1. validates the document;
2. acquires `<file>.lock` via `create_new`, retrying briefly (up to ~5 s) and
   reclaiming lockfiles older than ~30 s (abandoned by a crashed process);
3. re-reads the stored document and fails with a conflict if its revision
   differs from the expected version;
4. bumps `revision`, writes `<file>.tmp`, fsyncs, atomically renames over the
   data file, and fsyncs the directory;
5. releases the lock and returns the new version string.

A file that fails parsing or validation reads as corrupt rather than being
overwritten.

### Consequences

- Good, because the invariants live in the shared core and are re-checked at
  the commit boundary regardless of client behavior.
- Good, because the lockfile protocol needs no extra dependencies and works
  on every filesystem that supports atomic create and rename.
- Bad, because a crash during commit can leave a stale lockfile; reclamation
  bounds that cost to a one-time delay.
- Bad, because `active_span_id` denormalizes state that must be kept
  consistent; mitigated by commit-time validation.
- Neutral, because the version string is opaque — a remote provider may map
  it to an ETag.

## Implementation Plan

- **Affected paths**: `libs/contextswitch-core/src/domain.rs`,
  `storage.rs`, `conformance.rs`, `lib.rs`; `python/contextswitch_core/`
  stubs; provider conformance tests.
- **Patterns to follow**: explicit `at` timestamps on mutations; focused
  mutations that keep `active_span_id` consistent by construction; commit
  returns the new version; conflicts are reported, never retried silently.
- **Patterns to avoid**: silent repair of divergent `active_span_id`;
  hard deletion of projects/tags; last-write-wins writes; a second code path
  that can stop the active span.
- **Migration steps**: this defines schema version 1. A future version 2 adds
  a migration step in the provider read path, never in-place rewrites.

## Verification

- [x] Two clients cannot commit two different active timers from the same
  observed version (conformance suite).
- [x] A switch atomically stops the old span and starts the new one.
- [x] A stale commit fails with a conflict; the stored document is unchanged.
- [x] A divergent `active_span_id` reads as corrupt.
- [x] Locks are held only for the compare-and-write, never during edits.

## Amendment (2026-09-13, see ADR-0007)

The local commit protocol described here is now implemented as a
composition of a document layer (this ADR's validation and revision
rules, unchanged) over a `LocalFsBlobStore` blob layer (ADR-0007) with an
identity cipher. The lockfile, atomic rename, and stale-lock reclamation
behavior described above is unchanged; it now lives in
`blob::LocalFsBlobStore` rather than directly in `storage::LocalFsProvider`.
`LocalFsProvider`'s own bootstrap (creating an empty document if none
exists) tolerates losing a first-write race to another process: the
losing side's own `IfAbsent` put failing with a conflict is not surfaced
as an error opening the provider, since the document exists either way.
