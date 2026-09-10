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
    assert snapshot.document.active_span_id is None

    document = snapshot.document
    project = document.add_project("work", NOW)
    tag = document.add_tag("focus", NOW)
    span_id = document.start_timer(NOW, project_id=project, tag_ids=[tag])
    assert document.active_span_id == span_id

    version = provider.commit(document, snapshot.version)
    assert version == "1"

    reread = provider.read()
    assert reread.version == "1"
    assert reread.document.active_span_id == span_id
    active = reread.document.active_span()
    assert active is not None
    assert active.is_active
    assert active.project_id == project
    assert active.tag_ids == [tag]

    # A commit against the stale version is rejected.
    stale = reread.document
    stale.add_tag("late", NOW)
    with pytest.raises(core.StorageError):
        provider.commit(stale, snapshot.version)


def test_switch_and_stop(tmp_path) -> None:
    provider = core.LocalFsProvider(str(tmp_path / "data.json"))
    snapshot = provider.read()
    document = snapshot.document
    document.start_timer(NOW)
    later = NOW + timedelta(hours=1)
    second = document.switch(later)
    document.stop_timer(later + timedelta(hours=2))
    provider.commit(document, snapshot.version)

    document = provider.read().document
    assert document.active_span_id is None
    spans = {s.id: s for s in document.spans()}
    assert spans[second].started_at == later
    assert len(spans) == 2


def test_domain_errors_raise_domain_error() -> None:
    document = core.Document()
    with pytest.raises(core.DomainError):
        document.stop_timer(NOW)
    document.start_timer(NOW)
    with pytest.raises(core.DomainError):
        document.start_timer(NOW)
    document.add_project("a", NOW)
    with pytest.raises(core.DomainError):
        document.add_project("A", NOW)


def test_datetime_round_trip() -> None:
    document = core.Document()
    span_id = document.start_timer(NOW)
    span = document.span(span_id)
    assert span is not None
    assert span.started_at == NOW
    assert span.stopped_at is None


def test_json_round_trip() -> None:
    document = core.Document()
    document.add_project("work", NOW)
    document.start_timer(NOW)
    parsed = core.Document.from_json(document.to_json())
    assert parsed.spans()[0].id == document.spans()[0].id
