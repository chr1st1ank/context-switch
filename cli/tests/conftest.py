"""Shared test fixtures."""

from collections.abc import Callable
from pathlib import Path

import pytest
from click.testing import CliRunner, Result
from cosw.cli import main


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
