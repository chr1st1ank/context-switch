---
status: "accepted"
date: 2026-09-14
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Commit against a cached head; let the conditional write enforce atomicity

## Context and Problem Statement

Every S3-backed action cost ~5 s: `GenericProvider::commit` re-GET the blob
it had just read — solely to re-learn the revision, the ETag to
precondition on, and the envelope `KeyState` — and the Android
`CoswStore::mutate` re-read once more after committing. That is 3–4
sequential round trips (≈300 ms each to a remote endpoint) per action.

## Decision Drivers

- One round trip is the unit of latency; each avoided request saves
  ~300-800 ms.
- The stale-write guarantee must not weaken.
- The change must hold for both `S3BlobStore` and `LocalFsBlobStore`.

## Considered Options

- Cache the observed head (revision, ETag, key state) and commit directly
  with a conditional PUT.
- Keep the pre-GET in commit (status quo).
- Per-operation provider instances that cannot cache at all.

## Decision Outcome

Chosen option: "Cache the observed head and commit directly", because the
pre-GET never provided atomicity anyway — a racing writer can always slip
in between the GET and the PUT. Atomicity rests on the blob store's
conditional write (`If-Match`/`If-None-Match` on S3, the lockfile-checked
content hash on the local filesystem), which is unchanged.

`read()` records `(version, etag, key_state)`; `commit` skips the GET when
`expected_version` matches the cached head, and on a `Conflict` (or any PUT
error) invalidates the cache and re-reads once so the reported `actual`
revision is truthful and the head is fresh for a retry.

### Consequences

- Good, because a mutation drops from 3 requests to 2 (GET + PUT) and one
  Argon2 derivation is removed along with the dropped GET.
- Good, because a commit against an absent blob now skips the GET too.
- Bad, because a backend that silently ignores `If-Match` now fails open
  instead of failing closed — the pre-GET used to provide a second,
  non-atomic check. This is accepted: AWS S3 and Scaleway Object Storage
  both document conditional writes, and the backlog's conditional-write
  probe (story 44) and precondition-honoring conformance check (story 57)
  become the guard for unknown endpoints rather than a per-commit GET.
- Neutral, because `GenericProvider` now carries per-instance mutable
  state (`Mutex<Option<CachedHead>>`).

## Verification

- [ ] Conformance suite still passes for all providers, including
  `stale_commit_is_a_conflict` and `concurrent_commits_have_a_single_winner`.
- [ ] Two processes committing against the same observed revision produce
  exactly one winner on a real S3 endpoint.
