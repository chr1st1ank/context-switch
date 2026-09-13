# context-switch Android

The Android client for context-switch time tracking.

## Architecture

Thin Kotlin/Compose shell over `contextswitch-core`. All domain logic,
storage providers (local filesystem, S3 with envelope encryption), and
conditional-write semantics live in Rust; the app calls into it through
UniFFI-generated Kotlin bindings (`libs/contextswitch-uniffi`).

- Timer lifecycle: start, stop, switch, cancel
- Project and tag management (create, rename, archive)
- Span log: view, edit, delete, add past spans
- Persistent timer notification via a foreground service (live chronometer,
  stop action — like a podcast player)
- Offline reads via a cached snapshot; mutations require connectivity

See ADR-0009 (`docs/decisions/`) for the decision record.

## Prerequisites

- JDK 17+ (`pacman -S jdk17-openjdk`)
- Android SDK: `platform-tools`, `platforms;android-35`, `build-tools`, `ndk`
  (install via `android sdk`/`sdkmanager`; set `ANDROID_HOME`)
- Rust: `rustup target add aarch64-linux-android x86_64-linux-android`
  and `cargo install cargo-ndk`
- Gradle wrapper: run `gradle wrapper` once (any Gradle ≥8.9) or open the
  project in Android Studio to generate `gradlew`

## Build and test

```bash
task rust      # cargo-ndk build + UniFFI Kotlin bindings
task build     # assembleDebug APK (includes the Rust step)
task install   # install on connected device via adb
task lint      # gradle lint
task test      # unit tests
```

Output APK: `app/build/outputs/apk/debug/app-debug.apk`.

## Configuration

The app is configured entirely in its Settings screen (S3 bucket, region,
prefix, endpoint, path-style flag, credentials, envelope passphrase).
Secrets are kept in a private SharedPreferences file; see ADR-0009 for the
credential-storage note.
