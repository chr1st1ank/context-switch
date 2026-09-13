# context-switch System Architecture Concept

**Status:** Proposed concept, confirmed through the architecture interview on 2026-09-10.

The application is named **context-switch**. Its laptop command-line client is named **`cosw`**.

## 1. Purpose and scope

This system provides time tracking for one user across multiple devices. The user can start, stop, and switch a timer; maintain projects and tags; correct historical spans; and generate reports from raw time data.

Initial clients are:

- the `cosw` CLI for laptops;
- an Android application;
- a separate reporting dashboard.

The first release does not include an API server or multi-user collaboration.

## 2. Architectural principles

1. **Central consistency over long-lived offline work.** The selected storage provider is authoritative. Clients may cache data, but they must not silently overwrite newer data.
2. **Short transactions.** Clients read a version, make one small mutation, and conditionally commit it. User editing must never hold a storage lock.
3. **One global active timer, no overlapping spans.** These invariants are enforced by the storage transaction, not by client conventions. Time must not be double-recorded.
4. **Portable contracts.** Versioned JSON schemas and protocol specifications are the cross-language source of truth. Shared code is used where practical, but platform languages are not forced to match.
5. **Reports are derived.** Reports are computed on demand from raw synchronized records.
6. **Provider independence.** Local filesystem and remote object storage implement the same provider contract. Provider-specific credentials and optional encryption stay outside the domain model.

## 3. Logical components

```text
cosw ──────────────┐
Android ───────────┼── client domain/sync/report core where practical
Dashboard ────────┘
                         │
                 storage-provider interface
                         │
             ┌───────────┴───────────┐
             │                       │
       local filesystem       remote object storage
```

A client contains platform UI/lifecycle integration, cached data and pending offline actions, and the common domain/synchronization/reporting logic where its language permits. The dashboard is a client, not a reporting service.

## 4. Domain model

### Span

A span has a stable ID, `started_at`, nullable `stopped_at`, nullable `project_id`, zero or more tag IDs, and mutation metadata. Spans are mutable from any authorized client. An active span has `stopped_at = null`.

There can be only one active span. A switch transaction stops the existing active span at the switch time and creates the new active span with its selected project and tags. A normal start requires a project selected from the client’s cached project list; unassigned remains a valid fallback for exceptional cases.

### Project and tag

Projects and tags have stable IDs and mutable display metadata. Renaming must not change historical identity. A span may have no project and multiple tags.

## 5. Canonical data and storage contract

The native format is a versioned, human-readable JSON logbook using named object fields. Compatibility with external legacy formats is not a requirement.

The logbook should contain at least:

- schema version;
- projects;
- tags;
- spans;
- active-span identity or an equivalent representation;
- logbook/revision metadata sufficient for conditional writes.

The provider interface must support:

- reading a snapshot and its version/ETag;
- conditional commit against the observed version;
- short transaction semantics for mutations;
- provider-specific credential configuration;
- optional encryption wrapping/configuration;
- safe atomic replacement for local files.

A remote object provider must implement the contract with conditional object writes (`If-Absent`/`If-Match`); a backend that cannot honour those preconditions is rejected rather than approximated with a lease or lock object (ADR-0007).

### Storage stack layering

Implemented as a three-layer stack rather than a monolithic provider (`libs/contextswitch-core/src/{storage,provider,blob,crypto}.rs`):

- **Logbook layer** (`storage::StorageProvider`, `provider::GenericProvider`): owns serialization, whole-logbook validation, the revision counter, and conflict detection. Unaware of bytes-on-the-wire or encryption.
- **Cipher layer** (`crypto::Cipher`): seals/opens a logbook's plaintext bytes into/from a self-describing envelope. Storage-agnostic — the same cipher works for any blob store.
- **Blob layer** (`blob::BlobStore`): conditional byte-level `get`/`put` against `Precondition::IfAbsent`/`IfMatch`. `LocalFsBlobStore` synthesizes its ETag as a content hash under the existing lockfile; `S3BlobStore` uses the object store's native ETag; `InMemoryBlobStore` exists for fast offline tests.

`GenericProvider` composes one `BlobStore` and one `Cipher`: local storage is the file blob store with an identity cipher, remote storage is the S3 blob store with the AEAD cipher. The opaque version a client reads/commits against remains the logbook revision, never the blob store's ETag — see ADR-0007 for why the two must stay independent. See `docs/envelope-format.md` for the cipher layer's on-disk byte format and ADR-0008 for the key-management design.

## 6. Synchronization and conflicts

The normal mutation flow is:

1. Read canonical data and its version.
2. Apply one focused mutation in memory.
3. Commit conditionally against that version.
4. On success, update the client cache.
5. On stale-version failure, reload and reapply only if the operation is unambiguously independent; otherwise preserve the local mutation as conflict data.

The initial user experience does not provide interactive merge resolution. Conflicted offline timer captures remain available for export or copying into a message.

## 7. Offline behavior

The system is online-first. When disconnected, clients provide degraded read-only access to cached projects, spans, and reports. The only initial writes allowed offline are timer lifecycle actions: start, stop, and switch. A start includes a cached project and tags where possible. These actions are queued locally and synchronized immediately when connectivity returns.

Offline actions are not canonical until accepted by the provider. Historical editing, project maintenance, and arbitrary cached mutations require connectivity in the initial design.

## 8. Reporting

Reports are computed on demand by clients from raw spans. The initial reporting vocabulary should include:

- detailed log of spans;
- totals by project;
- totals by tag;
- time-series aggregation by day and other ranges;
- filtering by project/tag and date range;
- machine-readable output for future export workflows.

No report database or central reporting service is required.

## 9. Export and interoperability

Export is a future client capability. Exporters read the native domain model and produce selected formats. The native schema must therefore be versioned and stable, but it must not be constrained by any export format not yet chosen.

## 10. Security and configuration

Each client uses user-configured storage credentials. Credentials are stored through the platform's secure credential facility, not in the synchronized JSON logbook.

For remote storage, client-side envelope encryption is mandatory, not optional (ADR-0008): a randomly generated master key encrypts the logbook; the master key is itself wrapped under a key derived from the user's passphrase via Argon2id, and the wrapped copy travels inside the stored object. The storage provider never sees plaintext, the passphrase, or the master key. Local filesystem storage is unaffected — it composes the same logbook/blob layering with an identity cipher (§5).

## 11. Explicit non-goals

- API server in the initial architecture;
- multi-user accounts, sharing, or permissions;
- long-lived offline editing;
- automatic interactive conflict resolution;
- broad support for arbitrary storage providers;
- compatibility with external legacy file formats;
- persisted report read models;
- choosing a final export format before the domain model stabilizes.

## 12. Open design questions for implementation

- ~~Exact remote object-storage provider and credential mechanism.~~ Resolved: S3-compatible object storage with AWS-standard credential sourcing (env vars, shared credentials file, named profile); see ADR-0007.
- ~~Concrete lock/conditional-write protocol for that provider.~~ Resolved: native conditional writes only (`If-Absent`/`If-Match`), no lease/lock object; a backend lacking them is rejected. See ADR-0007.
- Native JSON schema details and migration policy.
- Shared-core language/runtime strategy.
- Android background behavior and notification requirements.
- Conflict export format and retention policy.
- Passphrase rotation's home in the provider contract (logbook-layer operation vs. crypto-layer operation driven by the client) — see `docs/backlog.md`.
