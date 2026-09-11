"""Datetime parsing and display helpers for cosw.

Inputs are ISO 8601 or a bare ``HH:MM[:SS]`` meaning today; naive values are
interpreted in the local timezone and converted to UTC, which is what the
core document stores.
"""

from __future__ import annotations

import re
from datetime import UTC, date, datetime, time, tzinfo

_TIME_RE = re.compile(r"^(\d{1,2}):(\d{2})(?::(\d{2}))?$")
_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def local_tz() -> tzinfo | None:
    return datetime.now().astimezone().tzinfo


def utcnow() -> datetime:
    return datetime.now(UTC)


def parse_datetime(value: str) -> datetime:
    """Parse ISO 8601 or bare HH:MM[:SS] (today, local time) into UTC."""
    text = value.strip()
    match = _TIME_RE.match(text)
    if match:
        try:
            t = time(int(match.group(1)), int(match.group(2)), int(match.group(3) or 0))
        except ValueError as e:
            raise ValueError(f"invalid time {value!r}") from e
        return datetime.combine(date.today(), t, tzinfo=local_tz()).astimezone(UTC)
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError as e:
        raise ValueError(f"invalid datetime {value!r}; use ISO 8601 or HH:MM") from e
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=local_tz())
    return parsed.astimezone(UTC)


def parse_range_end(value: str) -> datetime:
    """Like :func:`parse_datetime`, but a bare date means the end of that day."""
    text = value.strip()
    if _DATE_RE.match(text):
        end = date.fromisoformat(text)
        return datetime.combine(end, time.max, tzinfo=local_tz()).astimezone(UTC)
    return parse_datetime(value)


def fmt_local(dt: datetime) -> str:
    return dt.astimezone().strftime("%Y-%m-%d %H:%M")


def fmt_time(dt: datetime) -> str:
    return dt.astimezone().strftime("%H:%M")


def fmt_duration(seconds: float) -> str:
    total = int(seconds)
    hours, rem = divmod(total, 3600)
    minutes, secs = divmod(rem, 60)
    if hours:
        return f"{hours}h {minutes:02d}m"
    if minutes:
        return f"{minutes}m {secs:02d}s"
    return f"{secs}s"
