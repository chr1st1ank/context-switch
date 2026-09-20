"""Span filtering and aggregation shared by ``log`` and ``report``.

Pure query logic with no CLI-framework imports. Errors surface as the
exceptions below; the command layer in ``history.py`` translates them into
click errors with the same user-facing text.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime, time, timedelta

from contextswitch_core import Logbook, Span

from cosw.core import project_name
from cosw.timeparse import parse_datetime, parse_range_end

RANGE_SHORTCUTS = ("day", "week", "month", "year", "all")


class RangeConflictError(ValueError):
    """A --range shortcut was combined with --from/--to."""


class UnknownNameError(Exception):
    """A filter named a project or tag that does not exist."""


@dataclass
class Filters:
    """User-facing filter options for history commands."""

    range: str | None
    from_str: str | None
    to_str: str | None
    projects: tuple[str, ...]
    tags: tuple[str, ...]
    ignore_projects: tuple[str, ...]
    ignore_tags: tuple[str, ...]
    include_current: bool


def bounds(f: Filters, now: datetime) -> tuple[datetime | None, datetime | None]:
    """Resolve the (from, to) range; ``None`` means unbounded on that side."""
    if f.range is not None:
        if f.from_str is not None or f.to_str is not None:
            raise RangeConflictError("range shortcuts cannot be combined with --from/--to")
        if f.range == "all":
            return None, None
        today = now.astimezone().date()
        if f.range == "day":
            start = today
        elif f.range == "week":
            start = today - timedelta(days=today.weekday())
        elif f.range == "month":
            start = today.replace(day=1)
        else:
            start = today.replace(month=1, day=1)
        lo = datetime.combine(start, time.min).astimezone(UTC)
        return lo, None
    lo = parse_datetime(f.from_str) if f.from_str is not None else None
    hi = parse_range_end(f.to_str) if f.to_str is not None else None
    return lo, hi


def _name_ids(logbook: Logbook, names: tuple[str, ...], kind: str) -> set[str]:
    """Resolve filter names to IDs; unknown names are errors, never created."""
    items = logbook.projects() if kind == "project" else logbook.tags()
    ids = set()
    for name in names:
        lowered = name.casefold()
        match = next((item for item in items if item.name.casefold() == lowered), None)
        if match is None:
            raise UnknownNameError(f"unknown {kind}: {name}")
        ids.add(match.id)
    return ids


def matching_spans(
    logbook: Logbook, f: Filters, now: datetime
) -> list[tuple[Span, datetime, datetime]]:
    """Spans matching ``f``, each with its range-clipped (start, stop) bounds.

    Results are sorted by clipped start, oldest first. The active span is
    clipped at ``now``.
    """
    lo, hi = bounds(f, now)
    project_ids = _name_ids(logbook, f.projects, "project") if f.projects else None
    tag_ids = _name_ids(logbook, f.tags, "tag") if f.tags else None
    ignore_projects = _name_ids(logbook, f.ignore_projects, "project")
    ignore_tags = _name_ids(logbook, f.ignore_tags, "tag")

    rows: list[tuple[Span, datetime, datetime]] = []
    for span in logbook.spans():
        if span.is_active and not f.include_current:
            continue
        if project_ids is not None and span.project_id not in project_ids:
            continue
        span_tag_ids = set(span.tag_ids)
        if tag_ids is not None and not tag_ids <= span_tag_ids:
            continue
        if ignore_projects and span.project_id in ignore_projects:
            continue
        if ignore_tags and ignore_tags & span_tag_ids:
            continue
        stop = span.stopped_at if span.stopped_at is not None else now
        clipped_lo = max(span.started_at, lo) if lo is not None else span.started_at
        clipped_hi = min(stop, hi) if hi is not None else stop
        if clipped_lo >= clipped_hi:
            continue
        rows.append((span, clipped_lo, clipped_hi))
    rows.sort(key=lambda row: row[1])
    return rows


def totals_by(
    rows: list[tuple[Span, datetime, datetime]], key: Callable[[Span], list[str]]
) -> dict[str, float]:
    """Sum clipped seconds per key; a span may contribute to several keys."""
    totals: dict[str, float] = {}
    for span, lo, hi in rows:
        seconds = (hi - lo).total_seconds()
        for name in key(span):
            totals[name] = totals.get(name, 0.0) + seconds
    return totals


def totals_by_day(
    logbook: Logbook, rows: list[tuple[Span, datetime, datetime]]
) -> dict[str, dict[str, float]]:
    """Per-local-date totals broken down by project.

    A span crossing local midnight contributes to every date it covers.
    """
    days: dict[str, dict[str, float]] = {}
    for span, lo, hi in rows:
        name = project_name(logbook, span)
        cursor = lo
        while cursor < hi:
            day = cursor.astimezone().date()
            next_midnight = datetime.combine(day + timedelta(days=1), time.min).astimezone(UTC)
            end = min(hi, next_midnight)
            bucket = days.setdefault(day.isoformat(), {})
            bucket[name] = bucket.get(name, 0.0) + (end - cursor).total_seconds()
            cursor = end
    return days
