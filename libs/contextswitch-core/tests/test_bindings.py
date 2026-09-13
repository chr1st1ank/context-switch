"""Smoke tests for the contextswitch_core Python bindings."""

from datetime import datetime, timedelta, timezone

import contextswitch_core as core
import pytest

NOW = datetime(2026, 9, 10, 12, 0, 0, tzinfo=timezone.utc)


def test_provider_lifecycle(tmp_path) -> None:
    """read → mutate → commit → conflict, through the Python bindings."""
    provider = core.LocalFsProvider(str(tmp_path / "data.json"))

    snapshot = provider.read()
    assert snapshot.version == "0"
    assert snapshot.logbook.active_span_id is None

    logbook = snapshot.logbook
    project = logbook.add_project("work", NOW)
    tag = logbook.add_tag("focus", NOW)
    span_id = logbook.start_timer(NOW, project_id=project, tag_ids=[tag])
    assert logbook.active_span_id == span_id

    version = provider.commit(logbook, snapshot.version)
    assert version == "1"

    reread = provider.read()
    assert reread.version == "1"
    assert reread.logbook.active_span_id == span_id
    active = reread.logbook.active_span()
    assert active is not None
    assert active.is_active
    assert active.project_id == project
    assert active.tag_ids == [tag]

    # A commit against the stale version is rejected.
    stale = reread.logbook
    stale.add_tag("late", NOW)
    with pytest.raises(core.StorageError):
        provider.commit(stale, snapshot.version)


def test_switch_and_stop(tmp_path) -> None:
    provider = core.LocalFsProvider(str(tmp_path / "data.json"))
    snapshot = provider.read()
    logbook = snapshot.logbook
    logbook.start_timer(NOW)
    later = NOW + timedelta(hours=1)
    second = logbook.switch(later)
    logbook.stop_timer(later + timedelta(hours=2))
    provider.commit(logbook, snapshot.version)

    logbook = provider.read().logbook
    assert logbook.active_span_id is None
    spans = {s.id: s for s in logbook.spans()}
    assert spans[second].started_at == later
    assert len(spans) == 2


def test_domain_errors_raise_domain_error() -> None:
    logbook = core.Logbook()
    with pytest.raises(core.DomainError):
        logbook.stop_timer(NOW)
    logbook.start_timer(NOW)
    with pytest.raises(core.DomainError):
        logbook.start_timer(NOW)
    logbook.add_project("a", NOW)
    with pytest.raises(core.DomainError):
        logbook.add_project("A", NOW)


def test_datetime_round_trip() -> None:
    logbook = core.Logbook()
    span_id = logbook.start_timer(NOW)
    span = logbook.span(span_id)
    assert span is not None
    assert span.started_at == NOW
    assert span.stopped_at is None


def test_json_round_trip() -> None:
    logbook = core.Logbook()
    logbook.add_project("work", NOW)
    logbook.start_timer(NOW)
    parsed = core.Logbook.from_json(logbook.to_json())
    assert parsed.spans()[0].id == logbook.spans()[0].id
