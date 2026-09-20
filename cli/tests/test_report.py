"""Tests for log and report."""

import json
from pathlib import Path

import pytest


def _seed(invoke) -> None:
    invoke(
        "add",
        "apollo11",
        "+module",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:30:00Z",
    )
    invoke(
        "add",
        "apollo11",
        "+review",
        "--from",
        "2026-09-09T10:00:00Z",
        "--to",
        "2026-09-09T11:00:00Z",
    )
    invoke("add", "personal", "--from", "2026-09-09T12:00:00Z", "--to", "2026-09-09T12:45:00Z")


def test_log_lists_spans(invoke) -> None:
    _seed(invoke)
    result = invoke("log")
    assert result.exit_code == 0
    lines = result.output.strip().splitlines()
    assert len(lines) == 3
    assert "personal" in lines[0]
    assert "apollo11" in lines[1]
    assert "1h 30m" in lines[2]


def test_log_reverse(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--reverse")
    lines = result.output.strip().splitlines()
    assert "1h 30m" in lines[0]
    assert "personal" in lines[2]


def test_log_json(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--json")
    payload = json.loads(result.output)
    assert len(payload) == 3
    assert payload[0]["project"] == "personal"
    assert payload[0]["seconds"] == 2700
    assert payload[0]["tags"] == []


def test_log_active_span_shown(invoke) -> None:
    _seed(invoke)
    invoke("start", "apollo11")
    result = invoke("log")
    assert "now" in result.output
    result = invoke("log", "--no-current")
    assert "now" not in result.output


def test_log_project_filter(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "-p", "apollo11")
    lines = result.output.strip().splitlines()
    assert len(lines) == 2
    assert all("apollo11" in line for line in lines)


def test_log_tag_filter(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "-T", "module")
    assert "apollo11" in result.output
    assert "personal" not in result.output


def test_log_ignore_filters(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--ignore-project", "apollo11")
    assert "apollo11" not in result.output
    assert "personal" in result.output
    result = invoke("log", "--ignore-tag", "module")
    lines = result.output.strip().splitlines()
    assert len(lines) == 2


def test_log_unknown_project_filter_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "-p", "nosuch")
    assert result.exit_code == 1
    assert "unknown project" in result.output


def test_log_range_and_from_conflict(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--range", "day", "--from", "2026-09-08")
    assert result.exit_code == 2


def test_log_from_to_range(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--from", "2026-09-09", "--to", "2026-09-09")
    lines = result.output.strip().splitlines()
    assert len(lines) == 2
    assert "personal" in lines[0]


def test_log_from_clips_duration(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--from", "2026-09-08T09:00:00Z")
    assert "30m" in result.output


def test_log_bad_from(invoke) -> None:
    _seed(invoke)
    result = invoke("log", "--from", "bogus")
    assert result.exit_code == 2


def test_report_by_project(invoke) -> None:
    _seed(invoke)
    result = invoke("report")
    assert result.exit_code == 0
    assert "apollo11" in result.output
    assert "2h 30m" in result.output
    assert "personal" in result.output
    assert "45m" in result.output
    assert "total" in result.output
    assert "3h 15m" in result.output


def test_report_by_tag(invoke) -> None:
    _seed(invoke)
    result = invoke("report", "--by", "tag")
    assert "module" in result.output
    assert "review" in result.output
    assert "(untagged)" in result.output


def test_report_by_day(invoke) -> None:
    _seed(invoke)
    result = invoke("report", "--by", "day")
    assert "2026-09-08" in result.output
    assert "2026-09-09" in result.output


@pytest.mark.usefixtures("berlin_tz")
def test_report_by_day_splits_overnight_span(invoke) -> None:
    """A span crossing local midnight contributes to each date it covers."""
    # 23:00–01:00 local in Berlin (CEST, UTC+2).
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-10T21:00:00Z",
        "--to",
        "2026-09-10T23:00:00Z",
    )
    result = invoke("report", "--by", "day", "--json")
    days = {d["date"]: d["total_seconds"] for d in json.loads(result.output)["days"]}
    assert days == {"2026-09-10": 3600, "2026-09-11": 3600}


@pytest.mark.usefixtures("berlin_tz")
def test_report_by_day_midnight_boundary(invoke) -> None:
    """A span ending exactly at local midnight leaves no next-day bucket."""
    # 23:00–00:00 local in Berlin (CEST, UTC+2).
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-11T21:00:00Z",
        "--to",
        "2026-09-11T22:00:00Z",
    )
    result = invoke("report", "--by", "day", "--json")
    days = {d["date"]: d["total_seconds"] for d in json.loads(result.output)["days"]}
    assert days == {"2026-09-11": 3600}


def test_report_json(invoke) -> None:
    _seed(invoke)
    result = invoke("report", "--json")
    payload = json.loads(result.output)
    assert payload["by"] == "project"
    assert payload["totals"]["apollo11"] == 9000
    assert payload["total_seconds"] == 11700


def test_report_day_json(invoke) -> None:
    _seed(invoke)
    result = invoke("report", "--by", "day", "--json")
    payload = json.loads(result.output)
    assert payload["by"] == "day"
    assert payload["days"][0]["date"] == "2026-09-08"


def test_report_empty(invoke) -> None:
    result = invoke("report")
    assert result.exit_code == 0
    assert "No time recorded" in result.output


def test_report_empty_day(invoke) -> None:
    result = invoke("report", "--by", "day")
    assert "No time recorded" in result.output


def test_report_no_current(invoke) -> None:
    _seed(invoke)
    invoke("start", "apollo11")
    result = invoke("report", "--no-current", "--json")
    payload = json.loads(result.output)
    assert payload["totals"]["apollo11"] == 9000


def test_report_range_shortcut(invoke) -> None:
    _seed(invoke)
    result = invoke("report", "--range", "all", "--json")
    assert json.loads(result.output)["total_seconds"] == 11700
    result = invoke("report", "--range", "day", "--json")
    assert json.loads(result.output)["total_seconds"] == 0


def test_report_range_shortcuts(invoke) -> None:
    _seed(invoke)
    for shortcut in ("week", "month", "year"):
        result = invoke("report", "--range", shortcut, "--json")
        assert result.exit_code == 0


def test_report_tag_filter_and_semantics(invoke) -> None:
    _seed(invoke)
    invoke(
        "add",
        "apollo11",
        "+module",
        "+review",
        "--from",
        "2026-09-09T14:00:00Z",
        "--to",
        "2026-09-09T15:00:00Z",
    )
    result = invoke("report", "-T", "module", "-T", "review", "--json")
    payload = json.loads(result.output)
    assert payload["total_seconds"] == 3600


def test_reporting_module_is_framework_free() -> None:
    """The query module holds no CLI-framework bindings of its own."""
    import cosw.reporting

    assert "click" not in cosw.reporting.__dict__


def test_matching_spans_raises_domain_errors(invoke, data_file: Path) -> None:
    """Query failures surface as domain exceptions, translated at the CLI layer."""
    from dataclasses import replace

    from contextswitch_core import LocalFsProvider
    from cosw.reporting import (
        Filters,
        RangeConflictError,
        UnknownNameError,
        matching_spans,
    )
    from cosw.timeparse import utcnow

    invoke("add", "apollo11", "--from", "2026-09-08T08:00:00Z", "--to", "2026-09-08T09:00:00Z")
    logbook = LocalFsProvider(str(data_file)).read().logbook
    now = utcnow()
    base = Filters(
        range=None,
        from_str=None,
        to_str=None,
        projects=(),
        tags=(),
        ignore_projects=(),
        ignore_tags=(),
        include_current=True,
    )
    with pytest.raises(UnknownNameError):
        matching_spans(logbook, replace(base, projects=("nosuch",)), now)
    with pytest.raises(RangeConflictError):
        matching_spans(logbook, replace(base, range="day", from_str="2026-09-08"), now)
    with pytest.raises(ValueError):
        matching_spans(logbook, replace(base, from_str="bogus"), now)
