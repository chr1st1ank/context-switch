# context-switch Time Tracking

The domain of the context-switch single-user, multi-device time-tracking system with synchronized storage and client-generated reports. The laptop command-line client is named `cosw`.

## Language

### Recording

**Span**:
A mutable record of a period during which the user records time. It has a start time, an optional stop time while active, an optional project, and zero or more tags. Spans are half-open intervals that never overlap — time must not be double-recorded — though touching boundaries are allowed.
_Avoid_: time entry, frame, session, booking (unless referring to an external system)

**Active timer**:
The one span whose stop time has not been recorded. The system permits at most one active timer for the user across all clients.
_Avoid_: current frame, running session

**Switch**:
The operation that stops the active timer and starts a new timer in one logical action.
_Avoid_: restart (when describing the new system)

**Resume**:
The operation that starts a new timer copying the project and tags of a previous span, typically after a break. It requires that no timer is active; changing tasks while recording is a switch.
_Avoid_: restart

**Cancel**:
The operation that deletes the active timer's span so that no time is recorded.
_Avoid_: undo, delete

**Unassigned time**:
A span whose project is currently null. It is valid but should remain visible for later classification.
_Avoid_: orphaned time, unknown project

### Classification

**Project**:
A named work context to which time can be assigned. Projects have stable identities independent of their display names.
_Avoid_: workspace, client (unless referring to a platform client)

**Tag**:
A reusable label that can be attached to a span in addition to its project.
_Avoid_: category, keyword

### Synchronization

**Storage provider**:
An implementation of the common interface through which a client reads and conditionally writes synchronized data.
_Avoid_: backend (the initial system has no API backend)

**Canonical data**:
The authoritative versioned domain data held by the selected storage provider.
_Avoid_: server state (there may be no server)

**Logbook**:
The canonical data serialized as a single versioned JSON value: spans, projects, tags, and the revision counter. Storage providers read and conditionally write one logbook.
_Avoid_: database, record, document

**Logbook File**:
The single stored object a storage provider persists the logbook as — `<file>.json` for local storage, `logbook.json` under the configured prefix for object storage. One provider location holds exactly one logbook file.
_Avoid_: data file, blob, data.json

**Envelope**:
The self-describing byte format a cipher produces when sealing a logbook for storage: a header (format version, method, key identity, wrapped-key list) bound as associated data, followed by a nonce and the authenticated ciphertext. Envelopes are opaque to the storage provider.
_Avoid_: encrypted blob, ciphertext (too narrow; excludes the header)

**Revision**:
The logbook's monotonically increasing write counter, used as the opaque version a client reads and commits against. Distinct from a blob store's ETag, which exists only to implement compare-and-swap at the byte level.
_Avoid_: version (ambiguous with schema version), generation

**Master key**:
The random symmetric key that encrypts a logbook's plaintext. It is never derived from the passphrase directly; instead it is generated independently and then wrapped (encrypted) under a key derived from the passphrase, so the passphrase can rotate without re-encrypting history.
_Avoid_: encryption key (ambiguous with the passphrase-derived wrapping key)

**Pending offline action**:
A locally cached timer lifecycle action that has not yet been accepted by canonical storage.
_Avoid_: committed event, synchronized event

**Conflict**:
A failed attempt to commit because the client wrote against a stale version of canonical data. The system preserves the local capture and reports the conflict; interactive merge resolution is out of scope initially.
_Avoid_: merge (unless referring to a future explicit feature)

### Clients and reporting

**Client**:
An application such as the CLI, Android app, or reporting dashboard that uses the storage-provider interface.
_Avoid_: frontend (too narrow for the CLI)

**Report**:
A view or export generated on demand from canonical spans, projects, and tags rather than persisted report data.
_Avoid_: materialized report

**Degraded read-only mode**:
A client mode that serves cached data and reports while connectivity is unavailable, but does not accept arbitrary edits.
_Avoid_: offline database
