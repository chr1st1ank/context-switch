"""Tests for projects/tags management groups."""

import json


def test_projects_empty(invoke) -> None:
    result = invoke("projects")
    assert result.exit_code == 0
    assert "No projects" in result.output


def test_projects_add_and_list(invoke) -> None:
    result = invoke("projects", "add", "apollo11")
    assert result.exit_code == 0
    assert "created project apollo11" in result.output
    result = invoke("projects")
    assert "apollo11" in result.output


def test_projects_add_duplicate_errors(invoke) -> None:
    invoke("projects", "add", "Apollo11")
    result = invoke("projects", "add", "apollo11")
    assert result.exit_code == 1
    assert "duplicate" in result.output


def test_projects_rename(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "rename", "apollo11", "apollo12")
    assert result.exit_code == 0
    assert "renamed project apollo11 to apollo12" in result.output
    result = invoke("projects")
    assert "apollo12" in result.output


def test_projects_rename_unknown_errors(invoke) -> None:
    result = invoke("projects", "rename", "nosuch", "x")
    assert result.exit_code == 1
    assert "unknown project" in result.output


def test_projects_rename_to_existing_errors(invoke) -> None:
    invoke("projects", "add", "a")
    invoke("projects", "add", "b")
    result = invoke("projects", "rename", "a", "B")
    assert result.exit_code == 1


def test_projects_archive_unarchive(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "archive", "apollo11")
    assert result.exit_code == 0
    assert "archived project apollo11" in result.output
    result = invoke("projects")
    assert "apollo11" not in result.output
    result = invoke("projects", "--all")
    assert "apollo11 (archived)" in result.output
    result = invoke("projects", "unarchive", "apollo11")
    assert "unarchived project apollo11" in result.output
    result = invoke("projects")
    assert "apollo11" in result.output


def test_projects_json(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "--json")
    payload = json.loads(result.output)
    assert payload[0]["name"] == "apollo11"
    assert payload[0]["archived"] is False


def test_tags_lifecycle(invoke) -> None:
    assert "No tags" in invoke("tags").output
    invoke("tags", "add", "focus")
    assert "focus" in invoke("tags").output
    invoke("tags", "rename", "focus", "deep-focus")
    assert "deep-focus" in invoke("tags").output
    invoke("tags", "archive", "deep-focus")
    assert "deep-focus" not in invoke("tags").output
    assert "(archived)" in invoke("tags", "--all").output
    invoke("tags", "unarchive", "deep-focus")
    assert "deep-focus" in invoke("tags").output


def test_tags_unknown_errors(invoke) -> None:
    result = invoke("tags", "archive", "nosuch")
    assert result.exit_code == 1
    assert "unknown tag" in result.output


def test_tags_json(invoke) -> None:
    invoke("tags", "add", "focus")
    payload = json.loads(invoke("tags", "--json").output)
    assert payload[0]["name"] == "focus"


def test_archived_tag_rejected_on_start(invoke) -> None:
    invoke("tags", "add", "focus")
    invoke("tags", "archive", "focus")
    result = invoke("start", "proj", "+focus")
    assert result.exit_code == 1
    assert "archived" in result.output
