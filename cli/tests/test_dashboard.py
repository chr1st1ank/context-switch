"""Tests for the local reporting dashboard."""

import http.client
import json
import threading
import urllib.parse
from collections.abc import Iterator
from datetime import UTC, datetime
from pathlib import Path

import cosw.dashboard as dashboard_module
import pytest
from contextswitch_core import LocalFsProvider, Logbook, StorageError, StorageSnapshot
from cosw.dashboard import _DashboardServer, spans_payload
from cosw.timeparse import utcnow


def _get(url: str) -> tuple[int, dict[str, str], bytes]:
    parsed = urllib.parse.urlparse(url)
    conn = http.client.HTTPConnection(parsed.hostname or "localhost", parsed.port, timeout=5)
    try:
        conn.request("GET", parsed.path or "/")
        resp = conn.getresponse()
        return resp.status, dict(resp.getheaders()), resp.read()
    finally:
        conn.close()


@pytest.fixture
def dashboard_url(invoke, data_file: Path) -> Iterator[str]:
    """A running dashboard server against the isolated data file."""
    invoke(
        "add",
        "apollo11",
        "+module",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:30:00Z",
    )
    server = _DashboardServer(("127.0.0.1", 0), LocalFsProvider(str(data_file)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}"
    server.shutdown()
    server.server_close()
    thread.join()


def test_spans_payload() -> None:
    logbook = Logbook()
    now = utcnow()
    project_id = logbook.add_project("apollo11", now, client="NASA")
    tag_id = logbook.add_tag("module", now)
    span_id = logbook.add_span(
        datetime(2026, 9, 8, 8, 0, tzinfo=UTC),
        datetime(2026, 9, 8, 9, 30, tzinfo=UTC),
        project_id,
        [tag_id],
        now,
    )
    logbook.set_span_description(span_id, "LEM checkout", now)
    logbook.start_timer(now, project_id, [tag_id])

    payload = spans_payload(logbook, now)

    assert payload["generatedAt"] == now.isoformat()
    done, active = payload["spans"]
    assert done["project"] == "apollo11"
    assert done["client"] == "NASA"
    assert done["tags"] == ["module"]
    assert done["description"] == "LEM checkout"
    assert done["seconds"] == 5400
    assert done["active"] is False
    assert active["active"] is True
    assert active["stopped_at"] is None
    assert active["seconds"] == 0


def test_dashboard_serves_html(dashboard_url: str) -> None:
    status, headers, body = _get(dashboard_url + "/")
    assert status == 200
    assert "text/html" in headers["Content-Type"]
    assert b"dashboard" in body


def test_dashboard_serves_chart_js(dashboard_url: str) -> None:
    status, headers, body = _get(dashboard_url + "/chart.umd.min.js")
    assert status == 200
    assert "javascript" in headers["Content-Type"]
    assert b"Chart.js" in body


def test_spans_json_endpoint(dashboard_url: str) -> None:
    status, _, body = _get(dashboard_url + "/spans.json")
    assert status == 200
    payload = json.loads(body)
    assert "generatedAt" in payload
    (span,) = payload["spans"]
    assert span["project"] == "apollo11"
    assert span["tags"] == ["module"]
    assert span["seconds"] == 5400
    assert span["active"] is False


def test_spans_json_re_reads_provider(invoke, dashboard_url: str) -> None:
    """Each request reads the provider again so reload picks up new data."""
    invoke(
        "add",
        "personal",
        "--from",
        "2026-09-09T12:00:00Z",
        "--to",
        "2026-09-09T12:45:00Z",
    )
    status, _, body = _get(dashboard_url + "/spans.json")
    assert status == 200
    payload = json.loads(body)
    assert len(payload["spans"]) == 2


def test_dashboard_unknown_path(dashboard_url: str) -> None:
    status, _, _ = _get(dashboard_url + "/nope")
    assert status == 404


def test_spans_json_provider_error() -> None:
    class FailingProvider:
        def read(self) -> StorageSnapshot:
            raise StorageError("unreachable")

    server = _DashboardServer(("127.0.0.1", 0), FailingProvider())
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        status, _, body = _get(f"http://127.0.0.1:{server.server_port}/spans.json")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert status == 503
    assert json.loads(body)["error"] == "unreachable"


def test_dashboard_command_no_open(invoke, monkeypatch: pytest.MonkeyPatch) -> None:
    opened: list[str] = []
    monkeypatch.setattr(dashboard_module.webbrowser, "open", opened.append)
    monkeypatch.setattr(_DashboardServer, "serve_forever", lambda self: None)
    result = invoke("dashboard", "--no-open")
    assert result.exit_code == 0
    assert "http://127.0.0.1:" in result.output
    assert opened == []


def test_dashboard_command_opens_browser(invoke, monkeypatch: pytest.MonkeyPatch) -> None:
    opened: list[str] = []
    monkeypatch.setattr(dashboard_module.webbrowser, "open", opened.append)
    monkeypatch.setattr(_DashboardServer, "serve_forever", lambda self: None)
    result = invoke("dashboard")
    assert result.exit_code == 0
    (url,) = opened
    assert url.startswith("http://127.0.0.1:")


def test_dashboard_command_keyboard_interrupt(invoke, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(dashboard_module.webbrowser, "open", lambda url: None)
    monkeypatch.setattr(
        _DashboardServer, "serve_forever", lambda self: (_ for _ in ()).throw(KeyboardInterrupt)
    )
    result = invoke("dashboard")
    assert result.exit_code == 0
    assert "Stopped" in result.output


def test_dashboard_command_bad_port(invoke) -> None:
    result = invoke("dashboard", "--port", "99999")
    assert result.exit_code == 2
