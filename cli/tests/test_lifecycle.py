"""Tests for timer lifecycle: start, stop, switch, resume, cancel, status."""

import json
from typing import Any


def test_start_creates_project_and_tags(invoke, data_file) -> None:
    result = invoke("start", "apollo11", "+module", "+brakes")
    assert result.exit_code == 0
    assert "created project apollo11" in result.output
    assert "created tag module" in result.output
    assert "created tag brakes" in result.output
    assert "Timer started on apollo11 (+brakes +module)" in result.output


def test_start_unassigned(invoke) -> None:
    result = invoke("start")
    assert result.exit_code == 0
    assert "unassigned" in result.output


def test_start_tags_only(invoke) -> None:
    result = invoke("start", "+focus")
    assert result.exit_code == 0
    assert "created tag focus" in result.output
    assert "(unassigned) (+focus)" in result.output


def test_start_while_active_errors(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("start", "other")
    assert result.exit_code == 1
    assert "already active" in result.output
    assert "switch" in result.output


def test_start_second_project_arg_errors(invoke) -> None:
    result = invoke("start", "a", "b")
    assert result.exit_code == 2
    assert "unexpected argument" in result.output


def test_start_bare_plus_errors(invoke) -> None:
    result = invoke("start", "+")
    assert result.exit_code == 2


def test_start_at_iso(invoke) -> None:
    result = invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    assert result.exit_code == 0


def test_start_at_hhmm(invoke) -> None:
    result = invoke("start", "apollo11", "--at", "08:15")
    assert result.exit_code == 0


def test_start_at_invalid(invoke) -> None:
    result = invoke("start", "apollo11", "--at", "not-a-time")
    assert result.exit_code == 2


def test_stop(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    result = invoke("stop", "--at", "2026-09-10T09:30:00Z")
    assert result.exit_code == 0
    assert "Stopped apollo11" in result.output
    assert "1h 30m" in result.output


def test_stop_without_active(invoke) -> None:
    result = invoke("stop")
    assert result.exit_code == 1
    assert "no active timer" in result.output


def test_stop_before_start_errors(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T10:00:00Z")
    result = invoke("stop", "--at", "2026-09-10T09:00:00Z")
    assert result.exit_code == 1


def test_switch(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    result = invoke("switch", "personal", "+errand", "--at", "2026-09-10T09:00:00Z")
    assert result.exit_code == 0
    assert "created project personal" in result.output
    assert "Switched from apollo11 to personal (+errand)" in result.output


def test_switch_without_active_starts(invoke) -> None:
    result = invoke("switch", "apollo11")
    assert result.exit_code == 0
    assert "Timer started on apollo11" in result.output


def test_resume_copies_classification(invoke) -> None:
    invoke("start", "apollo11", "+module", "--at", "2026-09-10T08:00:00Z")
    invoke("stop", "--at", "2026-09-10T09:00:00Z")
    invoke("start", "personal", "--at", "2026-09-10T10:00:00Z")
    invoke("stop", "--at", "2026-09-10T11:00:00Z")
    result = invoke("resume", "--at", "2026-09-10T11:30:00Z")
    assert result.exit_code == 0
    assert "Resumed personal" in result.output
    invoke("stop", "--at", "2026-09-10T12:00:00Z")
    result = invoke("resume", "-3", "--at", "2026-09-10T13:00:00Z")
    assert result.exit_code == 0
    assert "Resumed apollo11 (+module)" in result.output


def test_resume_while_active_errors(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("resume")
    assert result.exit_code == 1
    assert "already active" in result.output


def test_resume_empty_errors(invoke) -> None:
    result = invoke("resume")
    assert result.exit_code == 1
    assert "nothing to resume" in result.output


def test_resume_unknown_ref_errors(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    invoke("stop", "--at", "2026-09-10T09:00:00Z")
    result = invoke("resume", "zzzz")
    assert result.exit_code == 1
    assert "no span matching" in result.output


def test_cancel_force(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("cancel", "--force")
    assert result.exit_code == 0
    assert "Discarded timer on apollo11" in result.output
    status = invoke("status")
    assert "No active timer" in status.output


def test_cancel_confirmed(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("cancel", input="y\n")
    assert result.exit_code == 0
    assert "Discarded" in result.output


def test_cancel_aborted_keeps_timer(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("cancel", input="n\n")
    assert result.exit_code != 0
    status = invoke("status")
    assert "apollo11" in status.output


def test_cancel_without_active(invoke) -> None:
    result = invoke("cancel", "--force")
    assert result.exit_code == 1
    assert "no active timer" in result.output


def test_status_active(invoke) -> None:
    invoke("start", "apollo11", "+module", "--at", "2026-09-10T08:00:00Z")
    result = invoke("status")
    assert result.exit_code == 0
    assert "apollo11 (+module)" in result.output
    assert "elapsed" in result.output


def test_status_json_active(invoke) -> None:
    invoke("start", "apollo11", "+module", "--at", "2026-09-10T08:00:00Z")
    result = invoke("status", "--json")
    assert result.exit_code == 0
    payload = json.loads(result.output)
    assert payload["active"] is True
    assert payload["project"] == "apollo11"
    assert payload["tags"] == ["module"]
    assert payload["stopped_at"] is None


def _spans(invoke) -> list[dict[str, Any]]:
    spans: list[dict[str, Any]] = json.loads(invoke("log", "--json", "--range", "all").output)
    return spans


def test_start_with_description(invoke) -> None:
    invoke("start", "apollo11", "-d", "  wire up\nthe brakes ")
    (span,) = _spans(invoke)
    assert span["description"] == "wire up\nthe brakes"


def test_start_without_description_is_null(invoke) -> None:
    invoke("start", "apollo11")
    assert _spans(invoke)[0]["description"] is None


def test_description_too_long_is_rejected_and_nothing_written(invoke) -> None:
    result = invoke("start", "apollo11", "-d", "x" * 1001)
    assert result.exit_code == 1
    assert "too long" in result.output
    assert _spans(invoke) == []


def test_stop_description_overwrites(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z", "-d", "x")
    result = invoke("stop", "--at", "2026-09-10T09:00:00Z", "-d", "y")
    assert result.exit_code == 0
    assert _spans(invoke)[0]["description"] == "y"


def test_stop_without_description_keeps_existing(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z", "-d", "x")
    invoke("stop", "--at", "2026-09-10T09:00:00Z")
    assert _spans(invoke)[0]["description"] == "x"


def test_switch_starts_empty_unless_described(invoke) -> None:
    invoke("start", "a", "--at", "2026-09-10T08:00:00Z", "-d", "first")
    invoke("switch", "b", "--at", "2026-09-10T09:00:00Z")
    invoke("switch", "c", "--at", "2026-09-10T10:00:00Z", "-d", "third")
    by_project = {s["project"]: s["description"] for s in _spans(invoke)}
    assert by_project == {"a": "first", "b": None, "c": "third"}


def test_resume_copies_description(invoke) -> None:
    invoke("start", "a", "--at", "2026-09-10T08:00:00Z", "-d", "first")
    invoke("stop", "--at", "2026-09-10T09:00:00Z")
    invoke("resume", "--at", "2026-09-10T10:00:00Z")
    descriptions = [s["description"] for s in _spans(invoke)]
    assert descriptions == ["first", "first"]


def test_resume_with_description_overrides_copy(invoke) -> None:
    invoke("start", "a", "--at", "2026-09-10T08:00:00Z", "-d", "first")
    invoke("stop", "--at", "2026-09-10T09:00:00Z")
    invoke("resume", "--at", "2026-09-10T10:00:00Z", "-d", "second")
    descriptions = [s["description"] for s in _spans(invoke)]
    assert descriptions == ["second", "first"]


def test_status_json_includes_description(invoke) -> None:
    invoke("start", "a", "-d", "hello")
    assert json.loads(invoke("status", "--json").output)["description"] == "hello"
