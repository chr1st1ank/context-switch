---
status: "accepted"
date: 2026-09-13
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# S3-compatible remote storage provider

## Context and Problem Statement

ADR-0001 chose direct clients over an API server, and ADR-0004 defined the
local commit protocol. Canonical data still lived on exactly one machine.
`docs/architecture.md` §12 left the remote provider, its credential
mechanism, and its conditional-write protocol as open questions. This
record resolves them and the blob-level contract used to implement them.

## Decision Drivers

- Two devices racing to start a timer must produce a conflict, never two
  active timers — the remote provider must offer the same
  conditional-write guarantee as the local lockfile.
- No lease/lock object: managing lease duration, renewal, fencing, and
  stale-lease reclamation is exactly the complexity a conditional-write
  backend avoids.
- Credentials must reuse a mechanism users already have, not a bespoke one.
- The logbook layer above the provider contract must not change.

## Considered Options

- A lease/lock object alongside the data object vs. native conditional
  writes (`If-Absent`/`If-Match`) only.
- Bespoke credential file vs. standard AWS environment variables/shared
  credentials file/profile.
- Monolithic `S3Provider` vs. composing a blob store with a cipher (see
  ADR-0008) behind the existing `StorageProvider` contract.

## Decision Outcome

### Blob-level contract

A new layer beneath the logbook layer, so encryption (ADR-0008) and
storage backend are independent axes of composition:

```rust
pub enum Precondition {
    IfAbsent,      // create-only
    IfMatch(ETag), // compare-and-swap
}

pub trait BlobStore: Send + Sync {
    fn get(&self) -> Result<Option<(Vec<u8>, ETag)>, StorageError>;
    fn put(&self, bytes: &[u8], cond: Precondition) -> Result<ETag, StorageError>;
}
```

`LocalFsBlobStore` synthesizes its ETag as a SHA-256 content hash, compared
under the existing `<file>.lock` lockfile, so both backends expose identical
precondition semantics to the layer above. `S3BlobStore` uses the object
store's native ETag via `If-Match`/`If-None-Match`. `InMemoryBlobStore`
exists purely for fast, offline tests.

**Version identity stays the logbook revision, not the ETag.** The ETag is
used only inside a provider's own compare-and-swap; the version a client
reads and commits against is always the logbook's `revision` counter
(ADR-0004). This keeps conflict messages human-meaningful and keeps the
Android reimplementation identical between backends.

**No lease or lock object.** Conditional writes are a hard requirement; a
backend that cannot honour `If-Absent`/`If-Match` is rejected rather than
approximated with a lease. (Automatically probing for this at configuration
time is not yet implemented — see `docs/backlog.md`.)

**Object layout and bootstrap.** A single object per location, at a
configurable key prefix, named `logbook.json` (see the domain glossary
entry for "Logbook File" in `CONTEXT.md`). No sibling lock or temporary objects.
Absence reads as an empty logbook at revision zero; the first commit
creates the object with `IfAbsent`. Opening therefore performs no network
write and needs no write permission until the first mutation.

### Credentials

Sourced from the standard AWS mechanisms only: `AWS_ACCESS_KEY_ID` /
`AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN` environment variables first,
then `~/.aws/credentials`, optionally scoped to a named profile from client
config. No bespoke credential file or format. Credentials never appear in
the synchronized logbook.

### Request signing

Implemented directly (AWS Signature Version 4) rather than depending on a
full AWS SDK, since only `GET`/`PUT` against a single, known object are
needed. The signed canonical URI is percent-encoded per the SigV4 rules
(unreserved characters and `/` only) so a prefix containing spaces or other
special characters signs correctly.

### Error taxonomy

`StorageError` gains `Unauthorized` (credential/signature problems) and
`Unavailable` (network/unexpected-status problems) variants alongside the
existing `Conflict`/`Corrupt`/`InvalidData`/`Locked`. A 403 response is
disambiguated by the S3 XML error body's `<Code>` element (e.g.
`AccessDenied` vs. `SignatureDoesNotMatch`) rather than reported with one
generic message that conflates a credentials problem with a client-side
signing bug.

### Client configuration

The `[storage]` table (ADR-0006) gains `provider` (`"local"` | `"s3"`),
`bucket`, `region`, `prefix`, `endpoint`, `use_path_style`, `profile`, and
`passphrase_command`. No secrets. `cosw`'s provider construction dispatches
on `storage.provider` via a factory instead of a hardcoded
`LocalFsProvider`; `cosw status --verbose`'s storage line reads a
`location_url` property both providers expose, instead of assuming a local
path.

### Consequences

- Good, because the same conformance suite (`src/conformance.rs`) exercises
  both providers — "implements the contract" is verified, not asserted.
- Good, because opening a provider never requires network access until the
  first write.
- Bad, because a storage backend without real conditional writes is simply
  unsupported; there is no degraded lease-based fallback.
- Neutral, because retry/backoff for transient failures and a
  conditional-write capability probe are not yet implemented (tracked in
  `docs/backlog.md`).

## Implementation Plan

- **Affected paths**: `libs/contextswitch-core/src/{blob,s3,provider,storage}.rs`;
  `cli/cosw/{config,core}.py`; conformance and encrypted-provider tests.
- **Patterns to follow**: compose a `BlobStore` + `Cipher` into
  `GenericProvider`; keep version identity as the logbook revision;
  surface backend-specific failures through the shared `StorageError`
  taxonomy.
- **Patterns to avoid**: leaking ETags as the client-visible version;
  a second, S3-specific commit protocol; storing credentials or passphrases
  in the config file.
- **Migration steps**: none — this is a new provider; existing local
  storage is untouched.

## Verification

- [x] Both `LocalFsProvider` and `S3Provider`-shaped compositions pass
  `check_provider_conformance`.
- [x] A backend lacking `If-Match` support (simulated by tampering the
  in-memory store's compare-and-swap) is caught as a `Conflict`, not a
  silent overwrite.
- [x] Credentials are never written into the synchronized logbook.
- [ ] Transient network failures are retried with bounded backoff (not yet
  implemented; `docs/backlog.md`).
