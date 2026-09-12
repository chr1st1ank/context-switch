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

### S3 / remote object storage provider

Implement `StorageProvider` for S3-compatible object storage in
`libs/contextswitch-core` so data can be synchronized across devices.

- Conditional writes via ETags / `If-None-Match`; may need a short
  lease/lock mechanism (`architecture.md` §5)
- Must pass the conformance suite in `src/conformance.rs`
- Credentials come from client config (ADR-0006) and platform credential
  facilities — never from the synced document
- Resolves open questions in `architecture.md` §12: provider choice,
  credential mechanism, lock/conditional-write protocol

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
