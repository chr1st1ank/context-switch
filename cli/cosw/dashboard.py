"""Local reporting dashboard: serve the logbook over loopback HTTP.

``cosw dashboard`` starts a throwaway HTTP server bound to localhost, opens
the browser on the single-page dashboard shipped in ``cosw/assets``, and
serves a fresh span export on every request so a reload reflects the latest
timer state. Decrypted data never leaves the loopback interface and the
server dies with the process.
"""

from __future__ import annotations

import json
import webbrowser
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib import resources
from typing import Any, Protocol, override

import click
from contextswitch_core import Logbook, StorageError, StorageSnapshot

from cosw.cli import main
from cosw.core import get_provider, read_logbook, sorted_spans, span_to_json
from cosw.timeparse import utcnow

_ASSETS = resources.files("cosw") / "assets"


class _Reader(Protocol):
    """Anything that can read a ``StorageSnapshot`` — the storage providers."""

    def read(self) -> StorageSnapshot: ...


def spans_payload(logbook: Logbook, now: datetime) -> dict[str, Any]:
    """Serialize the logbook for the dashboard's ``/spans.json`` endpoint.

    The running span is included with ``active: true`` and ``seconds``
    clipped at ``now``; ``stopped_at`` stays ``None``.
    """
    return {
        "generatedAt": now.isoformat(),
        "spans": [span_to_json(logbook, span, now) for span in sorted_spans(logbook)],
    }


class _DashboardHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path in ("/", "/dashboard.html"):
            self._send_asset("dashboard.html", "text/html; charset=utf-8")
        elif self.path == "/chart.umd.min.js":
            self._send_asset("chart.umd.min.js", "text/javascript; charset=utf-8")
        elif self.path == "/spans.json":
            self._send_spans()
        else:
            self.send_error(404)

    def _send_asset(self, name: str, content_type: str) -> None:
        try:
            body = (_ASSETS / name).read_bytes()
        except FileNotFoundError:
            self.send_error(404)
            return
        self._send(200, content_type, body)

    def _send_spans(self) -> None:
        assert isinstance(self.server, _DashboardServer)
        try:
            snap = self.server.provider.read()
        except StorageError as e:
            self._send_json(503, {"error": str(e)})
            return
        self._send_json(200, spans_payload(snap.logbook, utcnow()))

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        self._send(status, "application/json; charset=utf-8", json.dumps(payload).encode())

    def _send(self, status: int, content_type: str, body: bytes) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    @override
    def log_message(self, format: str, *args: Any) -> None:
        """Silence per-request logging; the terminal stays clean."""


class _DashboardServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, server_address: tuple[str, int], provider: _Reader) -> None:
        super().__init__(server_address, _DashboardHandler)
        self.provider = provider


@main.command()
@click.option(
    "--port",
    type=click.IntRange(0, 65535),
    default=0,
    metavar="N",
    help="Port to listen on; 0 picks an ephemeral port (default).",
)
@click.option(
    "--open/--no-open",
    "open_browser",
    default=True,
    help="Open the dashboard in a browser (default: open).",
)
@click.pass_context
def dashboard(ctx: click.Context, port: int, open_browser: bool) -> None:
    """Open an interactive time dashboard in the browser."""
    provider = get_provider(ctx)
    read_logbook(ctx)  # surface storage/decryption errors before serving
    server = _DashboardServer(("127.0.0.1", port), provider)
    url = f"http://127.0.0.1:{server.server_port}/"
    click.echo(f"Serving the dashboard at {url} — Ctrl+C to stop.")
    if open_browser:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        click.echo("Stopped.")
    finally:
        server.server_close()
