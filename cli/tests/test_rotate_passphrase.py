"""Tests for `cosw rotate-passphrase` using a fake S3 provider."""

import json
import stat
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

import pytest
from click.testing import Result
from contextswitch_core import Logbook, StorageError
from cosw import cli

Invoke = Callable[..., Result]


@dataclass
class FakeSnapshot:
    logbook: Logbook = field(default_factory=Logbook)


class FakeS3:
    def __init__(self, read_error: str | None = None, rotate_error: str | None = None) -> None:
        self.rotated_to: str | None = None
        self.read_error = read_error
        self.rotate_error = rotate_error

    def read(self) -> FakeSnapshot:
        if self.read_error is not None:
            raise StorageError(self.read_error)
        return FakeSnapshot()

    def rotate_passphrase(self, new_passphrase: str) -> None:
        if self.rotate_error is not None:
            raise StorageError(self.rotate_error)
        self.rotated_to = new_passphrase


@dataclass
class S3Setup:
    fake: FakeS3
    backups: Path


@pytest.fixture
def s3_setup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> S3Setup:
    config = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('[storage]\nprovider = "s3"\nbucket = "b"\nregion = "r"\n')
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg-data"))
    monkeypatch.delenv("COSW_NEW_PASSPHRASE", raising=False)
    fake = FakeS3()
    monkeypatch.setattr(cli, "get_provider", lambda ctx: fake)
    return S3Setup(fake, tmp_path / "xdg-data" / "context-switch" / "backups")


def test_rotates_after_writing_private_backup(s3_setup: S3Setup, invoke: Invoke) -> None:
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code == 0, result.output
    assert s3_setup.fake.rotated_to == "new"
    (backup,) = s3_setup.backups.glob("logbook-*.json")
    assert json.loads(backup.read_text())["schema_version"]
    assert stat.S_IMODE(backup.stat().st_mode) == 0o600
    assert "backup written" in result.output


def test_new_passphrase_from_command_and_env(
    s3_setup: S3Setup, invoke: Invoke, monkeypatch: pytest.MonkeyPatch
) -> None:
    assert invoke("rotate-passphrase", "--new-passphrase-command", "echo viacmd").exit_code == 0
    assert s3_setup.fake.rotated_to == "viacmd"
    monkeypatch.setenv("COSW_NEW_PASSPHRASE", "viaenv")
    assert invoke("rotate-passphrase").exit_code == 0
    assert s3_setup.fake.rotated_to == "viaenv"


def test_empty_new_passphrase_rejected(s3_setup: S3Setup, invoke: Invoke) -> None:
    result = invoke("rotate-passphrase", "--new-passphrase-command", "true")
    assert result.exit_code != 0
    assert s3_setup.fake.rotated_to is None
    assert not s3_setup.backups.exists()


def test_dry_run_changes_nothing(s3_setup: S3Setup, invoke: Invoke) -> None:
    result = invoke("rotate-passphrase", "--dry-run")
    assert result.exit_code == 0
    assert s3_setup.fake.rotated_to is None
    assert not s3_setup.backups.exists()


def test_backup_failure_aborts_rotation(
    s3_setup: S3Setup, invoke: Invoke, monkeypatch: pytest.MonkeyPatch
) -> None:
    def boom(path: Path, text: str) -> None:
        raise OSError("disk full")

    monkeypatch.setattr(cli, "_write_private", boom)
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "nothing changed" in result.output
    assert s3_setup.fake.rotated_to is None


def test_rotation_failure_reported(
    s3_setup: S3Setup, invoke: Invoke, monkeypatch: pytest.MonkeyPatch
) -> None:
    failing = FakeS3(rotate_error="conflict")
    monkeypatch.setattr(cli, "get_provider", lambda ctx: failing)
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "rotation failed" in result.output


def test_read_failure_reported(
    s3_setup: S3Setup, invoke: Invoke, monkeypatch: pytest.MonkeyPatch
) -> None:
    failing = FakeS3(read_error="wrong passphrase")
    monkeypatch.setattr(cli, "get_provider", lambda ctx: failing)
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "wrong passphrase" in result.output


def test_requires_s3_provider(invoke: Invoke) -> None:
    result = invoke("rotate-passphrase")
    assert result.exit_code != 0
    assert "requires the 's3'" in result.output
