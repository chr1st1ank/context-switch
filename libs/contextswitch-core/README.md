# contextswitch-core

Shared domain model and storage provider interface for context-switch, implemented in Rust with Python bindings.

## Overview

This library provides:

- **Domain model**: Span, Project, Tag, and related types (implemented in Rust)
- **Storage provider interface**: Abstract interface for reading and writing synchronized data
- **Python bindings**: Full Python interoperability via PyO3
- **Type safety**: Leverages Rust's type system for correctness

## Installation

Build the Rust extension:

```bash
cd libs/contextswitch-core
cargo build --release
```

Or install via the workspace:

```bash
uv sync
```

## Development

### Rust

```bash
task build      # build the library
task lint       # check code with clippy
task fmt        # format code
task test       # run tests
task doc        # build and open documentation
```

### Python

The library is exposed to Python via PyO3. Python code can import and use the types:

```python
from contextswitch_core import Project, Tag, Span
from datetime import datetime
from uuid import uuid4

project = Project(
    id=str(uuid4()),
    name="My Project",
    created_at=datetime.now().isoformat(),
    updated_at=datetime.now().isoformat(),
)
```

## Architecture

The Rust implementation provides:

- **Performance**: Core domain logic runs at native speed
- **Memory safety**: Rust's ownership system prevents common bugs
- **Serialization**: Serde integration for JSON and other formats
- **Python integration**: PyO3 bindings for seamless Python interop

## Domain model

### Span

A mutable record of a period during which the user records time.

```rust
pub struct Span {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub project_id: Option<Uuid>,
    pub tag_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Project

A named work context to which time can be assigned.

```rust
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Tag

A reusable label that can be attached to a span.

```rust
pub struct Tag {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## Storage provider interface

The `StorageProvider` trait defines the contract for reading and writing synchronized data:

```rust
pub trait StorageProvider: Send + Sync {
    fn read(&self) -> Result<StorageSnapshot, StorageError>;
    fn commit(
        &self,
        snapshot: StorageSnapshot,
        expected_version: &str,
    ) -> Result<bool, StorageError>;
}
```

Implementations can use local filesystem, remote object storage, or other backends.

## See also

- [System Architecture](../../docs/architecture.md)
- [Domain Language](../../CONTEXT.md)
