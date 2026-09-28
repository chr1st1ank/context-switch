---
status: "proposed"
date: 2026-09-10
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Enforce short conditional transactions at the provider boundary

## Context and Problem Statement

The user switches between devices frequently and considers consistency more important than long-lived offline work. There must be exactly one global active timer. Spans are mutable, but a stale client must never silently overwrite a newer change. Clients connect directly to storage and may temporarily queue timer actions while offline.

How should concurrent and briefly offline mutations be committed safely without requiring interactive merge resolution?

## Decision Drivers

- Preserve the one-active-timer invariant.
- Prevent silent lost updates.
- Keep the conflict window small.
- Permit start/stop/switch capture during short connectivity gaps.
- Avoid locks held during user interaction.
- Keep conflict handling understandable for a single user.

## Considered Options

- Short conditional transactions using version/ETag checks and provider-specific locking where needed.
- Last-write-wins object replacement.
- Long-lived client-held locks.
- Full append-only event sourcing with automatic merge.

## Decision Outcome

Chosen option: "Short conditional transactions using version/ETag checks", because it provides strong stale-write protection and central enforcement without requiring a server or a complex merge model.

### Consequences

- Good, because concurrent writes are serialized by the provider contract.
- Good, because starting a timer and switching can atomically enforce one active timer.
- Good, because small mutations reduce contention.
- Good, because independent offline timer actions can be retried promptly.
- Bad, because a stale write can fail and require the user to export or manually recover the captured time.
- Bad, because each provider must implement real conditional-write semantics; unsupported providers cannot be treated as safe.
- Neutral, because internal revision history may be retained, but user-facing records remain mutable.

## Implementation Plan

- **Affected paths**: future provider interface, transaction coordinator, timer command handlers, local pending-action queue, conflict export/reporting, provider conformance tests.
- **Dependencies**: no dependency selected. Use atomic filesystem replacement/locking locally and the selected remote provider’s conditional-write/lease primitives remotely.
- **Patterns to follow**: read snapshot plus version; construct one focused mutation; conditional commit; reload/retry only when safe; preserve rejected offline captures.
- **Patterns to avoid**: last-write-wins writes; UI-held locks; arbitrary offline historical edits; pretending a provider is transactional when it cannot compare versions.
- **Configuration**: transaction retry limits, short lease duration where required, pending-action retention, conflict export location.
- **Migration steps**: implement local provider semantics and conformance tests first; validate the remote provider against the same scenarios before enabling it.

## Verification

- [ ] Two clients cannot successfully commit two different active timers from the same observed version.
- [ ] Starting a new timer atomically stops the old active span and creates the new one.
- [ ] A stale historical edit cannot overwrite a newer edit.
- [ ] An offline start/stop/switch action is preserved until accepted or exported as a conflict.
- [ ] Locks are not held while a user edits a form.
- [ ] Local and remote providers pass tests for stale writes, retries, and atomic replacement.
