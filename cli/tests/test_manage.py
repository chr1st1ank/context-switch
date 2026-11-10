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


def test_projects_add_with_client(invoke) -> None:
    result = invoke("projects", "add", "apollo11", "--client", "NASA")
    assert result.exit_code == 0
    assert "apollo11 (NASA)" in invoke("projects").output


def test_projects_client_set_and_clear(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "client", "apollo11", "NASA")
    assert result.exit_code == 0
    assert "client set to NASA" in result.output
    assert "apollo11 (NASA)" in invoke("projects").output
    result = invoke("projects", "client", "apollo11", "--clear")
    assert result.exit_code == 0
    assert "cleared client of project apollo11" in result.output
    assert "apollo11 (NASA)" not in invoke("projects").output


def test_projects_client_errors(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "client", "apollo11")
    assert result.exit_code == 2
    result = invoke("projects", "client", "apollo11", "NASA", "--clear")
    assert result.exit_code == 2
    result = invoke("projects", "client", "nosuch", "NASA")
    assert result.exit_code == 1
    assert "unknown project" in result.output


def test_projects_json_includes_client(invoke) -> None:
    invoke("projects", "add", "apollo11", "--client", "NASA")
    invoke("projects", "add", "personal")
    payload = json.loads(invoke("projects", "--json").output)
    by_name = {p["name"]: p for p in payload}
    assert by_name["apollo11"]["client"] == "NASA"
    assert by_name["personal"]["client"] is None


def test_client_projects_lifecycle(invoke) -> None:
    assert "No client projects" in invoke("client-projects").output
    assert invoke("client-projects", "add", "AP-11").output == "created client project AP-11\n"
    assert invoke("client-projects", "add", "ap-11").exit_code == 1
    invoke("client-projects", "rename", "AP-11", "AP-12")
    assert invoke("client-projects").output == "AP-12\n"
    assert invoke("client-projects", "archive", "AP-12").output == "archived client project AP-12\n"
    assert "No client projects" in invoke("client-projects").output
    assert invoke("client-projects", "--all").output == "AP-12 (archived)\n"
    invoke("client-projects", "unarchive", "AP-12")
    assert json.loads(invoke("client-projects", "-j").output)[0]["name"] == "AP-12"


def test_projects_client_project_set_and_clear(invoke) -> None:
    invoke("client-projects", "add", "AP-11")
    invoke("client-projects", "add", "AP-12")
    assert invoke("projects", "add", "apollo11", "--client-project", "ap-11").exit_code == 0

    def client_project() -> object:
        (entry,) = json.loads(invoke("projects", "-j").output)
        return entry["client_project"]

    assert client_project() == "AP-11"
    result = invoke("projects", "client-project", "apollo11", "--clear")
    assert result.exit_code == 0
    assert "cleared client project of project apollo11" in result.output
    assert client_project() is None
    result = invoke("projects", "client-project", "apollo11", "AP-12")
    assert "project apollo11 client project set to AP-12" in result.output
    assert client_project() == "AP-12"


def test_projects_list_shows_client_project(invoke) -> None:
    invoke("client-projects", "add", "AP-11")
    invoke("projects", "add", "apollo11", "--client", "NASA", "--client-project", "AP-11")
    assert "apollo11 (NASA, client project AP-11)" in invoke("projects").output


def test_projects_client_project_errors(invoke) -> None:
    invoke("projects", "add", "apollo11")
    result = invoke("projects", "client-project", "apollo11")
    assert result.exit_code == 2
    assert "missing CLIENT_PROJECT" in result.output
    result = invoke("projects", "client-project", "apollo11", "AP-11", "--clear")
    assert result.exit_code == 2
    assert "--clear takes no CLIENT_PROJECT value" in result.output
    result = invoke("projects", "client-project", "apollo11", "AP-11")
    assert result.exit_code == 1
    assert "unknown client project: AP-11" in result.output


def test_projects_add_with_unknown_client_project_creates_nothing(invoke) -> None:
    result = invoke("projects", "add", "apollo11", "--client-project", "AP-11")
    assert result.exit_code == 1
    assert "No projects" in invoke("projects").output


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
