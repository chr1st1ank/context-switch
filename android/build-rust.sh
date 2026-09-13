#!/usr/bin/env bash
# Build libcontextswitch_uniffi.so for Android ABIs and generate Kotlin bindings.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UNIFFI_CRATE="$REPO_ROOT/libs/contextswitch-uniffi"
APP_MAIN="$REPO_ROOT/android/app/src/main"

# Locate the SDK/NDK without requiring exported env vars.
ANDROID_HOME="${ANDROID_HOME:-/opt/android-sdk}"
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)"
fi
export ANDROID_HOME ANDROID_NDK_HOME
[ -n "$ANDROID_NDK_HOME" ] || { echo "error: no NDK found under $ANDROID_HOME/ndk" >&2; exit 1; }

TARGET_ARGS=()
for abi in arm64-v8a x86_64; do
    TARGET_ARGS+=(-t "$abi")
done

cargo ndk "${TARGET_ARGS[@]}" -o "$APP_MAIN/jniLibs" \
    --manifest-path "$UNIFFI_CRATE/Cargo.toml" build --release

# cargo-ndk copies every cdylib dependency; only the UniFFI facade is loaded.
find "$APP_MAIN/jniLibs" -name "libcontextswitch_core.so" -delete

cargo run --quiet --manifest-path "$UNIFFI_CRATE/Cargo.toml" --bin uniffi-bindgen -- \
    generate \
    --library "$APP_MAIN/jniLibs/arm64-v8a/libcontextswitch_uniffi.so" \
    --language kotlin \
    --out-dir "$APP_MAIN/java" \
    --no-format

echo "Rust build + Kotlin bindings generated into $APP_MAIN"
