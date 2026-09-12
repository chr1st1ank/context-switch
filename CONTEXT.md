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
