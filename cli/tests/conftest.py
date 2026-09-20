"""Shared test fixtures."""

import os
import time
from collections.abc import Callable, Iterator
from pathlib import Path

import pytest
from click.testing import CliRunner, Result
from cosw.cli import main


@pytest.fixture(autouse=True)
def _isolated_config(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """Keep a real user config file from leaking into tests."""
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "xdg-config"))
    monkeypatch.delenv("COSW_CONFIG", raising=False)


@pytest.fixture
def berlin_tz() -> Iterator[None]:
    """Pin the process timezone to a DST-observing zone (CET/CEST)."""
    old = os.environ.get("TZ")
    os.environ["TZ"] = "Europe/Berlin"
    time.tzset()
    yield
    if old is None:
        os.environ.pop("TZ", None)
    else:
        os.environ["TZ"] = old
    time.tzset()


@pytest.fixture
def data_file(tmp_path: Path) -> Path:
    """An isolated canonical data file per test."""
    return tmp_path / "data.json"


@pytest.fixture
def invoke(data_file: Path) -> Callable[..., Result]:
    """Invoke the CLI against the isolated data file."""
    runner = CliRunner()

    def run(*args: str, input: str | None = None) -> Result:
        return runner.invoke(
            main,
            list(args),
            input=input,
            env={"COSW_DATA_FILE": str(data_file)},
        )

    return run
