"""Tests for `cosw rotate-passphrase` using a fake S3 provider."""

import json
import stat
from pathlib import Path

import pytest
from contextswitch_core import Logbook, StorageError
from cosw import cli


class FakeS3:
    def __init__(self, fail_rotate: bool = False) -> None:
        self.rotated_to: str | None = None
        self.fail_rotate = fail_rotate

    def read(self):
        class Snap:
            logbook = Logbook()

        return Snap()

    def rotate_passphrase(self, new_passphrase: str) -> None:
        if self.fail_rotate:
            raise StorageError("conflict")
        self.rotated_to = new_passphrase


@pytest.fixture
def s3_setup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch, invoke):
    config = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    config.parent.mkdir(parents=True)
    config.write_text('[storage]\nprovider = "s3"\nbucket = "b"\nregion = "r"\n')
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg-data"))
    monkeypatch.delenv("COSW_NEW_PASSPHRASE", raising=False)
    fake = FakeS3()
    monkeypatch.setattr(cli, "get_provider", lambda ctx: fake)
    return fake, tmp_path / "xdg-data" / "context-switch" / "backups"


def test_rotates_after_writing_private_backup(s3_setup, invoke) -> None:
    fake, backups = s3_setup
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code == 0, result.output
    assert fake.rotated_to == "new"
    (backup,) = backups.glob("logbook-*.json")
    assert json.loads(backup.read_text())["schema_version"]
    assert stat.S_IMODE(backup.stat().st_mode) == 0o600
    assert "backup written" in result.output


def test_new_passphrase_from_command_and_env(s3_setup, invoke, monkeypatch) -> None:
    fake, _ = s3_setup
    assert invoke("rotate-passphrase", "--new-passphrase-command", "echo viacmd").exit_code == 0
    assert fake.rotated_to == "viacmd"
    monkeypatch.setenv("COSW_NEW_PASSPHRASE", "viaenv")
    assert invoke("rotate-passphrase").exit_code == 0
    assert fake.rotated_to == "viaenv"


def test_empty_new_passphrase_rejected(s3_setup, invoke) -> None:
    fake, backups = s3_setup
    result = invoke("rotate-passphrase", "--new-passphrase-command", "true")
    assert result.exit_code != 0
    assert fake.rotated_to is None
    assert not backups.exists()


def test_dry_run_changes_nothing(s3_setup, invoke) -> None:
    fake, backups = s3_setup
    result = invoke("rotate-passphrase", "--dry-run")
    assert result.exit_code == 0
    assert fake.rotated_to is None
    assert not backups.exists()


def test_backup_failure_aborts_rotation(s3_setup, invoke, monkeypatch) -> None:
    fake, _ = s3_setup

    def boom(path, text):
        raise OSError("disk full")

    monkeypatch.setattr(cli, "_write_private", boom)
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "nothing changed" in result.output
    assert fake.rotated_to is None


def test_rotation_failure_reported(s3_setup, invoke, monkeypatch) -> None:
    monkeypatch.setattr(cli, "get_provider", lambda ctx: FakeS3(fail_rotate=True))
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "rotation failed" in result.output


def test_read_failure_reported(s3_setup, invoke, monkeypatch) -> None:
    class Bad(FakeS3):
        def read(self):
            raise StorageError("wrong passphrase")

    monkeypatch.setattr(cli, "get_provider", lambda ctx: Bad())
    result = invoke("rotate-passphrase", input="new\nnew\n")
    assert result.exit_code != 0
    assert "wrong passphrase" in result.output


def test_requires_s3_provider(invoke) -> None:
    result = invoke("rotate-passphrase")
    assert result.exit_code != 0
    assert "requires the 's3'" in result.output
