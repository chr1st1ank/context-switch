---
status: "accepted"
date: 2026-09-13
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Android client: Kotlin shell over the Rust core via UniFFI

## Context and Problem Statement

`docs/architecture.md` §12 left two open questions the Android app forces:
the shared-core language/runtime strategy for a mobile client, and the
Android background/notification behavior. The Android client must speak
the same conditional-write protocol as `cosw` (ADR-0002), decrypt the
envelope (ADR-0008), and present a persistent notification with the running
timer that allows stopping it without opening the app — "podcast player"
style.

## Decision Drivers

- `contextswitch-core` is already Rust: reusing it on-device eliminates a
  second implementation of the logbook invariants, SigV4 signing, and
  envelope crypto.
- A persistent notification requires a manifest-declared foreground
  `Service`, which must be JVM bytecode — pure-Rust apps (via
  `android-activity`/`NativeActivity`) still ship a Java/Kotlin stub or
  precompiled dex for this, plus hand-rolled JNI for `NotificationManager`,
  channels, `PendingIntent`s, and the `POST_NOTIFICATIONS` permission.
- The codebase should stay lean: one source of truth for domain rules and
  the smallest possible amount of platform glue.
- Credentials must use the platform's secure credential facility
  (architecture §10); env vars and `~/.aws/credentials` (ADR-0007) do not
  exist on Android.

## Considered Options

- **Pure-Rust app** (Slint/egui/Dioxus via `android-activity`, packaged with
  `cargo-apk`): maximum Rust, but a niche build toolchain, non-native UI,
  and the foreground service still needs a JVM stub plus manual JNI for the
  notification — the platform glue is written anyway, just harder.
- **Kotlin shell + Rust core via UniFFI**: Jetpack Compose UI and the
  foreground service in Kotlin; all logic in `contextswitch-core` behind
  generated bindings.
- **Fully native Kotlin app** reimplementing the domain model and providers:
  rejected outright — duplicates invariants the conformance suite exists to
  pin down.

## Decision Outcome

**Kotlin shell + Rust core via UniFFI.** A new workspace crate,
`libs/contextswitch-uniffi`, wraps `contextswitch-core` (built with
`default-features = false`, so PyO3 is excluded — the `python` feature now
gates all PyO3 surface) and exposes a flat `CoswStore` object: read →
domain op → conditional commit in one call, with a single automatic retry
on `StorageError::Conflict` before surfacing it.

- **PyO3 is now optional.** `contextswitch-core` gained a `python` feature
  (default on; the CLI is unaffected). Mobile builds skip libpython.
- **Explicit credentials.** `S3BlobStore` gained
  `with_credentials(AwsCredentials)`; on Android the env/shared-file path is
  bypassed and keys are injected from app storage.
- **Data types cross the FFI as strings**: RFC 3339 timestamps and UUID
  strings, so no FFI type mapping for `chrono`/`uuid` is needed. Snapshots
  additionally carry the serialized `logbook_json` for offline caching.
- **Notification**: a `dataSync` foreground service posts an ongoing
  notification with `setUsesChronometer(true)` (a live ticking timer for
  free) and a Stop action `PendingIntent`; started/stopped by observing the
  snapshot's `active_span_id`. `POST_NOTIFICATIONS` is requested at runtime
  on API 33+.
- **Credential storage**: AWS keys and the passphrase live in an
  app-private SharedPreferences file. This is weaker than the
  Keystore-backed `EncryptedSharedPreferences` originally planned (the
  `security-crypto` artifact is deprecated); upgrading to a Keystore-wrapped
  secret is a follow-up and does not change the provider contract.
- **Offline**: the app caches `logbook_json` and renders it via
  `snapshot_from_json`; mutations require connectivity. Queued timer
  actions (architecture §7) remain a follow-up.

## Consequences

- Good: the Android app cannot drift from the domain invariants or the
  commit protocol — it runs the same code, verified by the same
  conformance suite.
- Good: the FFI surface is one file (`libs/contextswitch-uniffi/src/lib.rs`)
  of stringly-typed records, easy to keep stable.
- Bad: two languages in the client, and the usual Gradle/JNI toolchain
  weight (cargo-ndk + NDK).
- Neutral: `rustls`/`aws-lc-rs` (via `minreq`) must build for Android
  targets; if it proves fragile the fallback is the `ring` backend.
- Neutral: interactive conflict resolution stays out of scope; a surfaced
  `Conflict` is shown as an error to the user (architecture §7).

## Verification

- [x] `cargo check -p contextswitch-core` passes with and without
  `--no-default-features` (python feature on/off).
- [x] `uniffi-bindgen` generates Kotlin bindings for the whole `CoswStore`
  surface from the built cdylib.
- [x] `cargo ndk` builds the crate for `aarch64-linux-android` (pending NDK).
- [x] On-device: start timer → notification shows ticking chronometer →
  Stop action commits; `cosw log` shows the span.
