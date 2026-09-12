#!/usr/bin/env python3
"""Simulate realistic working time data for context-switch.

Records DAYS days of plausible working time via `cosw add`, starting today
and going into the past. Weekdays only, 8h days with breaks; each block is
50% coding / 25% meetings / 25% orga.
"""

import argparse
import random
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta
from pathlib import Path


@dataclass
class BlockType:
    """A work block category with duration range and tag pool."""

    name: str
    projects: list[str]
    tags: list[str]
    min_duration: int
    max_duration: int
    tag_probability: float = 1.0


# Block type definitions: 50% coding / 25% meetings / 25% orga
BLOCK_TYPES = [
    BlockType(
        "coding",
        ["api-service", "web-app", "data-pipeline"],
        ["feature", "bugfix", "refactor", "review", "tests"],
        45,
        135,
        tag_probability=0.5,
    ),
    BlockType(
        "meetings",
        ["meetings"],
        ["standup", "planning", "review", "sync", "retro"],
        20,
        70,
        tag_probability=1.0,
    ),
    BlockType(
        "orga",
        ["orga", "presentation"],
        ["docs", "email", "admin", "slides"],
        30,
        90,
        tag_probability=1 / 3,
    ),
]

# Probabilities (normalized to 100) that sum to 50/25/25
BLOCK_PROBABILITIES = [50, 25, 25]

# Find cosw on PATH, else use `uv run --package cosw cosw`
COSW_CMD = ["cosw"] if shutil.which("cosw") else ["uv", "run", "--package", "cosw", "cosw"]


def ts(day: date, minute: int) -> str:
    """Convert day + minute-of-day to ISO 8601 timestamp."""
    return datetime.combine(day, time(minute // 60, minute % 60)).isoformat()


def add_span(day: date, project: str | None, start: int, end: int, tag: str | None = None) -> None:
    """Record a span via cosw add."""
    args = (
        ([project] if project else [])
        + ([f"+{tag}"] if tag else [])
        + ["--from", ts(day, start), "--to", ts(day, end)]
    )
    subprocess.run(
        [*COSW_CMD, "add", *args],
        cwd=Path(__file__).parent.parent,
        stdout=subprocess.DEVNULL,
        check=True,
    )


def generate_block() -> BlockType:
    """Pick a block type weighted by BLOCK_PROBABILITIES."""
    return random.choices(BLOCK_TYPES, weights=BLOCK_PROBABILITIES, k=1)[0]


def simulate_day(day: date, is_today: bool) -> int:
    """Simulate a workday, returning the number of spans recorded."""
    cursor = 8 * 60 + random.randrange(91)  # start 08:00–09:30
    end = cursor + 480  # 8h workday
    if is_today:
        now = datetime.now()
        end = min(end, now.hour * 60 + now.minute - 10)  # don't record into the future

    spans = 0
    lunch_done = False

    while cursor < end - 15:
        # Lunch break (30–60m), half recorded as unassigned
        if not lunch_done and cursor >= 12 * 60 + 15:
            gap = 30 + random.randrange(31)
            if random.random() < 0.5:
                add_span(day, None, cursor, cursor + gap)
                spans += 1
            cursor += gap
            lunch_done = True
            continue

        # Short break / gap (10–30m), half recorded as unassigned
        if random.random() < 0.15:
            gap = 10 + random.randrange(21)
            if random.random() < 0.5:
                add_span(day, None, cursor, cursor + gap)
                spans += 1
            cursor += gap
            continue

        # Generate a work block
        block = generate_block()
        dur = block.min_duration + random.randrange(block.max_duration - block.min_duration + 1)
        dur = min(dur, end - cursor)

        project = random.choice(block.projects)
        tag = None
        if random.random() < block.tag_probability:
            tag = random.choice(block.tags)

        add_span(day, project, cursor, cursor + dur, tag)
        spans += 1
        cursor += dur

    return spans


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Simulate realistic working time data for context-switch."
    )
    parser.add_argument("days", type=int, help="Number of days to simulate (going into the past)")
    args = parser.parse_args()

    today = date.today()
    for d in range(args.days - 1, -1, -1):
        day = today - timedelta(days=d)
        if day.weekday() >= 5:  # skip weekends
            continue
        spans = simulate_day(day, is_today=(d == 0))
        print(f"{day}: {spans} spans")


if __name__ == "__main__":
    main()
