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
3. **One global active timer.** The invariant is enforced by the storage transaction, not by client conventions.
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

The native format is a versioned, human-readable JSON document using named object fields. Compatibility with external legacy formats is not a requirement.

The document should contain at least:

- schema version;
- projects;
- tags;
- spans;
- active-span identity or an equivalent representation;
- document/revision metadata sufficient for conditional writes.

The provider interface must support:

- reading a snapshot and its version/ETag;
- conditional commit against the observed version;
- short transaction semantics for mutations;
- provider-specific credential configuration;
- optional encryption wrapping/configuration;
- safe atomic replacement for local files.

A remote object provider may implement the contract with conditional object writes plus a short lease/lock mechanism. A provider that cannot prevent stale overwrites is unsupported.

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

Each client uses user-configured storage credentials. Credentials are stored through the platform’s secure credential facility, not in the synchronized JSON document. Encryption is a storage-provider configuration option. The initial concept does not mandate application-level encryption, but the provider boundary should permit an encryption wrapper later.

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

- Exact remote object-storage provider and credential mechanism.
- Concrete lock/conditional-write protocol for that provider.
- Native JSON schema details and migration policy.
- Shared-core language/runtime strategy.
- Android background behavior and notification requirements.
- Conflict export format and retention policy.
