"""Project, client project and tag management: ``cosw projects``, ``cosw
client-projects`` and ``cosw tags`` groups."""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any, Literal

import click
from contextswitch_core import ClientProject, Logbook, Project, Tag

from cosw import completers
from cosw.cli import main
from cosw.core import (
    read_logbook,
    require_client_project,
    require_project,
    require_tag,
    resolve_client_project_id,
    transact,
)
from cosw.timeparse import utcnow

Kind = Literal["project", "tag", "client_project"]
_LABEL = {"project": "project", "tag": "tag", "client_project": "client project"}


def _items(logbook: Logbook, kind: Kind) -> list[Project] | list[Tag] | list[ClientProject]:
    if kind == "project":
        return logbook.projects()
    if kind == "tag":
        return logbook.tags()
    return logbook.client_projects()


def _require(logbook: Logbook, kind: Kind, name: str) -> Project | Tag | ClientProject:
    if kind == "project":
        return require_project(logbook, name)
    if kind == "tag":
        return require_tag(logbook, name)
    return require_client_project(logbook, name)


def _add(
    logbook: Logbook,
    kind: Kind,
    name: str,
    at,
    client: str | None = None,
    client_project: str | None = None,
) -> str:
    if kind == "project":
        pid = logbook.add_project(name, at, client)
        if client_project is not None:
            logbook.set_project_client_project(
                pid, resolve_client_project_id(logbook, client_project), at
            )
        return pid
    if kind == "client_project":
        return logbook.add_client_project(name, at)
    return logbook.add_tag(name, at)


def _rename(logbook: Logbook, kind: Kind, item_id: str, name: str, at) -> None:
    if kind == "project":
        logbook.rename_project(item_id, name, at)
    elif kind == "client_project":
        logbook.rename_client_project(item_id, name, at)
    else:
        logbook.rename_tag(item_id, name, at)


def _set_archived(logbook: Logbook, kind: Kind, item_id: str, archived: bool, at) -> None:
    if kind == "project":
        logbook.set_project_archived(item_id, archived, at)
    elif kind == "client_project":
        logbook.set_client_project_archived(item_id, archived, at)
    else:
        logbook.set_tag_archived(item_id, archived, at)


def _client_project_label(logbook: Logbook, client_project_id: str | None) -> str | None:
    if client_project_id is None:
        return None
    client_project = logbook.client_project(client_project_id)
    return client_project.name if client_project is not None else None


def _list(ctx: click.Context, kind: Kind, show_all: bool, as_json: bool) -> None:
    logbook = read_logbook(ctx)
    items = [i for i in _items(logbook, kind) if show_all or not i.archived]
    items.sort(key=lambda i: i.name.casefold())
    if as_json:
        payload = []
        for i in items:
            entry: dict[str, object] = {
                "id": i.id,
                "name": i.name,
                "archived": i.archived,
            }
            if isinstance(i, Project):
                entry["client"] = i.client
                entry["client_project"] = _client_project_label(logbook, i.client_project_id)
            payload.append(entry)
        click.echo(json.dumps(payload))
        return
    if not items:
        click.echo(f"No {_LABEL[kind]}s.")
        return
    for item in items:
        notes = []
        if isinstance(item, Project):
            if item.client is not None:
                notes.append(item.client)
            label = _client_project_label(logbook, item.client_project_id)
            if label is not None:
                notes.append(f"client project {label}")
        if item.archived:
            notes.append("archived")
        suffix = f" ({', '.join(notes)})" if notes else ""
        click.echo(f"{item.name}{suffix}")


def _register(group: click.Group, kind: Kind) -> None:
    def add_impl(
        ctx: click.Context,
        name: str,
        client: str | None = None,
        client_project: str | None = None,
    ) -> None:
        """Create a new entry."""
        transact(ctx, lambda logbook: _add(logbook, kind, name, utcnow(), client, client_project))
        click.echo(f"created {_LABEL[kind]} {name}")

    command: Callable[..., Any] = click.pass_context(add_impl)
    command = click.argument("name")(command)
    if kind == "project":
        command = click.option(
            "--client-project",
            "client_project",
            metavar="NAME",
            help="Bill the project to this existing client project.",
            shell_complete=completers.names("client_project"),
        )(command)
        command = click.option(
            "--client", "client", metavar="CLIENT", help="Optional client label."
        )(command)
    group.command("add")(command)

    @group.command("rename")
    @click.argument("old", shell_complete=completers.names(kind))
    @click.argument("new")
    @click.pass_context
    def rename_cmd(ctx: click.Context, old: str, new: str) -> None:
        """Rename an entry; history keeps pointing at the same identity."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, old)
            _rename(logbook, kind, item.id, new, utcnow())

        transact(ctx, mutate)
        click.echo(f"renamed {_LABEL[kind]} {old} to {new}")

    @group.command("archive")
    @click.argument("name", shell_complete=completers.names(kind))
    @click.pass_context
    def archive_cmd(ctx: click.Context, name: str) -> None:
        """Hide an entry from pickers while preserving its history."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, name)
            _set_archived(logbook, kind, item.id, True, utcnow())

        transact(ctx, mutate)
        click.echo(f"archived {_LABEL[kind]} {name}")

    @group.command("unarchive")
    @click.argument("name", shell_complete=completers.names(kind, archived=True))
    @click.pass_context
    def unarchive_cmd(ctx: click.Context, name: str) -> None:
        """Make an archived entry selectable again."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, name)
            _set_archived(logbook, kind, item.id, False, utcnow())

        transact(ctx, mutate)
        click.echo(f"unarchived {_LABEL[kind]} {name}")


@main.group(invoke_without_command=True)
@click.option("--all", "show_all", is_flag=True, help="Include archived entries.")
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def projects(ctx: click.Context, show_all: bool, as_json: bool) -> None:
    """List and manage projects."""
    if ctx.invoked_subcommand is None:
        _list(ctx, "project", show_all, as_json)


@main.group(invoke_without_command=True)
@click.option("--all", "show_all", is_flag=True, help="Include archived entries.")
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def tags(ctx: click.Context, show_all: bool, as_json: bool) -> None:
    """List and manage tags."""
    if ctx.invoked_subcommand is None:
        _list(ctx, "tag", show_all, as_json)


_register(projects, "project")
_register(tags, "tag")


@main.group("client-projects", invoke_without_command=True)
@click.option("--all", "show_all", is_flag=True, help="Include archived entries.")
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def client_projects(ctx: click.Context, show_all: bool, as_json: bool) -> None:
    """List and manage client projects (the identifiers your customers use)."""
    if ctx.invoked_subcommand is None:
        _list(ctx, "client_project", show_all, as_json)


_register(client_projects, "client_project")


@projects.command("client")
@click.argument("name", shell_complete=completers.names("project"))
@click.argument("value", required=False, metavar="CLIENT")
@click.option("--clear", is_flag=True, help="Remove the client label.")
@click.pass_context
def project_client_cmd(ctx: click.Context, name: str, value: str | None, clear: bool) -> None:
    """Set or clear a project's client label."""
    if clear and value is not None:
        raise click.UsageError("--clear takes no CLIENT value")
    if value is None and not clear:
        raise click.UsageError("missing CLIENT (or pass --clear)")
    new_client = None if clear else value

    def mutate(logbook: Logbook) -> None:
        project = require_project(logbook, name)
        logbook.set_project_client(project.id, new_client, utcnow())

    transact(ctx, mutate)
    if clear:
        click.echo(f"cleared client of project {name}")
    else:
        click.echo(f"project {name} client set to {new_client}")


@projects.command("client-project")
@click.argument("name", shell_complete=completers.names("project"))
@click.argument(
    "value",
    required=False,
    metavar="CLIENT_PROJECT",
    shell_complete=completers.names("client_project"),
)
@click.option("--clear", is_flag=True, help="Remove the client project.")
@click.pass_context
def project_client_project_cmd(
    ctx: click.Context, name: str, value: str | None, clear: bool
) -> None:
    """Set or clear the client project a project is billed to."""
    if clear and value is not None:
        raise click.UsageError("--clear takes no CLIENT_PROJECT value")
    if value is None and not clear:
        raise click.UsageError("missing CLIENT_PROJECT (or pass --clear)")

    def mutate(logbook: Logbook) -> None:
        project = require_project(logbook, name)
        target = None if clear else resolve_client_project_id(logbook, value or "")
        logbook.set_project_client_project(project.id, target, utcnow())

    transact(ctx, mutate)
    if clear:
        click.echo(f"cleared client project of project {name}")
    else:
        click.echo(f"project {name} client project set to {value}")
