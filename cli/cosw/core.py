"""Shared plumbing: provider access, transactions, and name/span resolution."""

from __future__ import annotations

import os
import re
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any, TypeVar

import click
from contextswitch_core import (
    Document,
    DomainError,
    LocalFsProvider,
    Project,
    Span,
    StorageError,
    Tag,
)

from cosw.timeparse import parse_datetime

ENV_DATA_FILE = "COSW_DATA_FILE"

T = TypeVar("T")

_INDEX_RE = re.compile(r"-\d+")


def default_data_file() -> Path:
    env = os.environ.get(ENV_DATA_FILE)
    if env:
        return Path(env).expanduser()
    xdg = os.environ.get("XDG_DATA_HOME")
    base = Path(xdg).expanduser() if xdg else Path.home() / ".local" / "share"
    return base / "context-switch" / "data.json"


def open_provider(path: Path) -> LocalFsProvider:
    try:
        return LocalFsProvider(str(path))
    except StorageError as e:
        raise click.ClickException(str(e)) from e


def get_provider(ctx: click.Context) -> LocalFsProvider:
    provider = ctx.obj["provider"]
    assert isinstance(provider, LocalFsProvider)
    return provider


def read_document(ctx: click.Context) -> Document:
    try:
        return get_provider(ctx).read().document
    except StorageError as e:
        raise click.ClickException(str(e)) from e


def transact(
    ctx: click.Context,
    mutate: Callable[[Document], T],
    confirm: Callable[[T], None] | None = None,
) -> T:
    """Read canonical data, apply one focused mutation, and commit conditionally.

    ``confirm`` runs after the mutation but before the commit; raising
    ``click.Abort`` there leaves canonical data untouched.
    """
    provider = get_provider(ctx)
    try:
        snap = provider.read()
    except StorageError as e:
        raise click.ClickException(str(e)) from e
    doc = snap.document
    try:
        result = mutate(doc)
    except DomainError as e:
        raise click.ClickException(str(e)) from e
    if confirm is not None:
        confirm(result)
    try:
        provider.commit(doc, snap.version)
    except (DomainError, StorageError) as e:
        raise click.ClickException(str(e)) from e
    return result


def parse_dt(value: str) -> datetime:
    try:
        return parse_datetime(value)
    except ValueError as e:
        raise click.BadParameter(str(e)) from e


_Named = TypeVar("_Named", Project, Tag)


def _find_by_name(items: list[_Named], name: str) -> _Named | None:
    lowered = name.casefold()
    for item in items:
        if item.name.casefold() == lowered:
            return item
    return None


def require_project(doc: Document, name: str) -> Project:
    project = _find_by_name(doc.projects(), name)
    if project is None:
        raise click.ClickException(f"unknown project: {name}")
    return project


def require_tag(doc: Document, name: str) -> Tag:
    tag = _find_by_name(doc.tags(), name)
    if tag is None:
        raise click.ClickException(f"unknown tag: {name}")
    return tag


def resolve_project_id(
    doc: Document, name: str | None, at: datetime, created: list[str]
) -> str | None:
    """Resolve a project name to an ID, creating it on first use."""
    if name is None:
        return None
    existing = _find_by_name(doc.projects(), name)
    if existing is not None:
        if existing.archived:
            raise click.ClickException(f"project {existing.name!r} is archived; unarchive it first")
        return existing.id
    project_id = doc.add_project(name, at)
    created.append(f"created project {name}")
    return project_id


def resolve_tag_ids(doc: Document, names: list[str], at: datetime, created: list[str]) -> list[str]:
    """Resolve tag names to IDs, creating them on first use."""
    ids: list[str] = []
    for name in names:
        existing = _find_by_name(doc.tags(), name)
        if existing is not None:
            if existing.archived:
                raise click.ClickException(f"tag {existing.name!r} is archived; unarchive it first")
            ids.append(existing.id)
            continue
        ids.append(doc.add_tag(name, at))
        created.append(f"created tag {name}")
    return ids


def parse_classification(args: tuple[str, ...]) -> tuple[str | None, list[str]]:
    """Split ``PROJECT [+TAG ...]`` positional arguments."""
    project = None
    tags = []
    for arg in args:
        if arg.startswith("+"):
            if len(arg) == 1:
                raise click.UsageError("'+' needs a tag name")
            tags.append(arg[1:])
        elif project is None:
            project = arg
        else:
            raise click.UsageError(f"unexpected argument {arg!r}; extra tags use '+name'")
    return project, tags


def sorted_spans(doc: Document) -> list[Span]:
    return sorted(doc.spans(), key=lambda s: s.started_at)


def resolve_span(doc: Document, ref: str | None) -> Span:
    """Resolve a span reference: ``-N`` recency index or unambiguous ID prefix.

    ``None`` means the active span, or the most recent span when idle.
    """
    spans = sorted_spans(doc)
    if ref is None:
        active = doc.active_span()
        if active is not None:
            return active
        if spans:
            return spans[-1]
        raise click.ClickException("no spans recorded")
    if _INDEX_RE.fullmatch(ref):
        idx = int(ref)
        if idx == 0 or idx < -len(spans):
            raise click.ClickException(f"no span at index {ref}")
        return spans[idx]
    matches = [s for s in spans if s.id.startswith(ref)]
    if not matches:
        raise click.ClickException(f"no span matching {ref!r}")
    if len(matches) > 1:
        raise click.ClickException(f"span id {ref!r} is ambiguous")
    return matches[0]


def project_name(doc: Document, span: Span) -> str:
    if span.project_id is None:
        return "(unassigned)"
    project = doc.project(span.project_id)
    return project.name if project is not None else "(unassigned)"


def tag_names(doc: Document, span: Span) -> list[str]:
    names = []
    for tag_id in span.tag_ids:
        tag = doc.tag(tag_id)
        names.append(tag.name if tag is not None else tag_id[:8])
    return sorted(names, key=str.casefold)


def describe(doc: Document, span: Span) -> str:
    name = project_name(doc, span)
    tags = " ".join(f"+{t}" for t in tag_names(doc, span))
    return f"{name} ({tags})" if tags else name


def span_to_json(doc: Document, span: Span, now: datetime) -> dict[str, Any]:
    stopped = span.stopped_at
    end = stopped if stopped is not None else now
    return {
        "id": span.id,
        "started_at": span.started_at.isoformat(),
        "stopped_at": stopped.isoformat() if stopped is not None else None,
        "active": span.is_active,
        "project": project_name(doc, span),
        "tags": tag_names(doc, span),
        "seconds": int((end - span.started_at).total_seconds()),
    }


def echo_created(created: list[str]) -> None:
    for message in created:
        click.echo(message)
