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

# CI overrides these to skip work a debug-APK job doesn't need:
#   CS_RUST_PROFILE=debug  CS_ABIS=arm64-v8a
PROFILE="${CS_RUST_PROFILE:-release}"
ABIS="${CS_ABIS:-arm64-v8a x86_64}"

TARGET_ARGS=()
FIRST_ABI=""
for abi in $ABIS; do
    TARGET_ARGS+=(-t "$abi")
    FIRST_ABI="${FIRST_ABI:-$abi}"
done
[ -n "$FIRST_ABI" ] || { echo "error: CS_ABIS is empty" >&2; exit 1; }

PROFILE_ARGS=()
if [ "$PROFILE" = release ]; then
    PROFILE_ARGS=(--release)
fi

cargo ndk "${TARGET_ARGS[@]}" -o "$APP_MAIN/jniLibs" \
    --manifest-path "$UNIFFI_CRATE/Cargo.toml" build "${PROFILE_ARGS[@]}"

# cargo-ndk copies every cdylib dependency; only the UniFFI facade is loaded.
find "$APP_MAIN/jniLibs" -name "libcontextswitch_core.so" -delete

cargo run --quiet --manifest-path "$UNIFFI_CRATE/Cargo.toml" --bin uniffi-bindgen -- \
    generate \
    --library "$APP_MAIN/jniLibs/$FIRST_ABI/libcontextswitch_uniffi.so" \
    --language kotlin \
    --out-dir "$APP_MAIN/java" \
    --no-format

echo "Rust build + Kotlin bindings generated into $APP_MAIN"
