# Architecture Decisions

This directory contains decisions that constrain the architecture and should be consulted before implementation.

- [ADR-0001: Use direct clients with storage-provider abstraction](./0001-direct-storage-provider-architecture.md)
- [ADR-0002: Enforce short conditional transactions at the provider boundary](./0002-conditional-storage-transactions.md)
- [ADR-0003: Use a native versioned JSON format](./0003-native-versioned-json-format.md)
- [ADR-0004: Logbook schema and local commit protocol](./0004-logbook-schema-and-local-commit-protocol.md)
- [ADR-0005: cosw CLI command surface](./0005-cosw-cli-command-surface.md)
- [ADR-0006: cosw client config file](./0006-cosw-config-file.md)
- [ADR-0007: S3-compatible remote storage provider](./0007-s3-remote-storage-provider.md)
- [ADR-0008: Client-side envelope encryption and key management](./0008-envelope-encryption-and-key-management.md)
- [ADR-0009: Android client — Kotlin shell over the Rust core via UniFFI](./0009-android-client-architecture.md)

These records are proposed until implementation validates the provider protocol and native schema.
