"""Type stubs for contextswitch_core Rust bindings."""

from typing import Optional

class Project:
    """A named work context to which time can be assigned."""

    id: str
    name: str
    created_at: str
    updated_at: str

    def __new__(
        cls,
        id: str,
        name: str,
        created_at: str,
        updated_at: str,
    ) -> Project: ...

class Tag:
    """A reusable label that can be attached to a span."""

    id: str
    name: str
    created_at: str
    updated_at: str

    def __new__(
        cls,
        id: str,
        name: str,
        created_at: str,
        updated_at: str,
    ) -> Tag: ...

class Span:
    """A mutable record of a period during which the user records time."""

    id: str
    started_at: str
    stopped_at: Optional[str]
    project_id: Optional[str]
    tag_ids: list[str]
    created_at: str
    updated_at: str
    is_active: bool

    def __new__(
        cls,
        id: str,
        started_at: str,
        stopped_at: Optional[str],
        project_id: Optional[str],
        tag_ids: list[str],
        created_at: str,
        updated_at: str,
    ) -> Span: ...
