"""Tests for datetime parsing, formatting, and classification helpers."""

from datetime import UTC, datetime

import pytest
from cosw.core import parse_classification
from cosw.timeparse import fmt_duration, parse_datetime, parse_range_end, utcnow


def test_parse_iso_utc() -> None:
    assert parse_datetime("2026-09-10T08:00:00Z") == datetime(2026, 9, 10, 8, 0, tzinfo=UTC)


def test_parse_iso_offset() -> None:
    assert parse_datetime("2026-09-10T10:00:00+02:00") == datetime(2026, 9, 10, 8, 0, tzinfo=UTC)


def test_parse_naive_is_local() -> None:
    parsed = parse_datetime("2026-09-10T08:00:00")
    assert parsed.tzinfo == UTC


def test_parse_date_only() -> None:
    parsed = parse_datetime("2026-09-10")
    assert parsed.astimezone().hour == 0
    assert parsed.tzinfo == UTC


@pytest.mark.usefixtures("berlin_tz")
def test_parse_naive_uses_dst_rules() -> None:
    """Naive values resolve against the offset in effect on their own date."""
    assert parse_datetime("2026-01-15T09:00") == datetime(2026, 1, 15, 8, 0, tzinfo=UTC)
    assert parse_datetime("2026-07-15T09:00") == datetime(2026, 7, 15, 7, 0, tzinfo=UTC)


@pytest.mark.usefixtures("berlin_tz")
def test_range_end_date_only_uses_dst_rules() -> None:
    parsed = parse_range_end("2026-01-15")
    assert parsed == datetime(2026, 1, 15, 22, 59, 59, 999999, tzinfo=UTC)


def test_parse_hhmm_today() -> None:
    parsed = parse_datetime("08:15")
    local = parsed.astimezone()
    assert (local.hour, local.minute) == (8, 15)
    assert local.date() == datetime.now().astimezone().date()


def test_parse_hhmmss() -> None:
    parsed = parse_datetime("08:15:30")
    assert parsed.astimezone().second == 30


def test_parse_invalid() -> None:
    with pytest.raises(ValueError, match="invalid datetime"):
        parse_datetime("not-a-time")


def test_parse_invalid_time() -> None:
    with pytest.raises(ValueError, match="invalid time"):
        parse_datetime("25:99")


def test_range_end_date_only() -> None:
    parsed = parse_range_end("2026-09-10")
    local = parsed.astimezone()
    assert (local.hour, local.minute) == (23, 59)


def test_range_end_datetime() -> None:
    parsed = parse_range_end("2026-09-10T12:00:00Z")
    assert parsed == datetime(2026, 9, 10, 12, 0, tzinfo=UTC)


def test_fmt_duration() -> None:
    assert fmt_duration(5400) == "1h 30m"
    assert fmt_duration(90) == "1m 30s"
    assert fmt_duration(45) == "45s"


def test_utcnow() -> None:
    assert utcnow().tzinfo == UTC


def test_parse_classification() -> None:
    assert parse_classification(("apollo11", "+a", "+b")) == ("apollo11", ["a", "b"])
    assert parse_classification(("+a",)) == (None, ["a"])
    assert parse_classification(()) == (None, [])


def test_parse_classification_errors() -> None:
    import click

    with pytest.raises(click.UsageError):
        parse_classification(("a", "b"))
    with pytest.raises(click.UsageError):
        parse_classification(("+",))
