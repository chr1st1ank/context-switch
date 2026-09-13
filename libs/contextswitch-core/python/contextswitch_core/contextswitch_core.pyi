"""Type stubs for the contextswitch_core Rust bindings."""

from datetime import datetime

class DomainError(Exception):
    """A domain rule or logbook invariant was violated."""

class StorageError(Exception):
    """A storage provider operation failed."""

class Project:
    """A named work context to which time can be assigned."""

    id: str
    name: str
    archived: bool
    created_at: datetime
    updated_at: datetime

    def __new__(cls, name: str) -> Project: ...

class Tag:
    """A reusable label that can be attached to a span."""

    id: str
    name: str
    archived: bool
    created_at: datetime
    updated_at: datetime

    def __new__(cls, name: str) -> Tag: ...

class Span:
    """A mutable record of a period during which the user records time."""

    id: str
    started_at: datetime
    stopped_at: datetime | None
    project_id: str | None
    tag_ids: list[str]
    created_at: datetime
    updated_at: datetime
    is_active: bool

    def __new__(
        cls,
        started_at: datetime,
        project_id: str | None = None,
        tag_ids: list[str] | None = None,
    ) -> Span: ...

class Logbook:
    """The canonical versioned data logbook.

    Mutations take an explicit ``at`` timestamp (timezone-aware, UTC) so
    timer actions captured offline can be replayed faithfully. The logbook
    is validated on every commit; mutating a snapshot's logbook and
    committing against the observed version is one transaction.
    """

    schema_version: int
    revision: int
    active_span_id: str | None

    def __new__(cls) -> Logbook: ...
    def active_span(self) -> Span | None:
        """The span currently recording time, if any."""
    def project(self, id: str) -> Project | None: ...
    def tag(self, id: str) -> Tag | None: ...
    def span(self, id: str) -> Span | None: ...
    def projects(self) -> list[Project]: ...
    def tags(self) -> list[Tag]: ...
    def spans(self) -> list[Span]: ...
    def start_timer(
        self,
        at: datetime,
        project_id: str | None = None,
        tag_ids: list[str] | None = None,
    ) -> str:
        """Start a new active timer. Raises DomainError if one is running."""
    def stop_timer(self, at: datetime) -> str:
        """Stop the active timer. Raises DomainError if none is running."""
    def switch(
        self,
        at: datetime,
        project_id: str | None = None,
        tag_ids: list[str] | None = None,
    ) -> str:
        """Stop the active span (if any) and start a new one at the same instant."""
    def edit_span(
        self,
        span_id: str,
        at: datetime,
        started_at: datetime | None = None,
        stopped_at: datetime | None = None,
        project_id: str | None = None,
        tag_ids: list[str] | None = None,
    ) -> None:
        """Apply one focused edit. ``None`` leaves a field unchanged.

        ``stopped_at`` may only be changed on an already-stopped span;
        stopping goes through ``stop_timer``/``switch``. To clear a span's
        project use ``unassign_span``.
        """
    def add_span(
        self,
        started_at: datetime,
        stopped_at: datetime,
        project_id: str | None = None,
        tag_ids: list[str] | None = None,
        at: datetime | None = None,
    ) -> str:
        """Insert an already-completed span without touching the active timer.

        ``at`` defaults to now; it only stamps the span's mutation metadata.
        """
    def remove_span(self, span_id: str) -> None:
        """Delete a span. Removing the active span discards the running timer."""
    def unassign_span(self, span_id: str, at: datetime) -> None:
        """Clear a span's project assignment."""
    def add_project(self, name: str, at: datetime) -> str:
        """Create a project; names are case-insensitively unique."""
    def rename_project(self, project_id: str, name: str, at: datetime) -> None: ...
    def set_project_archived(self, project_id: str, archived: bool, at: datetime) -> None: ...
    def add_tag(self, name: str, at: datetime) -> str:
        """Create a tag; names are case-insensitively unique."""
    def rename_tag(self, tag_id: str, name: str, at: datetime) -> None: ...
    def set_tag_archived(self, tag_id: str, archived: bool, at: datetime) -> None: ...
    def to_json(self) -> str:
        """Serialize to the canonical pretty-printed JSON representation."""
    @staticmethod
    def from_json(json: str) -> Logbook:
        """Parse a logbook from its canonical JSON representation."""

class StorageSnapshot:
    """A read of canonical data plus the version it was observed at."""

    version: str
    logbook: Logbook

class LocalFsProvider:
    """Canonical storage as a single JSON logbook on the local filesystem.

    Commits serialize through a ``<file>.lock`` lockfile and write via
    temp-file + atomic rename. Created with the logbook's file path;
    an empty v1 logbook is created if none exists.
    """

    path: str
    location_url: str

    def __new__(cls, path: str) -> LocalFsProvider: ...
    def read(self) -> StorageSnapshot:
        """Read the canonical logbook and its current version."""
    def commit(self, logbook: Logbook, expected_version: str) -> str:
        """Conditionally write ``logbook`` if the stored version still
        equals ``expected_version``. Returns the new version on success;
        raises StorageError on conflict."""

class S3Provider:
    """S3-compatible storage provider with mandatory client-side envelope encryption."""

    location_url: str

    def __new__(
        cls,
        bucket: str,
        region: str,
        prefix: str,
        passphrase: str,
        endpoint: str | None = None,
        use_path_style: bool = False,
        profile: str | None = None,
    ) -> S3Provider: ...
    def read(self) -> StorageSnapshot:
        """Read the canonical logbook and its current version."""
    def commit(self, logbook: Logbook, expected_version: str) -> str:
        """Conditionally write ``logbook`` if the stored version still
        equals ``expected_version``. Returns the new version on success;
        raises StorageError on conflict."""

def decrypt_envelope(envelope: bytes, passphrase: str) -> str:
    """Decrypt a client-side encrypted envelope to plaintext JSON."""

def encrypt_envelope(plaintext: str, passphrase: str) -> bytes:
    """Seal plaintext into a fresh envelope under a new random master key."""
